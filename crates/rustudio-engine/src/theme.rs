//! Aurora Foundation: Luavexa's Android-first dark visual system.
//! A low-cost navy surface palette with purple actions and cyan selection,
//! designed without GPU blur or expensive continuously animated backgrounds.
//! Keep theme changes independent of editor and engine logic.

use egui::{Color32, CornerRadius, Stroke};

pub const BG: Color32 = Color32::from_rgb(11, 16, 37);
pub const PANEL: Color32 = Color32::from_rgb(18, 25, 49);
pub const PANEL_ALT: Color32 = Color32::from_rgb(33, 43, 75);
pub const PANEL_DARK: Color32 = Color32::from_rgb(21, 27, 62);
/// The menu bar.
pub const MENU_BAR: Color32 = Color32::from_rgb(17, 24, 50);
/// The selected ribbon tab / toggled tool.
pub const ACTIVE_TAB: Color32 = Color32::from_rgb(36, 48, 88);
pub const DIVIDER: Color32 = Color32::from_rgb(54, 67, 97);
pub const TEXT: Color32 = Color32::from_rgb(232, 237, 255);
pub const TEXT_DIM: Color32 = Color32::from_rgb(169, 181, 210);
pub const ACCENT: Color32 = Color32::from_rgb(138, 73, 247);
pub const SELECT: Color32 = Color32::from_rgb(0, 217, 245);
pub const SELECT_BG: Color32 = Color32::from_rgb(37, 59, 98);
pub const HOVER_BG: Color32 = Color32::from_rgb(40, 54, 92);
/// 3D hover outline (Studio's "Hover Over Color").
pub const HOVER_3D: Color32 = Color32::from_rgb(131, 231, 255);
pub const WARN: Color32 = Color32::from_rgb(255, 199, 119);
pub const ERROR: Color32 = Color32::from_rgb(255, 122, 136);

pub fn apply(ctx: &egui::Context) {
    // eframe can report the system theme after app creation. Select our
    // dark style before customizing it so later frames keep using it.
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL_DARK;
    v.faint_bg_color = Color32::from_rgb(16, 22, 45);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = SELECT_BG;
    v.selection.stroke = Stroke::new(1.0, SELECT);
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(9);
    v.menu_corner_radius = CornerRadius::same(8);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DIVIDER);
    v.widgets.inactive.weak_bg_fill = PANEL_ALT;
    v.widgets.inactive.bg_fill = PANEL_ALT;
    v.widgets.hovered.weak_bg_fill = HOVER_BG;
    v.widgets.active.weak_bg_fill = SELECT_BG;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(6);
    }
    style.spacing.item_spacing = egui::vec2(7.0, 5.0);
    // Do not animate every editor widget. Aurora Home uses explicit short transitions.
    style.animation_time = 0.0;
    style.spacing.button_padding = egui::vec2(9.0, 5.0);
    // Studio's scroll bars take their own strip beside the content (Qt);
    // egui's default floats them over it, covering the right edge of rows
    // (values, checkboxes, buttons) so they can't be clicked.
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    ctx.set_global_style(style);
}

pub fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(style);
    s.tab_bar.bg_fill = PANEL_DARK;
    s.tab_bar.hline_color = DIVIDER;
    s.tab_bar.height = 28.0;
    // Studio's panels have a full-width title bar with the name centred.
    s.tab_bar.fill_tab_bar = true;
    s.tab.active.bg_fill = PANEL_DARK;
    s.tab.active.text_color = TEXT;
    s.tab.focused.bg_fill = PANEL_DARK;
    s.tab.focused.text_color = TEXT;
    s.tab.inactive.bg_fill = PANEL_DARK;
    s.tab.inactive.text_color = TEXT_DIM;
    s.tab.hovered.bg_fill = HOVER_BG;
    s.tab.tab_body.bg_fill = PANEL;
    s.tab.tab_body.inner_margin = egui::Margin::same(4);
    s.separator.width = 2.0;
    s.separator.color_idle = DIVIDER;
    s.separator.color_hovered = ACCENT;
    s.separator.color_dragged = SELECT;
    s.overlay.selection_color = ACCENT.gamma_multiply(0.4);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aurora_style_survives_system_theme_changes() {
        let ctx = egui::Context::default();
        apply(&ctx);

        for system_theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Light] {
            let input = egui::RawInput {
                system_theme: Some(system_theme),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                assert_eq!(ctx.theme(), egui::Theme::Dark);
                assert!(ui.visuals().dark_mode);
                assert_eq!(ui.visuals().widgets.inactive.bg_fill, PANEL_ALT);
                assert_eq!(ui.visuals().override_text_color, Some(TEXT));
            });
            output.textures_delta.clear();
        }
    }
}
