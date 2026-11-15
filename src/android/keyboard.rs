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

use std::time::{Duration, Instant};

use egui::{SafeAreaInsets, epaint::MarginF32};

use super::jni::KeyboardAnswer;

/// How often the height is read while it matters.
const POLL: Duration = Duration::from_millis(100);
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
        } else {
            self.polled = None;
            self.asked = None;
        }
        // Every frame, down (zero) or up: egui keeps the last inset given.
        let bottom = self.height(ctx);
        raw.safe_area_insets = Some(SafeAreaInsets(MarginF32 { left: 0.0, right: 0.0, top: 0.0, bottom }));
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
