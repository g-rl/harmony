use egui::style::WidgetVisuals;
use egui::{
    Color32, Context, CornerRadius, FontId, Margin, Stroke, TextStyle, ThemePreference, vec2,
};

pub const BG: Color32 = Color32::from_rgb(0x0e, 0x0e, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x16, 0x1e);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x23, 0x23, 0x2e);
pub const GRID: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x26);
pub const HOVER: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x2a);
pub const PRESSED: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x38);
pub const WAVE: Color32 = Color32::from_rgb(0x39, 0xd0, 0xd8);
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0x2e, 0x88);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xf0);
pub const DIM: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x6e);
pub const TEXT: Color32 = Color32::from_rgb(0xa4, 0xa4, 0xb8);
pub const UNLIT: Color32 = Color32::from_rgb(0xff, 0x4d, 0x6a);
/// A sound whose name harmony has not recovered: it is still a key, and the
/// colour says so without saying anything is wrong with it.
pub const UNNAMED: Color32 = Color32::from_rgb(0x6d, 0x5c, 0x8f);
pub const GOOD: Color32 = Color32::from_rgb(0x5a, 0xd8, 0x9a);

pub const UI_FONT_SIZE: f32 = 12.0;
pub const LABEL_FONT_SIZE: f32 = 10.0;
pub const CORNER: CornerRadius = CornerRadius::same(2);
pub const INTERACT_HEIGHT: f32 = 20.0;
pub const ROW_HEIGHT: f32 = 18.0;

pub fn label_font() -> FontId {
    FontId::monospace(LABEL_FONT_SIZE)
}

pub fn apply(ctx: &Context) {
    ctx.set_theme(ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.override_text_style = Some(TextStyle::Monospace);
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(UI_FONT_SIZE));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::monospace(UI_FONT_SIZE));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::monospace(UI_FONT_SIZE));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::monospace(LABEL_FONT_SIZE));

        style.spacing.item_spacing = vec2(8.0, 4.0);
        style.spacing.button_padding = vec2(8.0, 3.0);
        style.spacing.interact_size = vec2(0.0, INTERACT_HEIGHT);
        style.spacing.window_margin = Margin::ZERO;
        style.spacing.scroll.bar_width = 8.0;

        style.visuals.dark_mode = true;
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = PANEL;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.18);
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        style.visuals.window_stroke = Stroke::new(1.0, HAIRLINE);
        style.visuals.widgets.noninteractive =
            flat(PANEL, PANEL, TEXT, Stroke::new(1.0, HAIRLINE));
        style.visuals.widgets.inactive = flat(PANEL, PANEL, TEXT, Stroke::NONE);
        style.visuals.widgets.hovered = flat(HOVER, HOVER, WAVE, Stroke::NONE);
        style.visuals.widgets.active = flat(PRESSED, PRESSED, WAVE, Stroke::NONE);
        style.visuals.widgets.open = flat(HOVER, HOVER, TEXT, Stroke::NONE);
    });
}

fn flat(bg_fill: Color32, weak_bg_fill: Color32, text: Color32, bg_stroke: Stroke) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill,
        weak_bg_fill,
        bg_stroke,
        corner_radius: CORNER,
        fg_stroke: Stroke::new(1.0, text),
        expansion: 0.0,
    }
}
