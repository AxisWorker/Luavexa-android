//! Luavexa Debug: lightweight performance readings and a reversible
//! GPU transparency comparison, plus the existing driver diagnostics.
//! Use File > Studio Settings > Luavexa > Performance for quality controls.
//! This screen does not alter project files or GPU driver state by itself.

use egui::RichText;

use crate::app::Editor;

impl Editor {
    /// Touch-friendly floating diagnostics. Drag the title to move the
    /// window; swipe the body to scroll through long GPU-driver details.
    pub fn graphics_settings_ui(&mut self, ctx: &egui::Context) {
        if !self.ui.graphics_settings {
            rustudio_render::gpu_timing::set_live_debug(false);
            return;
        }
        let mut open = true;
        let screen = ctx.content_rect();
        let width = (screen.width() - 16.0).clamp(180.0, 440.0);
        egui::Window::new("Luavexa Debug")
            .id(egui::Id::new("luavexa_debug_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .drag_area(egui::WindowDrag::TitleBar)
            .constrain_to(screen)
            .default_width(width)
            .default_pos(screen.center() - egui::vec2(width * 0.5, (screen.height() * 0.45).min(320.0)))
            .vscroll(true)
            .show(ctx, |ui| self.graphics_settings_body(ui));
        self.ui.graphics_settings = open;
        if !open {
            rustudio_render::gpu_timing::set_live_debug(false);
        }
    }

    fn graphics_settings_body(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().interact_size.y = ui.spacing().interact_size.y.max(36.0);
        use rustudio_core::rustudio_settings as settings;

        ui.label(RichText::new("LUAVEXA / PERFORMANCE").strong());
        ui.small(RichText::new("Drag the title bar to move this window; swipe the details below to scroll.").weak());
        let fps = self.fps.max(0.0);
        let frame_ms = if fps > 0.1 { format!("{:.1} ms", 1000.0 / fps) } else { "measuring...".into() };
        let mode = if self.game.is_playing() { "Play" } else { "Edit" };
        ui.label(format!("{} · {:.0} FPS · {}", mode, fps, frame_ms));
        // This measures Editor::finish_frame's update and UI work, *before*
        // eframe's draw submission, egui GPU compositing and window present.
        // Readings are from the preceding frame; frame_ms is a smoothed
        // wall-clock interval. They are intentionally not added together.
        let cpu_ms = self.last_work_ms.max(0.0);
        ui.label(format!("CPU update + UI: {cpu_ms:.1} ms (previous frame)"));
        ui.small(RichText::new("CPU excludes eframe GPU submission and display present; FPS represents a smoothed full-frame interval. Compare stable camera views, not one snapshot.").weak());
        let view = self.game.viewport_rect;
        ui.label(format!("Viewport: {:.0} × {:.0} logical points", view.width().max(0.0), view.height().max(0.0)));
        ui.label(format!(
            "3D resolution: {}% · Graphics: {} · Detail: {}",
            settings::performance_value("resolution"),
            settings::performance_value("quality"),
            settings::performance_value("detail")
        ));
        ui.label(format!(
            "SSAO: {} · Shadows: {} · Bloom: {}",
            settings::performance_value("ssao"),
            settings::performance_value("shadows"),
            settings::performance_value("bloom")
        ));
        ui.small(RichText::new("Keep the same camera, graphics preset, and view size when comparing FPS.").weak());
        ui.separator();

        ui.label(RichText::new("GPU PASS TIMINGS").strong());
        let mut timing = rustudio_render::gpu_timing::live_debug_enabled();
        if ui.checkbox(&mut timing, "Measure individual GPU passes (experimental)").changed() {
            rustudio_render::gpu_timing::set_live_debug(timing);
        }
        ui.small(RichText::new("Off by default. Timestamp queries may lag a few frames. Timed scene passes exclude egui compositing, GPU queue waits and presentation; enabling adds some overhead.").weak());
        if timing {
            match self.game.render_state.as_ref() {
                Some(rs) if rustudio_render::gpu_timing::supported(&rs.device) => {
                    let mut spans = rustudio_render::gpu_timing::live_debug_snapshot();
                    spans.sort_by(|a, b| b.1.total_cmp(&a.1));
                    if spans.is_empty() {
                        ui.small("Collecting GPU timings...");
                    } else {
                        for (label, ms) in spans.iter().take(12) {
                            ui.monospace(format!("{label}: {ms:.2} ms"));
                        }
                        ui.small("Shown: latest resolved GPU scene spans, not a complete frame. If FPS is low but all spans are short, investigate CPU update/UI, egui rendering, pacing, waits and presentation.");
                        if ui.button("Copy GPU timings").clicked() {
                            let mut text = format!(
                                "Luavexa GPU timing (ms; most recent resolved frame)\nMode: {mode} | FPS (smoothed): {fps:.1} | frame interval: {frame_ms}\nCPU update + UI (previous frame): {cpu_ms:.3} ms\nGPU spans omit egui composition and present; not an end-to-end frame time.\n"
                            );
                            for (label, ms) in spans {
                                use std::fmt::Write as _;
                                let _ = writeln!(&mut text, "{label}: {ms:.3}");
                            }
                            ui.ctx().copy_text(text);
                        }
                    }
                }
                Some(_) => {
                    ui.small("This driver does not expose the timestamp queries needed for GPU pass measurements. FPS and device details remain available.");
                }
                None => {
                    ui.small("Renderer not ready yet.");
                }
            }
        }
        ui.separator();

        ui.label(RichText::new("TRANSPARENCY A/B TEST").strong());
        let mut fast = rustudio_render::render::fast_transparency_enabled();
        if ui.checkbox(&mut fast, "Use fast linear transparency (experimental)").changed() {
            rustudio_render::render::set_fast_transparency(fast);
        }
        ui.small(RichText::new("OFF: original encoded SeeThrough passes. ON: direct linear blending. Colors and transparent edges may look different. Session-only; automatically resets at restart. This test does not prove that blending is the only bottleneck.").weak());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Reset transparency").clicked() {
                rustudio_render::render::set_fast_transparency(false);
            }
            if ui.button("Copy FPS summary").clicked() {
                ui.ctx().copy_text(format!(
                    "Luavexa debug | {mode} | {fps:.0} FPS | {frame_ms} | CPU update+UI (prior): {cpu_ms:.1}ms | viewport {:.0}x{:.0} logical | resolution {}% | quality {} | detail {} | linear transparency {}",
                    view.width().max(0.0),
                    view.height().max(0.0),
                    settings::performance_value("resolution"),
                    settings::performance_value("quality"),
                    settings::performance_value("detail"),
                    rustudio_render::render::fast_transparency_enabled(),
                ));
            }
        });
        #[cfg(target_os = "android")]
        if ui.button("Export crash & session logs to Download").clicked() {
            match crate::android::logs::export("Luavexa Debug") {
                Ok(path) => self.info(format!("Luavexa debug logs saved to {path}")),
                Err(e) => self.error(format!("Couldn't export debug logs: {e}")),
            }
        }
        ui.separator();
        ui.label(RichText::new("GPU / DRIVER").strong());
        match self.game.render_state.as_ref() {
            Some(rs) => {
                let i = rs.adapter.get_info();
                ui.label(RichText::new(&i.name).strong());
                ui.small(format!("{:?}, {:?}; driver {} {}", i.backend, i.device_type, i.driver, i.driver_info).trim_end().to_string());
                ui.small(format!("Surface {:?}", rs.target_format));
            }
            None => {
                ui.label(RichText::new("No GPU renderer").strong());
            }
        }
        if rustudio_core::rustudio_settings::weak_gpu() {
            ui.small("Weak-GPU mode: Automatic quality starts at its lowest level and climbs as frame times allow (RUSTUDIO_LOWEND=0 turns it off).");
        }
        ui.small(RichText::new("Graphics quality, 3D resolution, shading detail, shadows and the frame limit: File > Studio Settings > Luavexa > Performance.").weak());
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
    ui.label(RichText::new("Vulkan driver").strong());
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
    ui.small(RichText::new("LRZ is off by default (Mesa 26.3.0 lost terrain chunks with it on an Adreno 650); on an Adreno 6xx bandwidth compression is off too (with it, Turnip's first frame restarted a Snapdragon 865 phone outright). For other missing or flickering geometry: try the rest top down, one at a time; the ones marked take effect at the next start.").weak());
    let flags = turnip::debug_flags();
    for s in turnip::SWITCHES {
        let mut on = flags.iter().any(|f| f == s.flag);
        let label = if s.live { s.name.to_string() } else { format!("{} (next start)", s.name) };
        if ui.checkbox(&mut on, label).on_hover_text(format!("{}\nTU_DEBUG={}", s.hint, s.flag)).changed() {
            turnip::set_debug_flag(s.flag, on);
        }
    }
}
