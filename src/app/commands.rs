//! Everything the editor can do by name, and the command palette (Ctrl+Shift+P) that finds it.

use eframe::egui::{self, text::LayoutJob, *};

use super::{AsciiArtEditor, DockTab, PendingAction, TRANSFORMS, file_dialog::FileAction};
use crate::glyphs::Transform;
use crate::theme;
use crate::tools::ToolType;
use crate::ui::{layer_panel::LayerAction, panels::PALETTES};

#[derive(Clone, PartialEq)]
pub enum Action {
    New,
    Open,
    OpenRecent(String),
    Save,
    /// Save As, Import Image and the exports: a file is chosen first.
    ChooseFile(FileAction),
    Quit,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    CopyAs { ansi: bool },
    Delete,
    Transform(Transform),
    SelectAll,
    Deselect,
    Replace,
    InsertBanner,
    ClearLayer,
    Resize,
    ZoomIn,
    ZoomOut,
    Fit,
    ActualSize,
    ToggleGrid,
    Tool(ToolType),
    SwapSlots,
    Palette(usize),
    Layer(LayerAction),
    Shortcuts,
    Author,
    CommandPalette,
}

/// One line of the command palette.
pub struct Entry {
    pub action: Action,
    pub group: &'static str,
    pub name: String,
    pub shortcut: &'static str,
}

impl Entry {
    /// Identifies the entry among the recently used ones.
    fn key(&self) -> String {
        format!("{}: {}", self.group, self.name)
    }
}

/// The open command palette.
#[derive(Default)]
pub struct CommandPalette {
    pub query: String,
    /// Index into the filtered list.
    selected: usize,
}

const MAX_RECENT_COMMANDS: usize = 8;

impl AsciiArtEditor {
    /// The actions that make sense right now.
    pub(crate) fn actions(&self) -> Vec<Entry> {
        let mut list = Vec::new();
        let mut add = |group, name: &str, shortcut, action| list.push(Entry { action, group, name: name.to_owned(), shortcut });
        let has_selection = self.tools.selection.selection.is_some();

        add("File", "New…", "Ctrl+N", Action::New);
        add("File", "Open…", "Ctrl+O", Action::Open);
        for path in &self.settings.recent_files {
            let name = std::path::Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().to_string());
            add("Open Recent", &name, "", Action::OpenRecent(path.clone()));
        }
        add("File", "Save", "Ctrl+S", Action::Save);
        add("File", "Save As…", "Ctrl+Shift+S", Action::ChooseFile(FileAction::SaveAs { then: None }));
        add("File", "Import Image…", "", Action::ChooseFile(FileAction::ImportImage));
        add("File", "Export Text…", "", Action::ChooseFile(FileAction::ExportText));
        add("File", "Export ANSI…", "", Action::ChooseFile(FileAction::ExportAnsi));
        add("File", "Export HTML…", "", Action::ChooseFile(FileAction::ExportHtml));
        add("File", "Export PNG…", "", Action::ChooseFile(FileAction::ExportPng { transparent: false }));
        add("File", "Export PNG (Transparent)…", "", Action::ChooseFile(FileAction::ExportPng { transparent: true }));
        add("File", "Quit", "", Action::Quit);

        if self.history.can_undo() { add("Edit", "Undo", "Ctrl+Z", Action::Undo); }
        if self.history.can_redo() { add("Edit", "Redo", "Ctrl+Y", Action::Redo); }
        if has_selection {
            add("Edit", "Copy", "Ctrl+C", Action::Copy);
            add("Edit", "Cut", "Ctrl+X", Action::Cut);
        }
        if self.clipboard.is_some() { add("Edit", "Paste", "Ctrl+V", Action::Paste); }
        add("Edit", "Copy As Plain Text", "", Action::CopyAs { ansi: false });
        add("Edit", "Copy As ANSI Colours", "", Action::CopyAs { ansi: true });
        add("Edit", "Select All", "Ctrl+A", Action::SelectAll);
        if has_selection {
            add("Edit", "Deselect", "Ctrl+D", Action::Deselect);
            add("Edit", "Delete", "Del", Action::Delete);
            for (t, label, key) in TRANSFORMS {
                add("Transform", label, key, Action::Transform(*t));
            }
        }
        add("Edit", "Replace…", "Ctrl+H", Action::Replace);
        add("Edit", "Insert Banner…", "", Action::InsertBanner);
        add("Edit", "Clear Layer", "", Action::ClearLayer);

        add("Canvas", "Resize…", "", Action::Resize);
        add("View", "Zoom In", "Ctrl++", Action::ZoomIn);
        add("View", "Zoom Out", "Ctrl+-", Action::ZoomOut);
        add("View", "Fit to Window", "Ctrl+0", Action::Fit);
        add("View", "Actual Size", "Ctrl+1", Action::ActualSize);
        add("View", if self.canvas_state.show_grid { "Hide Grid" } else { "Show Grid" }, "", Action::ToggleGrid);

        for &tool in ToolType::groups().iter().flat_map(|group| group.iter()) {
            add("Tool", tool.name(), tool_key(tool), Action::Tool(tool));
        }
        add("Colour", "Swap Primary and Secondary", "X", Action::SwapSlots);
        for (i, name) in PALETTES.iter().enumerate() {
            add("Palette", name, "", Action::Palette(i));
        }

        let layers = self.doc.layers.len();
        let active = self.doc.active_layer;
        let layer = self.doc.active_layer();
        add("Layer", "New Layer", "", Action::Layer(LayerAction::Add));
        add("Layer", "Duplicate Layer", "", Action::Layer(LayerAction::Duplicate));
        if layers > 1 { add("Layer", "Delete Layer", "", Action::Layer(LayerAction::Remove)); }
        if active > 0 {
            add("Layer", "Merge Down", "", Action::Layer(LayerAction::MergeDown));
            add("Layer", "Move Layer Down", "", Action::Layer(LayerAction::MoveDown));
        }
        if active + 1 < layers { add("Layer", "Move Layer Up", "", Action::Layer(LayerAction::MoveUp)); }
        add("Layer", if layer.visible { "Hide Layer" } else { "Show Layer" }, "", Action::Layer(LayerAction::ToggleVisible(active)));
        add("Layer", if layer.locked { "Unlock Layer" } else { "Lock Layer" }, "", Action::Layer(LayerAction::ToggleLock(active)));

        add("Help", "Keyboard Shortcuts", "", Action::Shortcuts);
        add("Help", "Author", "", Action::Author);
        list
    }

    pub(super) fn run(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::New => self.request(PendingAction::New, ctx),
            Action::Open => self.request(PendingAction::Open, ctx),
            Action::OpenRecent(path) => self.request(PendingAction::OpenPath(path.into()), ctx),
            Action::Save => self.save(ctx),
            Action::ChooseFile(file) => self.choose_file(file, ctx),
            Action::Quit => self.request(PendingAction::Quit, ctx),
            Action::Undo => {
                if !self.drawing && self.history.undo(&mut self.doc) { self.show_status("Undo"); }
            }
            Action::Redo => {
                if !self.drawing && self.history.redo(&mut self.doc) { self.show_status("Redo"); }
            }
            Action::Copy => { self.copy(ctx); }
            Action::Cut => self.cut(ctx),
            Action::Paste => self.paste(None),
            Action::CopyAs { ansi } => self.copy_as(ctx, ansi),
            Action::Delete => self.delete_selection(),
            Action::Transform(t) => self.transform_selection(t),
            Action::SelectAll => {
                if self.drawing { return; }
                self.tools.selection.clear();
                self.tools.selection.selection = Some((0, 0, self.doc.width - 1, self.doc.height - 1));
            }
            Action::Deselect => {
                if !self.drawing { self.tools.selection.selection = None; }
            }
            Action::Replace => self.open_replace(),
            Action::InsertBanner => self.open_banner(),
            Action::ClearLayer => self.clear_layer(),
            Action::Resize => {
                self.new_width  = self.doc.width;
                self.new_height = self.doc.height;
                self.show_resize_dialog = true;
            }
            Action::ZoomIn => self.canvas_state.zoom_view_center(1.25),
            Action::ZoomOut => self.canvas_state.zoom_view_center(1.0 / 1.25),
            Action::Fit => self.canvas_state.request_fit(),
            Action::ActualSize => self.canvas_state.actual_size(),
            Action::ToggleGrid => self.canvas_state.show_grid = !self.canvas_state.show_grid,
            Action::Tool(tool) => self.active_tool = tool,
            Action::SwapSlots => self.swap_slots(),
            Action::Palette(i) => {
                self.color_panel.palette = i;
                self.dock_tab = DockTab::Colors;
            }
            Action::Layer(action) => self.apply_layer_action(action),
            Action::Shortcuts | Action::Author => {
                self.about_tab = if action == Action::Author { 1 } else { 0 };
                self.show_about_dialog = true;
            }
            Action::CommandPalette => self.command_palette = Some(CommandPalette::default()),
        }
    }

    /// The entries matching the query, best first; with no query the recently used ones lead.
    pub(crate) fn palette_matches(&self, entries: &[Entry], query: &str) -> Vec<usize> {
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.is_empty() {
            let recent = &self.settings.recent_commands;
            let rank = |e: &Entry| recent.iter().position(|k| *k == e.key()).unwrap_or(usize::MAX);
            let mut order: Vec<usize> = (0..entries.len()).collect();
            order.sort_by_key(|&i| rank(&entries[i]));
            return order;
        }
        let mut scored: Vec<(i32, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let text = e.key();
                words.iter().map(|w| fuzzy_score(w, &text)).sum::<Option<i32>>().map(|s| (s, i))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, i)| i).collect()
    }

    pub(super) fn command_palette_dialog(&mut self, ctx: &egui::Context) {
        if self.command_palette.is_none() { return; }
        let entries = self.actions();
        let query = self.command_palette.as_ref().map(|p| p.query.clone()).unwrap_or_default();
        let matches = self.palette_matches(&entries, &query);
        let Some(palette) = &mut self.command_palette else { return };

        // Arrows and Enter steer the list, not the text field.
        let (up, down, enter) = ctx.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::Enter))
        });
        if down { palette.selected = (palette.selected + 1).min(matches.len().saturating_sub(1)); }
        if up { palette.selected = palette.selected.saturating_sub(1); }
        palette.selected = palette.selected.min(matches.len().saturating_sub(1));
        let mut chosen = if enter { matches.get(palette.selected).copied() } else { None };

        let id = Id::new("command_palette");
        let area = Modal::default_area(id).anchor(Align2::CENTER_TOP, vec2(0.0, 80.0));
        let modal = Modal::new(id).area(area).show(ctx, |ui| {
            ui.set_width(560.0);
            let edit = ui.add(
                TextEdit::singleline(&mut palette.query)
                    .hint_text("Type a command, a tool or a layer action")
                    .desired_width(f32::INFINITY)
                    .margin(Margin::symmetric(8, 6)),
            );
            edit.request_focus();
            if edit.changed() { palette.selected = 0; }
            ui.add_space(4.0);

            ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                if matches.is_empty() {
                    ui.label(RichText::new("Nothing matches").color(theme::TEXT_FAINT));
                }
                let style = ui.style().clone();
                for (row, &i) in matches.iter().enumerate() {
                    let entry = &entries[i];
                    let selected = row == palette.selected;
                    let mut job = LayoutJob::default();
                    let group = if selected { theme::TEXT_DIM } else { theme::TEXT_FAINT };
                    RichText::new(format!("{}  ", entry.group)).color(group)
                        .append_to(&mut job, &style, FontSelection::Default, Align::Center);
                    RichText::new(&entry.name).color(theme::TEXT)
                        .append_to(&mut job, &style, FontSelection::Default, Align::Center);
                    let button = Button::selectable(selected, job)
                        .shortcut_text(RichText::new(entry.shortcut).font(theme::mono(12.0)))
                        .min_size(vec2(ui.available_width(), 26.0));
                    let resp = ui.add(button);
                    if selected && (up || down) {
                        resp.scroll_to_me(None);
                    }
                    if resp.clicked() {
                        chosen = Some(i);
                    }
                }
            });
        });

        if let Some(i) = chosen {
            let entry = &entries[i];
            self.command_palette = None;
            self.settings.add_recent_command(entry.key(), MAX_RECENT_COMMANDS);
            self.run(entry.action.clone(), ctx);
        } else if modal.should_close() {
            self.command_palette = None;
        }
    }
}

/// The tool's single-key shortcut, as text.
fn tool_key(tool: ToolType) -> &'static str {
    const KEYS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let i = (tool.key() as u8 - b'A') as usize;
    &KEYS[i..i + 1]
}

/// How well `query` matches `text` as a subsequence (case aside), or `None` if it doesn't.
/// Letters in a row and letters at word starts count more; gaps count less.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let query: Vec<char> = query.to_lowercase().chars().collect();
    // Jumping ahead to a word start can skip letters the rest of the query needs;
    // then the first occurrences still match.
    score_matches(&query, &text, true).or_else(|| score_matches(&query, &text, false))
}

fn score_matches(query: &[char], text: &[char], prefer_word_starts: bool) -> Option<i32> {
    let word_start = |i: usize| i == 0 || !text[i - 1].is_alphanumeric();
    let mut score = 0;
    let mut next = 0;
    let mut last: Option<usize> = None;
    for &q in query {
        let mut found = (next..text.len()).filter(|&i| text[i] == q);
        let first = found.next()?;
        let run = last.is_some_and(|l| l + 1 == first);
        let i = if prefer_word_starts && !run && !word_start(first) {
            found.find(|&i| word_start(i)).unwrap_or(first)
        } else {
            first
        };
        score += 1;
        if last.is_some_and(|l| l + 1 == i) { score += 4; }
        if word_start(i) { score += 3; }
        score -= (i - next).min(5) as i32 / 2;
        last = Some(i);
        next = i + 1;
    }
    Some(score)
}
