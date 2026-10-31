//! Small custom-painted widgets shared by the panels.

use eframe::egui::*;

use crate::theme;

/// A square icon button. `selected` fills it with the accent.
pub fn icon_button(ui: &mut Ui, icon: &str, size: f32, selected: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let resp = resp.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let (bg, fg) = if !resp.enabled() {
            (Color32::TRANSPARENT, theme::TEXT_FAINT)
        } else if selected {
            (theme::ACCENT_STRONG, Color32::WHITE)
        } else if resp.hovered() {
            (theme::SURFACE_HOVER, theme::TEXT)
        } else {
            (Color32::TRANSPARENT, theme::TEXT_DIM)
        };
        ui.painter().rect_filled(rect, 6.0, bg);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::mono(size * 0.55), fg);
    }
    resp
}

/// An icon button that stays tinted while `on` (for toggles like the grid).
pub fn icon_toggle(ui: &mut Ui, icon: &str, size: f32, on: bool, tooltip: &str) -> Response {
    if !on {
        return icon_button(ui, icon, size, false, tooltip);
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let resp = resp.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let bg = if resp.hovered() { theme::SURFACE_ACTIVE } else { theme::ACCENT_SOFT };
        ui.painter().rect_filled(rect, 6.0, bg);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon, theme::mono(size * 0.55), theme::ACCENT);
    }
    resp
}

/// Small grey caps label above a group of controls.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(10.5).color(theme::TEXT_FAINT).extra_letter_spacing(0.8));
}

/// Checkerboard that shows through transparent colours.
pub fn paint_checker(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, Color32::from_gray(70));
    let step = 5.0;
    let mut y = rect.min.y;
    let mut row = 0;
    while y < rect.max.y {
        let mut x = rect.min.x + if row % 2 == 0 { 0.0 } else { step };
        while x < rect.max.x {
            let cell = Rect::from_min_size(pos2(x, y), Vec2::splat(step)).intersect(rect);
            painter.rect_filled(cell, 0.0, Color32::from_gray(110));
            x += step * 2.0;
        }
        y += step;
        row += 1;
    }
}

/// A colour swatch; transparent parts show a checkerboard. `selected` adds an accent ring.
pub fn color_chip(ui: &mut Ui, color: Color32, size: Vec2, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        paint_color(ui.painter(), rect, color);
        let ring = if selected {
            Stroke::new(2.0, theme::ACCENT)
        } else if resp.hovered() {
            Stroke::new(1.0, theme::TEXT_DIM)
        } else {
            Stroke::new(1.0, theme::BORDER_STRONG)
        };
        ui.painter().rect_stroke(rect, 4.0, ring, StrokeKind::Outside);
    }
    resp
}

/// Fills `rect` with `color`, over a checkerboard when it isn't opaque.
pub fn paint_color(painter: &Painter, rect: Rect, color: Color32) {
    if color.a() < 255 {
        paint_checker(painter, rect);
    }
    painter.rect_filled(rect, 0.0, color);
}

/// A row of tabs; returns the index of the clicked one.
pub fn tab_bar(ui: &mut Ui, tabs: &[(&str, &str)], current: usize) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, (icon, label)) in tabs.iter().enumerate() {
            let active = i == current;
            let text = format!("{icon}  {label}");
            let galley = ui.painter().layout_no_wrap(text, theme::semibold(13.0), Color32::PLACEHOLDER);
            let size = vec2(galley.size().x + 24.0, 32.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let color = if active { theme::TEXT } else if resp.hovered() { theme::TEXT_DIM } else { theme::TEXT_FAINT };
            ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
            if active {
                let bar = Rect::from_min_max(pos2(rect.min.x + 8.0, rect.max.y - 2.0), pos2(rect.max.x - 8.0, rect.max.y));
                ui.painter().rect_filled(bar, 1.0, theme::ACCENT);
            }
            if resp.clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// `#RRGGBB` / `#RRGGBBAA` (the `#` is optional).
pub fn parse_hex(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

pub fn to_hex(color: Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 { format!("#{r:02X}{g:02X}{b:02X}") } else { format!("#{r:02X}{g:02X}{b:02X}{a:02X}") }
}

/// A thin vertical divider for horizontal toolbars.
pub fn vdivider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 18.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, theme::BORDER_STRONG));
}

/// A full-width 1 px divider.
pub fn hline(ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    let (r, _) = ui.allocate_exact_size(vec2(rect.width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, theme::BORDER));
}
