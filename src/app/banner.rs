//! Edit → Insert Banner: big FIGlet letters, placed as a selection.

use eframe::egui::{self, *};

use super::AsciiArtEditor;
use super::import::preview;
use crate::document::Color;
use crate::figlet::fonts;
use crate::theme;
use crate::tools::{ToolType, selection::text_to_content};

pub struct BannerDialog {
    pub text: String,
    pub font: usize,
}

impl AsciiArtEditor {
    pub(super) fn open_banner(&mut self) {
        self.banner = Some(BannerDialog { text: "Void".into(), font: 0 });
    }

    pub(super) fn banner_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.banner else { return };
        let rows = fonts().get(dialog.font).map(|f| f.render(&dialog.text)).unwrap_or_default();

        let mut open = true;
        let mut insert = false;
        egui::Window::new("Insert Banner")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.add(TextEdit::multiline(&mut dialog.text).desired_rows(2).desired_width(f32::INFINITY).hint_text("Text"));
                ui.horizontal_wrapped(|ui| {
                    for (i, font) in fonts().iter().enumerate() {
                        ui.selectable_value(&mut dialog.font, i, font.name);
                    }
                });
                ui.add_space(6.0);
                let cells = text_to_content(&rows.join("\n"), &Color::from_egui(theme::TEXT), &Color::TRANSPARENT);
                preview(ui, &cells, vec2(500.0, 180.0));
                ui.add_space(6.0);
                let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
                ui.label(RichText::new(format!("{width} × {} cells", rows.len())).color(theme::TEXT_DIM));
                if ui.add_enabled(width > 0, Button::new("Insert")).clicked() {
                    insert = true;
                }
            });

        if insert && !self.active_layer_locked() {
            let panel = &self.color_panel;
            let block = text_to_content(&rows.join("\n"), &panel.primary_fg, &panel.primary_bg);
            let (x, y) = self.tools.selection.selection.map_or((0, 0), |(x1, y1, _, _)| (x1, y1));
            let changes = self.tools.selection.paste(&mut self.doc, &block, x, y);
            self.commit_draw(changes, "Banner");
            // Ready to be dragged or nudged into place.
            self.active_tool = ToolType::Select;
            self.banner = None;
        } else if !open {
            self.banner = None;
        }
    }
}
