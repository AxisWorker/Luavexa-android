//! Room for the soft keyboard. winit never resizes an Android window for
//! the keyboard (the manifest's `adjustResize` moves NativeActivity's
//! content rect; the surface egui draws on stays the whole screen), so the
//! field being typed into sat behind it. While a text field has the focus
//! the keyboard's height is read from the window insets
//! ([`jni::keyboard_height_later`](super::jni::keyboard_height_later)) and
//!
//! * the editor's panels are laid out above it ([`Keyboard::shrink`] takes
//!   that much off the root `Ui` in `App::ui`, so the dock, the 3D view and
//!   the status bar move up as `adjustResize` would move an Android view);
//! * it is egui's bottom safe-area inset, so windows, menus and popups keep
//!   to the area above it too ([`Keyboard::before_frame`]).
//!
//! The field's own scrolling brings the caret into view at the next
//! keystroke (egui's `TextEdit` scrolls to its cursor when the text or the
//! selection changes). The height is asked of the Java main thread every
//! [`POLL`] while a text field has the focus or the keyboard was up at the
//! last look, without waiting for the answer (it comes a frame or two
//! later); at other times nothing runs. egui keeps the last safe-area
//! inset it was given, so the inset is set every frame, zero included.
//!
//! The keyboard is shown and hidden by egui-winit alone, from egui's IME
//! output: up while a text field has the focus. The user hiding it (Back,
//! the keyboard's own hide key) is invisible to the app: the field keeps
//! the focus and nothing would bring the keyboard back but another tap —
//! or a `request_focus` of the field, which egui counts as an IME
//! interruption (egui-winit hides and shows the keyboard again at once).
//! So the editor's fields ask for the focus only while they lack it
//! (properties.rs, explorer.rs), and when the keyboard goes down while a
//! field the keyboard was up for keeps the focus for [`HIDDEN_FOR`], the
//! field's focus is released here: the edit ends (its text kept) as the
//! user meant, and a tap starts it again.

use std::time::{Duration, Instant};

use egui::{SafeAreaInsets, epaint::MarginF32};

use super::jni::KeyboardAnswer;

/// How often the height is read while it matters.
const POLL: Duration = Duration::from_millis(100);
/// The keyboard down this long while the field it was up for keeps the
/// focus: the user hid it (longer than its slide out, and than the hide
/// and show of a focus move between fields).
const HIDDEN_FOR: Duration = Duration::from_millis(300);
/// A question the main thread hasn't answered in this long is given up
/// on (it is busy with a pause or a configuration change) and asked again.
const GIVE_UP: Duration = Duration::from_secs(1);
/// Points the panels keep however tall the keyboard is.
const MIN_HEIGHT: f32 = 120.0;

#[derive(Default)]
pub struct Keyboard {
    polled: Option<Instant>,
    /// The question out to the main thread, and when it was asked.
    asked: Option<(Instant, KeyboardAnswer)>,
    /// The keyboard's height in pixels (0: down).
    height_px: i32,
    /// The text field the keyboard was last seen up for.
    up_for: Option<egui::Id>,
    /// Since when it has been down with that field still focused.
    down_since: Option<Instant>,
}

/// The focused widget when it is a text field (egui keeps a
/// `TextEditState` for every text field it has drawn).
pub fn focused_text(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|m| m.focused()).filter(|id| egui::TextEdit::load_state(ctx, *id).is_some())
}

impl Keyboard {
    /// In `App::raw_input_hook`: reads the keyboard's height when it may
    /// have changed and hands it to egui as the bottom safe-area inset.
    pub fn before_frame(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if focused_text(ctx).is_some() || self.height_px > 0 {
            let now = Instant::now();
            // The last question's answer, once it has come.
            if let Some((when, slot)) = &self.asked {
                let when = *when;
                let answer = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
                match answer {
                    Some(answer) => {
                        self.asked = None;
                        let height = match answer {
                            Ok(h) => h,
                            Err(e) => {
                                log::warn!("keyboard height: {e}");
                                0
                            }
                        };
                        if height != self.height_px {
                            log::info!("soft keyboard: {height} px");
                            self.height_px = height;
                        }
                    }
                    None if now.duration_since(when) >= GIVE_UP => self.asked = None,
                    None => {}
                }
            }
            if self.asked.is_none() && self.polled.is_none_or(|t| now.duration_since(t) >= POLL) {
                self.polled = Some(now);
                self.asked = Some((now, super::jni::keyboard_height_later()));
            }
            // The keyboard slides in after the focus (and out after it):
            // keep looking while it matters, input or none.
            ctx.request_repaint_after(POLL);
            self.hidden_by_user(ctx, now);
        } else {
            self.polled = None;
            self.asked = None;
            self.up_for = None;
            self.down_since = None;
        }
        // Every frame, down (zero) or up: egui keeps the last inset given.
        let bottom = self.height(ctx);
        raw.safe_area_insets = Some(SafeAreaInsets(MarginF32 { left: 0.0, right: 0.0, top: 0.0, bottom }));
    }

    /// Releases a text field's focus once the keyboard has been down for
    /// [`HIDDEN_FOR`] with the field the keyboard was up for still
    /// focused (the module comment).
    fn hidden_by_user(&mut self, ctx: &egui::Context, now: Instant) {
        let focused = focused_text(ctx);
        if self.height_px > 0 {
            self.up_for = focused;
            self.down_since = None;
            return;
        }
        match focused {
            Some(id) if self.up_for == Some(id) => {
                let since = *self.down_since.get_or_insert(now);
                if now.duration_since(since) >= HIDDEN_FOR {
                    log::info!("soft keyboard hidden by the user: the text field's focus released");
                    ctx.memory_mut(|m| m.surrender_focus(id));
                    self.up_for = None;
                    self.down_since = None;
                }
            }
            _ => {
                self.up_for = None;
                self.down_since = None;
            }
        }
    }

    /// The keyboard's height in points (0 while it is down).
    pub fn height(&self, ctx: &egui::Context) -> f32 {
        if self.height_px <= 0 {
            return 0.0;
        }
        self.height_px as f32 / ctx.pixels_per_point().max(0.1)
    }

    /// At the start of `App::ui`: the root `Ui` ends above the keyboard, so
    /// every panel laid out in it does too.
    pub fn shrink(&self, ui: &mut egui::Ui) {
        let keyboard = self.height(ui.ctx());
        if keyboard <= 0.0 {
            return;
        }
        let full = ui.max_rect().height();
        ui.set_max_height((full - keyboard).max(MIN_HEIGHT.min(full)));
    }
}
