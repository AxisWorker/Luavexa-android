//! Room for the soft keyboard. winit never resizes an Android window for
//! the keyboard (the manifest's `adjustResize` moves NativeActivity's
//! content rect; the surface egui draws on stays the whole screen, and on
//! Android 11+ a window's frame is never resized for the keyboard at all:
//! the keyboard is an inset, `WindowInsets.Type.ime()`, that the app lays
//! itself out around), so the field being typed into sat behind it. While
//! a text field has the focus the keyboard's height is read from the window
//! insets ([`jni::keyboard_height_later`](super::jni::keyboard_height_later))
//! and
//!
//! * the editor's panels are laid out above it ([`Keyboard::shrink`] takes
//!   that much off the root `Ui` in `App::ui`, so the dock, the 3D view and
//!   the status bar move up as `adjustResize` would move an Android view);
//! * it is egui's bottom safe-area inset, so windows, menus and popups keep
//!   to the area above it too ([`Keyboard::before_frame`]);
//! * the frame the room changes, [`reveal`] is true: the field being typed
//!   into scrolls itself into view (properties.rs `text_cell`, the script
//!   editor: [`reveal_cursor`]), as Android scrolls a focused EditText.
//!
//! The height is asked of the Java main thread every [`POLL`] while a text
//! field has the focus or the keyboard was up at the last look, without
//! waiting for the answer (it comes a frame or two later); at other times
//! nothing runs. egui keeps the last safe-area inset it was given, so the
//! inset is set every frame, zero included.
//!
//! The readings are not followed blindly, or the layout flaps. The keyboard
//! is shown and hidden by egui-winit alone, from egui's IME output: up
//! while a text field has the focus, and hidden and shown again at once
//! whenever egui counts an IME interruption — which every `request_focus`
//! is, including the one egui's own `TextEdit` makes for a tap that moves
//! the caret, and the focus moving from one field to the next. Each such
//! blink, and the keyboard's own slide in and out, reads as 0 for a few
//! polls. So while a text field keeps the focus:
//!
//! * the room is made as soon as a height is read (the keyboard slides in
//!   after the focus; its final height is reported from the start);
//! * a 0 reading takes nothing away for [`HIDDEN_FOR`]; past that, with the
//!   field the keyboard was up for still focused, the user hid the keyboard
//!   (Back, its own hide key) — which is invisible to the app otherwise,
//!   and nothing would bring the keyboard back but another tap — so the
//!   field's focus is released here: the edit ends (its text kept) as the
//!   user meant, the room goes with it, and a tap starts it again;
//! * another height (the keyboard's suggestions strip or symbols page)
//!   is taken once it has been read [`STEADY_READS`] times running.
//!
//! With no text field focused the first 0 reading takes the room away:
//! egui-winit has hidden the keyboard. Each change is logged (`adb logcat
//! -s rustudio`: "soft keyboard ...").

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use egui::{SafeAreaInsets, epaint::MarginF32};

use super::jni::KeyboardAnswer;

/// How often the height is read while it matters.
const POLL: Duration = Duration::from_millis(100);
/// The keyboard read as down this long while the field it was up for
/// keeps the focus: the user hid it. Longer than its slide out and back
/// in for an IME interruption (two animations of about 200 ms) and than
/// the hide and show of a focus move between fields.
const HIDDEN_FOR: Duration = Duration::from_millis(600);
/// A new height while the keyboard is up is taken after this many
/// readings running (one reading can be mid-change).
const STEADY_READS: u32 = 2;
/// A question the main thread hasn't answered in this long is given up
/// on (it is busy with a pause or a configuration change) and asked again.
const GIVE_UP: Duration = Duration::from_secs(1);
/// Points the panels keep however tall the keyboard is.
const MIN_HEIGHT: f32 = 120.0;

/// Set for the frame after the room for the keyboard changed (`reveal`).
static CHANGED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct Keyboard {
    polled: Option<Instant>,
    /// The question out to the main thread, and when it was asked.
    asked: Option<(Instant, KeyboardAnswer)>,
    /// The height the panels make room for, in pixels (0: none).
    height_px: i32,
    /// The last height read, and how many times running it was read.
    last_read: Option<(i32, u32)>,
    /// Since when the readings have been 0 while room is made.
    zero_since: Option<Instant>,
    /// The text field the keyboard is up for.
    up_for: Option<egui::Id>,
    /// Since when a text field has had the focus (for the log's timings).
    focus_since: Option<Instant>,
}

/// The focused widget when it is a text field (egui keeps a
/// `TextEditState` for every text field it has drawn).
pub fn focused_text(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|m| m.focused()).filter(|id| egui::TextEdit::load_state(ctx, *id).is_some())
}

/// True during the frame after the room for the keyboard changed: the
/// focused text field scrolls itself into view then.
pub fn reveal() -> bool {
    CHANGED.load(Ordering::Relaxed)
}

/// Scrolls a text field's caret into view when [`reveal`] says to (the
/// field's `ScrollArea` takes the request; egui's `TextEdit` does the
/// same of its own accord at a keystroke).
pub fn reveal_cursor(ui: &egui::Ui, out: &egui::text_edit::TextEditOutput) {
    if !reveal() {
        return;
    }
    let field = out.response.response.rect;
    let rect = match out.cursor_range {
        Some(range) => {
            let mut r = out.galley.pos_from_cursor(range.primary);
            r.max.y = r.max.y.max(r.min.y + out.galley.rows.first().map_or(16.0, |row| row.rect().height()));
            r.translate(out.galley_pos.to_vec2() - egui::vec2(out.galley.rect.left(), 0.0)).expand2(egui::vec2(0.0, 8.0))
        }
        None => field,
    };
    ui.scroll_to_rect(rect, Some(egui::Align::Center));
}

impl Keyboard {
    /// In `App::raw_input_hook`: reads the keyboard's height when it may
    /// have changed and hands it to egui as the bottom safe-area inset.
    pub fn before_frame(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        CHANGED.store(false, Ordering::Relaxed);
        let focused = focused_text(ctx);
        let watching = focused.is_some() || self.height_px > 0 || self.last_read.is_some_and(|(h, _)| h > 0);
        if watching {
            let now = Instant::now();
            match focused {
                Some(_) if self.focus_since.is_none() => self.focus_since = Some(now),
                None => self.focus_since = None,
                _ => {}
            }
            self.take_answer(now);
            if self.asked.is_none() && self.polled.is_none_or(|t| now.duration_since(t) >= POLL) {
                self.polled = Some(now);
                self.asked = Some((now, super::jni::keyboard_height_later()));
            }
            // The keyboard slides in after the focus (and out after it):
            // keep looking while it matters, input or none.
            ctx.request_repaint_after(POLL);
            self.follow(ctx, focused, now);
        } else {
            self.polled = None;
            self.asked = None;
            self.last_read = None;
            self.zero_since = None;
            self.up_for = None;
            self.focus_since = None;
        }
        // Every frame, down (zero) or up: egui keeps the last inset given.
        let bottom = self.height(ctx);
        raw.safe_area_insets = Some(SafeAreaInsets(MarginF32 { left: 0.0, right: 0.0, top: 0.0, bottom }));
    }

    /// The last question's answer, once it has come.
    fn take_answer(&mut self, now: Instant) {
        let Some((when, slot)) = &self.asked else { return };
        let when = *when;
        let answer = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        match answer {
            Some(answer) => {
                self.asked = None;
                let height = match answer {
                    Ok(h) => h.max(0),
                    Err(e) => {
                        log::warn!("keyboard height: {e}");
                        return;
                    }
                };
                self.last_read = Some(match self.last_read {
                    Some((h, n)) if h == height => (h, n.saturating_add(1)),
                    _ => (height, 1),
                });
            }
            None if now.duration_since(when) >= GIVE_UP => self.asked = None,
            None => {}
        }
    }

    /// Turns the readings into the room made (the module comment).
    fn follow(&mut self, ctx: &egui::Context, focused: Option<egui::Id>, now: Instant) {
        let Some((read, runs)) = self.last_read else { return };
        let since_focus = || self.focus_since.map_or(0, |t| now.duration_since(t).as_millis());
        if read > 0 {
            self.zero_since = None;
            if focused.is_some() {
                self.up_for = focused;
            }
            if self.height_px == 0 {
                log::info!("soft keyboard up: {read} px, {} ms after the focus", since_focus());
                self.set_height(read);
            } else if read != self.height_px && runs >= STEADY_READS {
                log::info!("soft keyboard now {read} px (was {})", self.height_px);
                self.set_height(read);
            }
            return;
        }
        // Read as down.
        if self.height_px == 0 {
            return;
        }
        let Some(id) = focused else {
            log::info!("soft keyboard down: no text field focused");
            self.up_for = None;
            self.zero_since = None;
            self.set_height(0);
            return;
        };
        let since = *self.zero_since.get_or_insert(now);
        if now.duration_since(since) < HIDDEN_FOR {
            // A blink (an IME interruption, a focus move) or the slide out
            // of a keyboard the user is hiding: the room stays meanwhile.
            return;
        }
        log::info!(
            "soft keyboard hidden by the user ({} ms down{}): the text field's focus released",
            now.duration_since(since).as_millis(),
            if self.up_for == Some(id) { "" } else { ", after the focus moved to another field" }
        );
        ctx.memory_mut(|m| m.surrender_focus(id));
        self.up_for = None;
        self.zero_since = None;
        self.set_height(0);
    }

    fn set_height(&mut self, px: i32) {
        if px != self.height_px {
            self.height_px = px;
            CHANGED.store(true, Ordering::Relaxed);
        }
    }

    /// The keyboard's height in points (0 while no room is made).
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
