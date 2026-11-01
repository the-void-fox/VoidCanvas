//! Edit → Replace: find cells by glyph and colours, change any of their parts.

use eframe::egui::{self, *};

use super::AsciiArtEditor;
use crate::document::{Cell, Changes, Color, Layer};
use crate::theme;

/// One side of the dialog: each part is used only when it's switched on.
#[derive(Clone)]
pub struct CellPattern {
    pub glyph: Option<char>,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

impl CellPattern {
    fn is_empty(&self) -> bool {
        self.glyph.is_none() && self.fg.is_none() && self.bg.is_none()
    }

    /// Does `cell` have every part this pattern asks for? (The foreground only counts on glyphs.)
    pub fn matches(&self, cell: &Cell) -> bool {
        self.glyph.is_none_or(|g| cell.ch == g)
            && self.fg.as_ref().is_none_or(|fg| cell.ch != ' ' && cell.fg.same_as(fg))
            && self.bg.as_ref().is_none_or(|bg| cell.bg.same_as(bg))
    }

    /// `cell` with this pattern's parts put in.
    pub fn apply(&self, cell: &Cell) -> Cell {
        Cell {
            ch: self.glyph.unwrap_or(cell.ch),
            fg: self.fg.clone().unwrap_or_else(|| cell.fg.clone()),
            bg: self.bg.clone().unwrap_or_else(|| cell.bg.clone()),
        }
    }
}

/// Changes replacing every cell of `layer` inside `region` that matches `find`.
pub fn replace_changes(layer: &Layer, region: (usize, usize, usize, usize), find: &CellPattern, with: &CellPattern) -> Changes {
    let mut changes = Changes::new();
    if find.is_empty() || with.is_empty() { return changes; }
    let (x0, y0, x1, y1) = region;
    for y in y0..y1.min(layer.height) {
        for x in x0..x1.min(layer.width) {
            let Some(cell) = layer.get(x, y) else { continue };
            if cell.is_empty() || !find.matches(cell) { continue; }
            let new = with.apply(cell);
            if new != *cell {
                changes.insert((x, y), (cell.clone(), new));
            }
        }
    }
    changes
}

pub struct ReplaceDialog {
    find: PatternInput,
    with: PatternInput,
}

/// Editable form of a [`CellPattern`].
struct PatternInput {
    use_glyph: bool,
    glyph: String,
    use_fg: bool,
    fg: Color32,
    use_bg: bool,
    bg: Color32,
}

impl PatternInput {
    fn pattern(&self) -> CellPattern {
        CellPattern {
            glyph: self.glyph.chars().next().filter(|_| self.use_glyph),
            fg: self.use_fg.then(|| Color::from_egui(self.fg)),
            bg: self.use_bg.then(|| Color::from_egui(self.bg)),
        }
    }

    fn ui(&mut self, ui: &mut Ui, id: &str) {
        Grid::new(id).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.checkbox(&mut self.use_glyph, "Glyph");
            ui.add_enabled(self.use_glyph, TextEdit::singleline(&mut self.glyph).char_limit(1).desired_width(28.0).font(TextStyle::Monospace));
            ui.end_row();
            ui.checkbox(&mut self.use_fg, "Fore");
            ui.add_enabled_ui(self.use_fg, |ui| color_picker::color_edit_button_srgba(ui, &mut self.fg, color_picker::Alpha::OnlyBlend));
            ui.end_row();
            ui.checkbox(&mut self.use_bg, "Back");
            ui.add_enabled_ui(self.use_bg, |ui| color_picker::color_edit_button_srgba(ui, &mut self.bg, color_picker::Alpha::OnlyBlend));
            ui.end_row();
        });
    }
}

impl AsciiArtEditor {
    /// Opens Replace with the secondary slot as "find" and the primary slot as "replace with".
    pub(super) fn open_replace(&mut self) {
        let panel = &self.color_panel;
        let input = |glyph: char, fg: &Color, bg: &Color| PatternInput {
            use_glyph: true,
            glyph: glyph.to_string(),
            use_fg: false,
            fg: fg.to_egui(),
            use_bg: false,
            bg: if bg.is_transparent() { Color32::BLACK } else { bg.to_egui() },
        };
        self.replace = Some(ReplaceDialog {
            find: input(self.secondary_char, &panel.secondary_fg, &panel.secondary_bg),
            with: input(self.primary_char, &panel.primary_fg, &panel.primary_bg),
        });
    }

    /// The part of the active layer Replace works on: the selection, or everything.
    fn replace_region(&self) -> (usize, usize, usize, usize) {
        match self.tools.selection.selection {
            Some((x1, y1, x2, y2)) => (x1, y1, x2 + 1, y2 + 1),
            None => (0, 0, self.doc.width, self.doc.height),
        }
    }

    pub(super) fn replace_dialog(&mut self, ctx: &egui::Context) {
        if self.replace.is_none() { return; }
        let region = self.replace_region();
        let Some(dialog) = &mut self.replace else { return };
        let (find, with) = (dialog.find.pattern(), dialog.with.pattern());
        let count = replace_changes(self.doc.active_layer(), region, &find, &with).len();
        let scope = if self.tools.selection.selection.is_some() { "in the selection" } else { "on this layer" };

        let mut open = true;
        let mut apply = false;
        egui::Window::new("Replace")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Find cells with").font(theme::semibold(13.0)));
                        dialog.find.ui(ui, "replace_find");
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Replace with").font(theme::semibold(13.0)));
                        dialog.with.ui(ui, "replace_with");
                    });
                });
                ui.add_space(8.0);
                let summary = if find.is_empty() || with.is_empty() {
                    "Switch on at least one part on each side".to_owned()
                } else {
                    format!("{count} cells {scope}")
                };
                ui.label(RichText::new(summary).color(theme::TEXT_DIM));
                ui.add_space(4.0);
                if ui.add_enabled(count > 0, Button::new("Replace All")).clicked() {
                    apply = true;
                }
            });

        if apply {
            if self.active_layer_locked() { return; }
            let changes = replace_changes(self.doc.active_layer(), region, &find, &with);
            let layer = self.doc.active_layer_mut();
            for (&(x, y), (_, new)) in &changes {
                layer.set(x, y, new.clone());
            }
            self.commit_draw(changes, "Replace");
            self.show_status(format!("Replaced {count} cells"));
        }
        if !open {
            self.replace = None;
        }
    }
}
