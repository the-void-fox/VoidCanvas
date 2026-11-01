//! The Library window: the user's glyph sets, density ramps, brushes and colour palettes, next to
//! the built-in ones, to add, copy, edit, reorder, delete and share as text.

use eframe::egui::{self, *};
use serde::{Deserialize, Serialize};

use super::AsciiArtEditor;
use super::storage::GlyphSet;
use crate::document::Color;
use crate::library::{Palette, Ramp, builtin_palettes, builtin_ramps};
use crate::tools::{ToolType, brush::{BrushKind, BrushPreset, builtin_presets}};
use crate::ui::char_palette::CATEGORIES;
use crate::ui::widgets::glyph_strip;
use crate::{icons, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryTab {
    GlyphSets,
    Ramps,
    Brushes,
    Palettes,
}

impl LibraryTab {
    const ALL: [LibraryTab; 4] = [LibraryTab::GlyphSets, LibraryTab::Ramps, LibraryTab::Brushes, LibraryTab::Palettes];

    fn name(self) -> &'static str {
        match self {
            LibraryTab::GlyphSets => "Glyph Sets",
            LibraryTab::Ramps => "Shade Ramps",
            LibraryTab::Brushes => "Brushes",
            LibraryTab::Palettes => "Palettes",
        }
    }
}

pub struct LibraryWindow {
    pub tab: LibraryTab,
    /// Index into the tab's whole list, built-in entries first.
    pub selected: usize,
    /// Text someone shared, to add.
    paste: String,
    message: Option<(bool, String)>,
}

/// An entry as text to share: `{"ramp": {"name": …, "glyphs": …}}` and so on.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Shared {
    GlyphSet(GlyphSet),
    Ramp(Ramp),
    Brush(BrushPreset),
    Palette(Palette),
}

/// A name not taken in `names`: `base`, else `base 2`, `base 3`…
fn fresh_name<'a>(base: &str, names: impl Iterator<Item = &'a str> + Clone) -> String {
    std::iter::once(base.to_owned())
        .chain((2..).map(|n| format!("{base} {n}")))
        .find(|name| names.clone().all(|taken| taken != name))
        .unwrap()
}

impl AsciiArtEditor {
    pub(crate) fn open_library(&mut self, tab: LibraryTab) {
        self.library = Some(LibraryWindow { tab, selected: 0, paste: String::new(), message: None });
    }

    /// How many entries a tab lists, and how many of them are built in.
    fn library_counts(&self, tab: LibraryTab) -> (usize, usize) {
        let s = &self.settings;
        match tab {
            LibraryTab::GlyphSets => (CATEGORIES.len() + s.glyph_sets.len(), CATEGORIES.len()),
            LibraryTab::Ramps => (builtin_ramps().len() + s.ramps.len(), builtin_ramps().len()),
            LibraryTab::Brushes => (builtin_presets().len() + s.brushes.len(), builtin_presets().len()),
            LibraryTab::Palettes => (builtin_palettes().len() + s.palettes.len(), builtin_palettes().len()),
        }
    }

    fn library_names(&self, tab: LibraryTab) -> Vec<String> {
        let s = &self.settings;
        match tab {
            LibraryTab::GlyphSets => CATEGORIES.iter().map(|c| c.0.to_owned()).chain(s.glyph_sets.iter().map(|g| g.name.clone())).collect(),
            LibraryTab::Ramps => s.all_ramps().into_iter().map(|r| r.name).collect(),
            LibraryTab::Brushes => s.all_brushes().into_iter().map(|b| b.name).collect(),
            LibraryTab::Palettes => s.all_palettes().into_iter().map(|p| p.name).collect(),
        }
    }

    /// The selected entry as text to share.
    fn library_share(&self, tab: LibraryTab, i: usize) -> Option<String> {
        let s = &self.settings;
        let shared = match tab {
            LibraryTab::GlyphSets => Shared::GlyphSet(match i.checked_sub(CATEGORIES.len()) {
                Some(own) => s.glyph_sets.get(own)?.clone(),
                None => GlyphSet { name: CATEGORIES[i].0.to_owned(), glyphs: CATEGORIES[i].1.to_owned() },
            }),
            LibraryTab::Ramps => Shared::Ramp(s.all_ramps().get(i)?.clone()),
            LibraryTab::Brushes => Shared::Brush(s.all_brushes().get(i)?.clone()),
            LibraryTab::Palettes => Shared::Palette(s.all_palettes().get(i)?.clone()),
        };
        serde_json::to_string(&shared).ok()
    }

    /// Adds an entry shared as text, under a name of its own; the tab it belongs on.
    pub(crate) fn library_add_shared(&mut self, text: &str) -> Result<LibraryTab, String> {
        let shared: Shared = serde_json::from_str(text.trim()).map_err(|_| "That isn't a shared glyph set, ramp, brush or palette".to_owned())?;
        let names = |tab| self.library_names(tab);
        Ok(match shared {
            Shared::GlyphSet(mut set) => {
                set.name = fresh_name(&set.name, names(LibraryTab::GlyphSets).iter().map(String::as_str));
                self.settings.edit_glyph_sets(|sets| sets.push(set));
                LibraryTab::GlyphSets
            }
            Shared::Ramp(mut ramp) => {
                ramp.name = fresh_name(&ramp.name, names(LibraryTab::Ramps).iter().map(String::as_str));
                self.settings.edit_ramps(|ramps| ramps.push(ramp));
                LibraryTab::Ramps
            }
            Shared::Brush(mut brush) => {
                brush.name = fresh_name(&brush.name, names(LibraryTab::Brushes).iter().map(String::as_str));
                self.settings.edit_brushes(|brushes| brushes.push(brush));
                LibraryTab::Brushes
            }
            Shared::Palette(mut palette) => {
                palette.name = fresh_name(&palette.name, names(LibraryTab::Palettes).iter().map(String::as_str));
                self.settings.edit_palettes(|palettes| palettes.push(palette));
                LibraryTab::Palettes
            }
        })
    }

    /// A new entry of the user's on `tab`: a copy of entry `from`, or a fresh one.
    pub(crate) fn library_new(&mut self, tab: LibraryTab, from: Option<usize>) {
        let names = self.library_names(tab);
        let taken = || names.iter().map(String::as_str);
        match tab {
            LibraryTab::GlyphSets => {
                let glyphs = from.map(|i| match i.checked_sub(CATEGORIES.len()) {
                    Some(own) => self.settings.glyph_sets[own].glyphs.clone(),
                    None => CATEGORIES[i].1.to_owned(),
                });
                let base = from.map_or("My Glyphs".to_owned(), |i| format!("{} copy", names[i]));
                let set = GlyphSet { name: fresh_name(&base, taken()), glyphs: glyphs.unwrap_or_default() };
                self.settings.edit_glyph_sets(|sets| sets.push(set));
            }
            LibraryTab::Ramps => {
                let glyphs = from.map_or(self.tool_options.ramp.clone(), |i| self.settings.all_ramps()[i].glyphs.clone());
                let base = from.map_or("My Ramp".to_owned(), |i| format!("{} copy", names[i]));
                let ramp = Ramp { name: fresh_name(&base, taken()), glyphs };
                self.settings.edit_ramps(|ramps| ramps.push(ramp));
            }
            LibraryTab::Brushes => {
                let kind = from.map_or(self.tool_options.brush.clone(), |i| self.settings.all_brushes()[i].kind.clone());
                let base = from.map_or(self.tool_options.brush_name.clone(), |i| format!("{} copy", names[i]));
                let brush = BrushPreset { name: fresh_name(&base, taken()), kind };
                self.settings.edit_brushes(|brushes| brushes.push(brush));
            }
            LibraryTab::Palettes => {
                let panel = &self.color_panel;
                let colors = from.map_or(vec![panel.primary_fg.clone(), panel.secondary_fg.clone()], |i| self.settings.all_palettes()[i].colors.clone());
                let base = from.map_or("My Palette".to_owned(), |i| format!("{} copy", names[i]));
                let palette = Palette { name: fresh_name(&base, taken()), colors };
                self.settings.edit_palettes(|palettes| palettes.push(palette));
            }
        }
        let last = self.library_counts(tab).0 - 1;
        if let Some(window) = &mut self.library {
            window.selected = last;
        }
    }

    /// Deletes or moves the user's entry `own` (an index into their own list).
    fn library_edit_list(&mut self, tab: LibraryTab, own: usize, change: ListEdit) {
        fn apply<T>(list: &mut Vec<T>, own: usize, change: ListEdit) {
            match change {
                ListEdit::Delete => { list.remove(own); }
                ListEdit::Up if own > 0 => list.swap(own, own - 1),
                ListEdit::Down if own + 1 < list.len() => list.swap(own, own + 1),
                _ => {}
            }
        }
        match tab {
            LibraryTab::GlyphSets => self.settings.edit_glyph_sets(|l| apply(l, own, change)),
            LibraryTab::Ramps => self.settings.edit_ramps(|l| apply(l, own, change)),
            LibraryTab::Brushes => self.settings.edit_brushes(|l| apply(l, own, change)),
            LibraryTab::Palettes => self.settings.edit_palettes(|l| apply(l, own, change)),
        }
    }

    pub(super) fn library_window(&mut self, ctx: &egui::Context) {
        let Some(window) = &self.library else { return };
        let tab = window.tab;
        let (count, builtin) = self.library_counts(tab);
        let selected = window.selected.min(count.saturating_sub(1));
        let names = self.library_names(tab);
        let own = selected.checked_sub(builtin);
        let mut open = true;
        let (mut new_tab, mut pick) = (None, None);
        let (mut create, mut copy, mut list_edit, mut share, mut add_shared) = (false, false, None, false, false);

        egui::Window::new("Library")
            .id(Id::new("library"))
            .open(&mut open)
            .default_size([640.0, 440.0])
            .default_pos(pos2(ctx.content_rect().center().x - 320.0, 110.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for t in LibraryTab::ALL {
                        if ui.selectable_label(t == tab, t.name()).clicked() && t != tab {
                            new_tab = Some(t);
                        }
                    }
                });
                ui.separator();
                // A fixed height: the divider between list and editor would otherwise grow the
                // window to the bottom of the screen.
                let body = vec2(ui.available_width().max(560.0), 360.0);
                ui.allocate_ui_with_layout(body, Layout::left_to_right(Align::Min), |ui| {
                    ui.set_min_size(body);
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        ScrollArea::vertical().id_salt("library_list").max_height(300.0).auto_shrink([false, false]).show(ui, |ui| {
                            for (i, name) in names.iter().enumerate() {
                                if i == builtin && builtin < count {
                                    ui.separator();
                                }
                                let label = if i < builtin {
                                    RichText::new(format!("{}  {name}", icons::LOCK)).color(theme::TEXT_DIM)
                                } else {
                                    RichText::new(name)
                                };
                                if ui.selectable_label(i == selected, label).clicked() {
                                    pick = Some(i);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            create = ui.button(icons::PLUS).on_hover_text("New").clicked();
                            copy = ui.button(icons::DUPLICATE).on_hover_text("Copy this one (built-in ones too) to change it").clicked();
                            ui.add_enabled_ui(own.is_some(), |ui| {
                                if ui.button(icons::ARROW_UP).on_hover_text("Move up").clicked() { list_edit = Some(ListEdit::Up); }
                                if ui.button(icons::ARROW_DOWN).on_hover_text("Move down").clicked() { list_edit = Some(ListEdit::Down); }
                                if ui.button(icons::DELETE).on_hover_text("Delete").clicked() { list_edit = Some(ListEdit::Delete); }
                            });
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        self.library_editor(ui, tab, selected, own);
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    share = ui.button("Copy as Text").on_hover_text("Copies this entry as text, to share it").clicked();
                    let Some(window) = &mut self.library else { return };
                    ui.add(TextEdit::singleline(&mut window.paste).hint_text("Paste shared text here").desired_width(220.0));
                    add_shared = ui.add_enabled(!window.paste.trim().is_empty(), Button::new("Add")).clicked();
                    if let Some((error, text)) = &window.message {
                        ui.label(RichText::new(text).color(if *error { theme::WARNING } else { theme::TEXT_DIM }));
                    }
                });
            });

        let Some(window) = &mut self.library else { return };
        if let Some(t) = new_tab {
            window.tab = t;
            window.selected = 0;
            window.message = None;
        }
        if let Some(i) = pick {
            window.selected = i;
        } else {
            window.selected = selected;
        }
        if create || copy {
            self.library_new(tab, copy.then_some(selected));
        }
        if let (Some(change), Some(own)) = (list_edit, own) {
            self.library_edit_list(tab, own, change);
            if let Some(window) = &mut self.library {
                window.selected = match change {
                    ListEdit::Up => selected.saturating_sub(1).max(builtin),
                    ListEdit::Down => (selected + 1).min(count - 1),
                    ListEdit::Delete => selected.saturating_sub(1),
                };
            }
        }
        if share
            && let Some(text) = self.library_share(tab, selected)
        {
            ctx.copy_text(text);
            if let Some(window) = &mut self.library {
                window.message = Some((false, "Copied: paste it into anyone's Library".into()));
            }
        }
        if add_shared {
            let text = self.library.as_ref().map(|w| w.paste.clone()).unwrap_or_default();
            let result = self.library_add_shared(&text);
            if let Some(window) = &mut self.library {
                match result {
                    Ok(t) => {
                        window.tab = t;
                        window.paste.clear();
                        window.message = Some((false, "Added".into()));
                        window.selected = usize::MAX;
                    }
                    Err(e) => window.message = Some((true, e)),
                }
            }
        }
        if !open {
            self.library = None;
        }
    }

    /// The editor of entry `selected`; built-in ones are shown, not changed.
    fn library_editor(&mut self, ui: &mut Ui, tab: LibraryTab, selected: usize, own: Option<usize>) {
        let read_only = own.is_none();
        if read_only {
            ui.label(RichText::new("Built in: copy it to make one of your own").color(theme::TEXT_FAINT));
        }
        match tab {
            LibraryTab::GlyphSets => {
                let (mut name, mut glyphs) = match own {
                    Some(o) => (self.settings.glyph_sets[o].name.clone(), self.settings.glyph_sets[o].glyphs.clone()),
                    None => (CATEGORIES[selected].0.to_owned(), CATEGORIES[selected].1.to_owned()),
                };
                let changed = name_field(ui, &mut name, read_only)
                    | ui.add_enabled(!read_only, TextEdit::multiline(&mut glyphs).desired_rows(3).desired_width(f32::INFINITY).hint_text("Type or paste glyphs")).changed();
                ScrollArea::vertical().id_salt("library_preview").max_height(190.0).show(ui, |ui| glyph_strip(ui, &glyphs, 14.0));
                if changed && let Some(o) = own {
                    self.settings.edit_glyph_sets(|sets| sets[o] = GlyphSet { name, glyphs });
                }
            }
            LibraryTab::Ramps => {
                let ramp = self.settings.all_ramps()[selected].clone();
                let (mut name, mut glyphs) = (ramp.name.clone(), ramp.glyphs.clone());
                let mut changed = name_field(ui, &mut name, read_only);
                ui.horizontal(|ui| {
                    changed |= ui.add_enabled(!read_only, TextEdit::singleline(&mut glyphs).desired_width(260.0).font(TextStyle::Monospace)).changed();
                    if ui.add_enabled(!read_only, Button::new("Reverse")).on_hover_text("Ramps go from empty to full").clicked() {
                        glyphs = glyphs.chars().rev().collect();
                        changed = true;
                    }
                });
                ui.label(RichText::new("From empty to full; Shade, Gradient, 3D and picture import use it").small().color(theme::TEXT_FAINT));
                glyph_strip(ui, &glyphs, 14.0);
                // The ramp stretched over a strip, as a gradient would draw it.
                let chars: Vec<char> = glyphs.chars().collect();
                if chars.len() > 1 {
                    let strip: String = (0..40).map(|i| chars[(i * (chars.len() - 1) + 20) / 39]).collect();
                    glyph_strip(ui, &strip, 8.0);
                }
                ui.horizontal(|ui| {
                    if ui.button("Use for Shade and Gradient").clicked() {
                        self.tool_options.ramp = glyphs.clone();
                        self.tool_options.ramp_name = name.clone();
                    }
                });
                if changed && let Some(o) = own {
                    // Tools using the ramp follow the change.
                    if self.tool_options.ramp_name == ramp.name {
                        self.tool_options.ramp = glyphs.clone();
                        self.tool_options.ramp_name = name.clone();
                    }
                    self.settings.edit_ramps(|ramps| ramps[o] = Ramp { name, glyphs });
                }
            }
            LibraryTab::Brushes => {
                let brush = self.settings.all_brushes()[selected].clone();
                let mut name = brush.name.clone();
                let changed = name_field(ui, &mut name, read_only);
                ui.label(RichText::new(describe(&brush.kind)).color(theme::TEXT_DIM));
                if let BrushKind::Stamp { cells, .. } = &brush.kind {
                    let rows: Vec<String> = cells.iter().map(|row| row.iter().map(|c| c.ch).collect()).collect();
                    glyph_strip(ui, &rows.join(""), 10.0);
                }
                ui.label(RichText::new("Change a brush's settings in the options bar, then save it again").small().color(theme::TEXT_FAINT));
                ui.horizontal(|ui| {
                    if ui.button("Use").clicked() {
                        self.tool_options.brush = brush.kind.clone();
                        self.tool_options.brush_name = brush.name.clone();
                        self.active_tool = ToolType::Brush;
                    }
                    if let BrushKind::Script { code } = &brush.kind
                        && ui.button("Edit Script…").clicked()
                    {
                        let code = code.clone();
                        self.open_script();
                        if let Some(script) = &mut self.script {
                            script.code = code;
                        }
                    }
                });
                if changed && let Some(o) = own {
                    self.settings.edit_brushes(|brushes| brushes[o].name = name);
                }
            }
            LibraryTab::Palettes => {
                let palette = self.settings.all_palettes()[selected].clone();
                let (mut name, mut colors) = (palette.name.clone(), palette.colors.clone());
                let mut changed = name_field(ui, &mut name, read_only);
                ScrollArea::vertical().id_salt("library_swatches").max_height(200.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let mut remove = None;
                        for (i, color) in colors.iter_mut().enumerate() {
                            let mut c = color.to_egui();
                            ui.add_enabled_ui(!read_only, |ui| {
                                let resp = color_picker::color_edit_button_srgba(ui, &mut c, color_picker::Alpha::OnlyBlend);
                                if resp.changed() {
                                    *color = Color::from_egui(c);
                                    changed = true;
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("Remove").clicked() {
                                        remove = Some(i);
                                        ui.close();
                                    }
                                });
                            });
                        }
                        if let Some(i) = remove {
                            colors.remove(i);
                            changed = true;
                        }
                    });
                });
                ui.add_enabled_ui(!read_only, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Add Primary Colour").clicked() {
                            colors.push(self.color_panel.primary_fg.clone());
                            changed = true;
                        }
                        if ui.button("From the Picture").on_hover_text("Add the colours the picture uses (up to 64)").clicked() {
                            for y in 0..self.doc.height {
                                for x in 0..self.doc.width {
                                    let cell = self.doc.composite_cell(x, y);
                                    for c in [cell.fg, cell.bg] {
                                        if (cell.ch != ' ' || !c.is_transparent()) && !c.is_transparent() && colors.len() < 64 && !colors.contains(&c) {
                                            colors.push(c);
                                        }
                                    }
                                }
                            }
                            changed = true;
                        }
                    });
                });
                ui.label(RichText::new("Right click a colour to remove it").small().color(theme::TEXT_FAINT));
                if changed && let Some(o) = own {
                    self.settings.edit_palettes(|palettes| palettes[o] = Palette { name, colors });
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ListEdit {
    Up,
    Down,
    Delete,
}

fn name_field(ui: &mut Ui, name: &mut String, read_only: bool) -> bool {
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.add_enabled(!read_only, TextEdit::singleline(name).desired_width(200.0)).changed()
    })
    .inner
}

fn describe(kind: &BrushKind) -> String {
    match kind {
        BrushKind::Square => "The glyph in a square".into(),
        BrushKind::Stamp { cells, recolor } => {
            let colours = if *recolor { "slot colours" } else { "its own colours" };
            format!("Stamp {}×{}, {colours}", cells.first().map_or(0, Vec::len), cells.len())
        }
        BrushKind::Scatter { glyphs, density } => format!("Scatter {glyphs} on {:.0}% of the cells", density * 100.0),
        BrushKind::Direction { glyphs } => format!("Glyphs by direction: {glyphs}"),
        BrushKind::Script { .. } => "A script's fn brush(x, y, dx, dy)".into(),
    }
}
