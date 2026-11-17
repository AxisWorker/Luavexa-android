//! View > Graphics Settings: Studio's graphics quality bar (1 to 10, or
//! Automatic) and each setting apart from it — render distance, part
//! detail, texture quality, texture filtering, the lite shader,
//! anti-aliasing, the scene's format, render scale and the scale while the
//! camera moves, Play's frame-rate cap, the sun's and the lights' shadows,
//! ambient occlusion, bloom, depth of field, clouds, reflections — each
//! "Auto" (from the level) unless chosen (src/quality.rs `Graphics`,
//! `Overrides`). The choices
//! live in the saved layout (layout_store.rs); the level mirrors Studio
//! Settings > Rendering > Editor Quality Level. Built for a thumb: one
//! combo box per setting, no drags but the bar's.

use egui::{ComboBox, Grid, RichText, Slider};

use crate::app::Editor;
use crate::quality::{Overrides, RenderSettings, Shadows, LEVELS};

impl Editor {
    /// The window, while View > Graphics Settings has it open.
    pub fn graphics_settings_ui(&mut self, ctx: &egui::Context) {
        if !self.ui.graphics_settings {
            return;
        }
        let mut open = true;
        let before = self.graphics;
        let screen = ctx.content_rect();
        egui::Window::new("Graphics Settings")
            .id(egui::Id::new("graphics_settings_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .default_pos(screen.center() - egui::vec2(200.0, (screen.height() * 0.45).min(320.0)))
            // (a phone's screen is shorter than the rows: the rest scrolls)
            .vscroll(true)
            .show(ctx, |ui| self.graphics_settings_body(ui));
        self.ui.graphics_settings = open;
        if self.graphics != before {
            if self.graphics.auto != before.auto || self.graphics.level != before.level {
                self.quality_auto.reset();
            }
            self.quality_changed(true);
        }
    }

    fn graphics_settings_body(&mut self, ui: &mut egui::Ui) {
        let format = self.render_state.as_ref().map(|rs| rs.device.clone());
        let g = &mut self.graphics;
        ui.spacing_mut().interact_size.y = ui.spacing().interact_size.y.max(28.0);

        ui.label(RichText::new("Graphics Mode").strong());
        ui.horizontal(|ui| {
            ui.radio_value(&mut g.auto, true, "Automatic");
            ui.radio_value(&mut g.auto, false, "Manual");
        });
        ui.small(if g.auto {
            "The level follows the frame rate: down after two seconds under 60 Hz, up after twenty seconds at it."
        } else {
            "The level stays where the bar is."
        });
        ui.add_space(6.0);

        ui.label(RichText::new("Graphics Quality").strong());
        let bar = Slider::new(&mut g.level, 1..=LEVELS).integer().step_by(1.0).show_value(true).trailing_fill(true);
        ui.add_enabled(!g.auto, bar);
        ui.horizontal(|ui| {
            ui.small("1 fastest");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.small("10 best"));
        });
        if g.auto {
            ui.small(format!("Automatic is at level {} now.", g.level));
        }
        ui.add_space(6.0);
        ui.separator();

        ui.label(RichText::new("Individual Settings").strong());
        ui.small("Auto follows the level; a choice here stays through level changes.");
        let lv = RenderSettings::level(g.level);
        let o = &mut g.over;
        Grid::new("graphics_overrides").num_columns(2).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
            choice(
                ui,
                "Render Distance",
                &mut o.view_distance,
                &studs(lv.view_distance),
                &[(500.0, "500 studs"), (1000.0, "1,000 studs"), (2000.0, "2,000 studs"), (4000.0, "4,000 studs"), (10000.0, "10,000 studs"), (0.0, "Unlimited")],
                |v| studs(*v),
            );
            choice(
                ui,
                "Part Detail",
                &mut o.detail_px,
                &detail(lv.detail_px),
                &[(4.0, "Low"), (2.0, "Medium"), (1.0, "High")],
                |v| detail(*v),
            );
            choice(
                ui,
                "Texture Quality",
                &mut o.texture_texels,
                &texels(lv.texture_texels),
                &[(256, "Low"), (512, "Medium"), (1024, "High")],
                |v| texels(*v),
            );
            choice(
                ui,
                "Texture Filtering",
                &mut o.anisotropy,
                &anisotropy(lv.anisotropy),
                &[(1, "Trilinear"), (2, "2x anisotropic"), (4, "4x anisotropic"), (8, "8x anisotropic"), (16, "16x anisotropic")],
                |v| anisotropy(*v),
            );
            choice(
                ui,
                "Shading",
                &mut o.lite,
                lite(lv.lite),
                &[(true, "Simple"), (false, "Full")],
                |v| lite(if *v { crate::quality::LITE_PHONE } else { 0 }).to_string(),
            );
            choice(
                ui,
                "Anti-aliasing",
                &mut o.msaa,
                &msaa(lv.msaa),
                &[(1, "Off"), (2, "2x MSAA"), (4, "4x MSAA")],
                |v| msaa(*v),
            );
            choice(
                ui,
                "HDR Scene",
                &mut o.hdr,
                &hdr(lv.hdr),
                &[(false, "Off"), (true, "On")],
                |v| hdr(*v),
            );
            choice(
                ui,
                "Render Scale",
                &mut o.render_scale,
                &percent(lv.render_scale),
                &[(0.5, "50%"), (0.6, "60%"), (0.7, "70%"), (0.8, "80%"), (1.0, "100%")],
                |v| percent(*v),
            );
            choice(
                ui,
                "Scale While Moving",
                &mut o.motion_scale,
                &percent(lv.motion_scale.min(lv.render_scale)),
                &[(0.4, "40%"), (0.5, "50%"), (0.6, "60%"), (0.7, "70%"), (0.8, "80%"), (1.0, "100%")],
                |v| percent(*v),
            );
            choice(
                ui,
                "Play Frame Rate",
                &mut o.fps_cap,
                &fps(lv.fps_cap),
                &[(0, "Uncapped"), (30, "30 fps"), (45, "45 fps"), (60, "60 fps")],
                |v| fps(*v),
            );
            choice(
                ui,
                "Sun Shadows",
                &mut o.shadows,
                Shadows::nearest(lv.sun_cascades, lv.shadow_scale).name(),
                &[(Shadows::Off, "Off"), (Shadows::Low, "Low"), (Shadows::Medium, "Medium"), (Shadows::High, "High")],
                |v| v.name().to_string(),
            );
            choice(ui, "Light Shadows", &mut o.light_shadows, onoff(lv.light_shadows), ONOFF, |v| onoff(*v).to_string());
            choice(ui, "Ambient Occlusion", &mut o.ssao, onoff(lv.ssao), ONOFF, |v| onoff(*v).to_string());
            choice(ui, "Bloom", &mut o.bloom, onoff(lv.bloom_levels > 0), ONOFF, |v| onoff(*v).to_string());
            choice(ui, "Depth of Field", &mut o.dof, onoff(lv.dof), ONOFF, |v| onoff(*v).to_string());
            choice(ui, "Clouds", &mut o.clouds, onoff(lv.clouds), ONOFF, |v| onoff(*v).to_string());
            choice(ui, "Reflections", &mut o.env_probe, onoff(lv.env_probe), ONOFF, |v| onoff(*v).to_string());
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(g.over.any(), egui::Button::new("All Auto")).clicked() {
                g.over = Overrides::default();
            }
            if ui.button("Restore Defaults").clicked() {
                *g = crate::quality::Graphics::platform_default();
            }
        });
        ui.separator();
        let s = g.settings();
        let scene = match format {
            Some(device) => s.scene_format(&device),
            None => egui_wgpu::wgpu::TextureFormat::Rgba16Float,
        };
        ui.small(format!("Drawing at {}", s.describe(scene)));
        if crate::quality::Graphics::env_level().is_some() {
            ui.small(RichText::new("RUSTUDIO_QUALITY is set for this run: it overrides the level.").weak());
        }
        #[cfg(target_os = "android")]
        gpu_driver_section(ui);
    }
}

/// Android: which Vulkan driver this run has, and the switch for the
/// Mesa driver the APK ships for the device — Turnip on Snapdragon, PanVK
/// on Mali (src/android/turnip.rs; takes effect at the next start).
#[cfg(target_os = "android")]
fn gpu_driver_section(ui: &mut egui::Ui) {
    use crate::android::turnip;
    ui.separator();
    ui.label(RichText::new("GPU Driver").strong());
    let st = turnip::status();
    if st.device.is_empty() {
        ui.small(format!("This run: the device's driver ({}).", st.note));
    } else {
        ui.small(format!("This run: {} — {}.", st.device, st.driver));
    }
    let renderer = turnip::renderer();
    if !renderer.is_empty() {
        ui.small(format!("Renderer on: {renderer}."));
    }
    if !(st.available || st.active) {
        return;
    }
    let mesa = st.mesa.unwrap_or(turnip::Mesa::Turnip);
    let mut on = turnip::wanted();
    if ui
        .checkbox(&mut on, format!("{} (Mesa) Vulkan driver", mesa.name()))
        .on_hover_text(format!("Mesa's open-source {} driver in place of the phone's, from the next start on", mesa.gpus()))
        .changed()
    {
        turnip::set_wanted(on);
    }
    if turnip::failed_last_time() {
        ui.horizontal_wrapped(|ui| {
            ui.small(RichText::new("Off since a start with it didn't reach the first frame.").weak());
            if ui.small_button("Try again").clicked() {
                turnip::retry();
            }
        });
    } else if on != st.active {
        ui.small(RichText::new("Takes effect at the next start.").weak());
    }
    // (TU_DEBUG is Turnip's; PanVK has switches of its own, PAN_MESA_DEBUG,
    // none of which the window offers yet)
    if st.active && mesa == turnip::Mesa::Turnip {
        turnip_switches(ui);
    }
}

/// Turnip's own switches (`TU_DEBUG` options), for when something draws
/// wrong with it: the live ones change the running driver as the box is
/// ticked (Mesa watches the file they are kept in), the others at the
/// next start. The choice stays until "Default".
#[cfg(target_os = "android")]
fn turnip_switches(ui: &mut egui::Ui) {
    use crate::android::turnip;
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Turnip switches").strong());
        if !turnip::debug_flags_default() && ui.small_button("Default").clicked() {
            turnip::reset_debug_flags();
        }
    });
    ui.small(RichText::new("LRZ is off by default (Mesa 26.3.0 lost terrain chunks with it on an Adreno 650). For other missing or flickering geometry: try the rest top down, one at a time; the ones marked take effect at the next start.").weak());
    let flags = turnip::debug_flags();
    for s in turnip::SWITCHES {
        let mut on = flags.iter().any(|f| f == s.flag);
        let label = if s.live { s.name.to_string() } else { format!("{} (next start)", s.name) };
        if ui.checkbox(&mut on, label).on_hover_text(format!("{}\nTU_DEBUG={}", s.hint, s.flag)).changed() {
            turnip::set_debug_flag(s.flag, on);
        }
    }
}

const ONOFF: &[(bool, &str)] = &[(false, "Off"), (true, "On")];

/// One setting's row: its name and a combo box of Auto (the level's
/// value, named) and the choices.
fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, label: &str, slot: &mut Option<T>, auto: &str, options: &[(T, &str)], name: impl Fn(&T) -> String) {
    ui.label(label);
    let shown = match slot {
        None => format!("Auto ({auto})"),
        Some(v) => options.iter().find(|(o, _)| o == v).map_or_else(|| name(v), |(_, n)| n.to_string()),
    };
    ComboBox::from_id_salt(label).selected_text(shown).width(170.0).show_ui(ui, |ui| {
        ui.selectable_value(slot, None, format!("Auto ({auto})"));
        for (v, n) in options {
            ui.selectable_value(slot, Some(*v), *n);
        }
    });
    ui.end_row();
}

fn studs(v: f32) -> String {
    if v.is_finite() && v > 0.0 {
        let n = v.round() as u64;
        if n >= 1000 { format!("{},{:03} studs", n / 1000, n % 1000) } else { format!("{n} studs") }
    } else {
        "Unlimited".to_string()
    }
}

fn detail(px: f32) -> String {
    match px {
        p if p >= 3.0 => format!("Low, parts under {px} px skipped"),
        p if p > 1.0 => format!("Medium, under {px} px"),
        _ => "High".to_string(),
    }
}

fn texels(t: u32) -> String {
    format!("{t}²")
}

fn msaa(n: u32) -> String {
    if n > 1 { format!("{n}x MSAA") } else { "Off".to_string() }
}

fn hdr(on: bool) -> String {
    if on { "On, 64-bit".to_string() } else { "Off, 32-bit".to_string() }
}

fn percent(s: f32) -> String {
    format!("{}%", (s * 100.0).round())
}

fn anisotropy(n: u16) -> String {
    if n > 1 { format!("{n}x anisotropic") } else { "Trilinear".to_string() }
}

fn lite(bits: u32) -> &'static str {
    if bits == 0 { "Full" } else { "Simple" }
}

fn fps(cap: u32) -> String {
    if cap > 0 { format!("{cap} fps") } else { "Uncapped".to_string() }
}

fn onoff(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}
