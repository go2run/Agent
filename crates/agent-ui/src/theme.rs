//! UI theme constants

use egui::{Color32, CornerRadius, Stroke, Vec2};

pub const BG_PRIMARY: Color32 = Color32::from_rgb(24, 24, 27);
pub const BG_SECONDARY: Color32 = Color32::from_rgb(39, 39, 42);
pub const BG_SURFACE: Color32 = Color32::from_rgb(52, 52, 56);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 228, 231);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(161, 161, 170);
pub const ACCENT: Color32 = Color32::from_rgb(99, 102, 241);
pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);
pub const ERROR: Color32 = Color32::from_rgb(239, 68, 68);
pub const WARNING: Color32 = Color32::from_rgb(234, 179, 8);
pub const TERMINAL_BG: Color32 = Color32::from_rgb(15, 15, 18);
pub const TERMINAL_FG: Color32 = Color32::from_rgb(180, 230, 180);
pub const TERMINAL_ERR: Color32 = Color32::from_rgb(255, 120, 120);

pub const PANEL_ROUNDING: CornerRadius = CornerRadius::same(6);
pub const PANEL_PADDING: Vec2 = Vec2::new(12.0, 8.0);

/// Apply the dark theme to an egui context
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals.dark_mode = true;
    style.visuals.panel_fill = BG_PRIMARY;
    style.visuals.window_fill = BG_SECONDARY;
    style.visuals.extreme_bg_color = TERMINAL_BG;

    style.visuals.widgets.inactive.bg_fill = BG_SURFACE;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    style.visuals.widgets.hovered.bg_fill = BG_SURFACE;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    style.visuals.widgets.active.bg_fill = ACCENT;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    style.visuals.selection.bg_fill = ACCENT.linear_multiply(0.4);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);

    ctx.set_style(style);
}
