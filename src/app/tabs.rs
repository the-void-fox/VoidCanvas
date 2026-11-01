//! Several documents open at once, one per tab.
//!
//! The document in front lives in the editor's own fields (`doc`, `history`, …), so the rest of
//! the editor works on it directly; the others wait in `tabs` and are swapped in when chosen.

use std::mem;

use eframe::egui::{self, *};

use super::{AsciiArtEditor, HISTORY_STEPS, PendingAction, commands::Action, storage};
use crate::document::Document;
use crate::history::History;
use crate::tools::selection::SelectionTool;
use crate::ui::canvas::CanvasState;
use crate::ui::widgets::icon_button;
use crate::{icons, theme};

/// A document waiting in a background tab.
pub struct Tab {
    doc: Document,
    history: History,
    canvas_state: CanvasState,
    selection: SelectionTool,
    autosaved_version: u64,
}

impl Tab {
    fn new(doc: Document) -> Self {
        let mut canvas_state = CanvasState::default();
        canvas_state.request_fit();
        let history = History::new(HISTORY_STEPS);
        let autosaved_version = history.version();
        Self { doc, history, canvas_state, selection: SelectionTool::default(), autosaved_version }
    }
}

const TAB_HEIGHT: f32 = 30.0;

/// The file name of `doc`, or "Untitled".
pub fn display_name(doc: &Document) -> String {
    doc.file_path
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_name())
        .map_or_else(|| "Untitled".to_owned(), |n| n.to_string_lossy().to_string())
}

fn same_file(a: &str, b: &str) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

impl AsciiArtEditor {
    /// Swaps the document in front with `tab`'s. The grid setting belongs to the editor, not the tab.
    fn swap_front(&mut self, tab: &mut Tab) {
        mem::swap(&mut self.doc, &mut tab.doc);
        mem::swap(&mut self.history, &mut tab.history);
        mem::swap(&mut self.tools.selection, &mut tab.selection);
        mem::swap(&mut self.autosaved_version, &mut tab.autosaved_version);
        let (grid, onion) = (self.canvas_state.show_grid, self.canvas_state.onion_skin);
        mem::swap(&mut self.canvas_state, &mut tab.canvas_state);
        self.canvas_state.show_grid = grid;
        self.canvas_state.onion_skin = onion;
        self.stop_playback();
        self.close_3d();
        self.tools.text.stop();
        self.layer_panel.rename_idx = None;
    }

    pub(crate) fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub(crate) fn active_tab(&self) -> usize {
        self.active_tab
    }

    /// Every open document in tab order, the one in front included.
    pub(crate) fn documents(&self) -> impl Iterator<Item = &Document> {
        self.tabs.iter().map(|tab| tab.as_ref().map_or(&self.doc, |tab| &tab.doc))
    }

    pub(crate) fn switch_tab(&mut self, i: usize) {
        if i == self.active_tab || i >= self.tabs.len() || self.drawing { return; }
        let mut tab = self.tabs[i].take().expect("a background tab");
        self.swap_front(&mut tab);
        self.tabs[self.active_tab] = Some(tab);
        self.active_tab = i;
    }

    /// The next tab (`step` 1) or the previous one (`step` -1), wrapping around.
    pub(super) fn cycle_tab(&mut self, step: isize) {
        let n = self.tabs.len() as isize;
        self.switch_tab((self.active_tab as isize + step).rem_euclid(n) as usize);
    }

    /// A blank document nobody has touched yet: opening another one takes its place.
    fn untouched(&self) -> bool {
        self.doc.file_path.is_none() && !self.doc.modified && !self.history.can_undo() && !self.history.can_redo()
    }

    /// Shows `doc` in a new tab next to the current one.
    pub(crate) fn open_in_tab(&mut self, doc: Document) {
        let mut tab = Tab::new(doc);
        let replace = self.untouched();
        self.swap_front(&mut tab);
        if !replace {
            // `tab` now holds the document that was in front.
            self.tabs[self.active_tab] = Some(tab);
            self.tabs.insert(self.active_tab + 1, None);
            self.active_tab += 1;
        }
    }

    /// The tab showing the file at `path`, if it is open.
    pub(super) fn tab_with_file(&self, path: &str) -> Option<usize> {
        self.documents().position(|doc| doc.file_path.as_deref().is_some_and(|p| same_file(p, path)))
    }

    /// The first tab with unsaved changes, the one in front first.
    pub(super) fn first_unsaved_tab(&self) -> Option<usize> {
        if self.doc.modified { return Some(self.active_tab); }
        self.documents().position(|doc| doc.modified)
    }

    /// Closes the tab in front without asking (that was done before). The last tab
    /// leaves an empty document behind.
    pub(super) fn close_front_tab(&mut self) {
        if self.tabs.len() == 1 {
            let mut fresh = Tab::new(Document::new(80, 40));
            self.swap_front(&mut fresh);
        } else {
            self.tabs.remove(self.active_tab);
            self.active_tab = self.active_tab.min(self.tabs.len() - 1);
            let mut tab = self.tabs[self.active_tab].take().expect("a background tab");
            // The closed document ends up in `tab` and is dropped.
            self.swap_front(&mut tab);
        }
        self.refresh_autosave();
    }

    /// Unsaved work changed since the last autosave, in any tab.
    pub(super) fn autosave_due(&self) -> bool {
        let changed = |doc: &Document, history: &History, version: u64| doc.modified && history.version() != version;
        changed(&self.doc, &self.history, self.autosaved_version)
            || self.tabs.iter().flatten().any(|tab| changed(&tab.doc, &tab.history, tab.autosaved_version))
    }

    /// Writes every unsaved document to the recovery file now (or removes it when all are saved).
    pub(super) fn refresh_autosave(&mut self) {
        storage::Autosave::write(&self.documents().filter(|doc| doc.modified).collect::<Vec<_>>());
        self.autosaved_version = self.history.version();
        for tab in self.tabs.iter_mut().flatten() {
            tab.autosaved_version = tab.history.version();
        }
    }

    /// One tab per open document, above the canvas.
    pub(super) fn tab_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut switch = None;
        let mut close = None;
        let mut new = false;
        egui::Panel::top("tabs")
            .frame(Frame::NONE.fill(theme::PANEL).inner_margin(Margin { left: 6, right: 6, top: 4, bottom: 0 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                ScrollArea::horizontal().scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    // A fixed height from the start, so every tab lines up (a growing row would centre
                    // the first one differently), with the tabs sitting on the canvas below.
                    let row = vec2(ui.available_width(), TAB_HEIGHT);
                    ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Max), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let tabs: Vec<(String, Option<String>, bool)> = self
                            .documents()
                            .map(|doc| (display_name(doc), doc.file_path.clone(), doc.modified))
                            .collect();
                        for (i, (name, path, modified)) in tabs.into_iter().enumerate() {
                            let active = i == self.active_tab;
                            let tab = ui.scope_builder(UiBuilder::new().id_salt(("tab", i)).sense(Sense::click()), |ui| {
                                let hovered = ui.response().hovered();
                                let fill = if active {
                                    theme::BG_DEEP
                                } else if hovered {
                                    theme::SURFACE
                                } else {
                                    Color32::TRANSPARENT
                                };
                                Frame::new()
                                    .fill(fill)
                                    .corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
                                    .inner_margin(Margin { left: 12, right: 4, top: 5, bottom: 5 })
                                    .show(ui, |ui| ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        let color = if active { theme::TEXT } else { theme::TEXT_DIM };
                                        ui.add(Label::new(RichText::new(&name).color(color)).selectable(false));
                                        if modified {
                                            ui.label(RichText::new("●").color(theme::WARNING).size(10.0));
                                        }
                                        let x = Button::new(RichText::new(icons::CLOSE).font(theme::mono(12.0)).color(theme::TEXT_FAINT))
                                            .frame(false);
                                        ui.add(x).on_hover_text("Close  (Ctrl+W)").clicked()
                                    }).inner)
                                    .inner
                            });
                            let resp = tab.response.on_hover_text(path.unwrap_or_else(|| "Not saved yet".to_owned()));
                            if tab.inner || resp.middle_clicked() {
                                close = Some(i);
                            } else if resp.clicked() {
                                switch = Some(i);
                            }
                        }
                        ui.add_space(4.0);
                        new = icon_button(ui, icons::PLUS, 22.0, false, "New document  (Ctrl+N)").clicked();
                    });
                });
            });
        if let Some(i) = switch {
            self.switch_tab(i);
        }
        if let Some(i) = close {
            self.switch_tab(i);
            self.request(PendingAction::CloseTab, &ctx);
        }
        if new {
            self.run(Action::New, &ctx);
        }
    }
}
