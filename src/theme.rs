//! The "Void" look: every colour, font and spacing of the UI lives here.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals, style::{ScrollStyle, WidgetVisuals}, vec2,
};

// ── Palette ──────────────────────────────────────────────────────────────────

/// Behind the canvas and inside text fields.
pub const BG_DEEP: Color32 = Color32::from_rgb(11, 11, 15);
pub const PANEL: Color32 = Color32::from_rgb(18, 18, 24);
/// Buttons, inputs, list rows.
pub const SURFACE: Color32 = Color32::from_rgb(28, 28, 37);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(38, 38, 50);
pub const SURFACE_ACTIVE: Color32 = Color32::from_rgb(48, 46, 66);
pub const BORDER: Color32 = Color32::from_rgb(36, 36, 48);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(58, 58, 78);

pub const TEXT: Color32 = Color32::from_rgb(228, 228, 238);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 150, 172);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(96, 96, 118);

/// Void Violet: text, icons and outlines on dark backgrounds.
pub const ACCENT: Color32 = Color32::from_rgb(167, 139, 250);
/// Filled accent (active tool, primary buttons).
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(124, 77, 255);
/// Tinted background of selected rows.
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_unmultiplied_const(139, 92, 246, 46);

pub const WARNING: Color32 = Color32::from_rgb(251, 191, 36);

/// Empty canvas cells.
pub const CANVAS_BG: Color32 = Color32::BLACK;
/// Tint over the selected area.
pub const SELECTION_TINT: Color32 = Color32::from_rgba_unmultiplied_const(139, 92, 246, 30);
pub const GRID: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 14);

// ── Fonts ────────────────────────────────────────────────────────────────────

const SEMIBOLD: &str = "ui-semibold";

/// Inter SemiBold, for headings and tab labels.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// The canvas / glyph font at `size` points.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("Inter", include_bytes!("assets/Inter-Regular.ttf").as_slice()),
        ("Inter-SemiBold", include_bytes!("assets/Inter-SemiBold.ttf").as_slice()),
        ("FiraCode", crate::export::FONT),
        ("AdwaitaMono", crate::export::FALLBACK_FONT),
    ] {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    // UI text is Inter; FiraCode (a Nerd Font) supplies icons and box-drawing glyphs Inter lacks,
    // and Adwaita Mono what both lack: braille, sextants, quarter blocks, many symbols.
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.splice(0..0, ["Inter".to_owned(), "FiraCode".to_owned(), "AdwaitaMono".to_owned()]);
    let fallbacks = proportional[3..].to_vec();
    fonts.families.entry(FontFamily::Monospace).or_default().splice(0..0, ["FiraCode".to_owned(), "AdwaitaMono".to_owned()]);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD.into()),
        ["Inter-SemiBold".to_owned(), "FiraCode".to_owned(), "AdwaitaMono".to_owned()].into_iter().chain(fallbacks).collect(),
    );
    fonts
}

// ── Style ────────────────────────────────────────────────────────────────────

/// Installs fonts and the dark "Void" style.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(Theme::Dark);
    ctx.style_mut_of(Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Heading, semibold(15.0)),
            (TextStyle::Monospace, mono(13.0)),
        ]
        .into();

        let spacing = &mut style.spacing;
        spacing.item_spacing = vec2(6.0, 6.0);
        spacing.button_padding = vec2(8.0, 4.0);
        spacing.window_margin = Margin::same(14);
        spacing.menu_margin = Margin::same(6);
        spacing.interact_size = vec2(24.0, 24.0);
        spacing.scroll = ScrollStyle::thin();

        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(140) };
    v.extreme_bg_color = BG_DEEP;
    v.faint_bg_color = SURFACE;
    v.code_bg_color = SURFACE;
    v.selection.bg_fill = ACCENT_STRONG.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARNING;
    v.weak_text_color = Some(TEXT_DIM);
    v.collapsing_header_frame = false;

    let radius = CornerRadius::same(6);
    let widget = |bg: Color32, stroke: Stroke, fg: Color32| WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: stroke,
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    };
    v.widgets.noninteractive = widget(PANEL, Stroke::new(1.0, BORDER), TEXT);
    v.widgets.inactive = widget(SURFACE, Stroke::NONE, TEXT);
    v.widgets.hovered = widget(SURFACE_HOVER, Stroke::new(1.0, BORDER_STRONG), Color32::WHITE);
    v.widgets.active = widget(SURFACE_ACTIVE, Stroke::new(1.0, ACCENT), Color32::WHITE);
    v.widgets.open = widget(SURFACE_HOVER, Stroke::new(1.0, BORDER_STRONG), TEXT);
    v
}
