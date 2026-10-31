use eframe::egui::{self, *};

use crate::document::{Cell, Changes, Document, Layer};
use crate::history::{Command, History};
use crate::tools::{
    DrawContext, ToolType, merge_changes,
    brush::BrushTool,
    eraser::EraserTool,
    fill::FillTool,
    shapes::{RectangleTool, EllipseTool, LineTool},
    border::BorderTool,
    selection::{SelectionTool, content_to_text, text_to_content},
};
use crate::ui::{
    canvas::{CanvasEvent, CanvasState, draw_canvas},
    char_palette::CharPalette,
    layer_panel::{LayerAction, LayerPanel},
    panels::{ColorPanel, ToolOptionsPanel},
};

const APP_NAME: &str = "Void Canvas";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct ToolState {
    pub brush:     BrushTool,
    pub eraser:    EraserTool,
    pub fill:      FillTool,
    pub rect:      RectangleTool,
    pub ellipse:   EllipseTool,
    pub line:      LineTool,
    pub border:    BorderTool,
    pub selection: SelectionTool,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            brush:     BrushTool::default(),
            eraser:    EraserTool::default(),
            fill:      FillTool::default(),
            rect:      RectangleTool::default(),
            ellipse:   EllipseTool::default(),
            line:      LineTool::default(),
            border:    BorderTool::default(),
            selection: SelectionTool::default(),
        }
    }
}

/// An action that would throw away unsaved work; it waits for the user's answer.
#[derive(Clone, Copy, PartialEq)]
enum PendingAction {
    New,
    Open,
    Quit,
}

pub struct AsciiArtEditor {
    pub doc:          Document,
    pub history:      History,
    pub active_tool:  ToolType,
    pub tools:        ToolState,
    pub canvas_state: CanvasState,
    pub char_palette: CharPalette,
    pub color_panel:  ColorPanel,
    pub layer_panel:  LayerPanel,
    pub tool_options: ToolOptionsPanel,
    pub primary_char:   char,
    pub secondary_char: char,
    pub clipboard: Option<Vec<Vec<Cell>>>,
    /// Text put on the system clipboard together with `clipboard`.
    pub clipboard_text: String,
    pub drawing: bool,
    pub last_primary: bool,
    /// Tool that started the current stroke; it gets the rest of the stroke's events.
    pub stroke_tool: ToolType,
    /// Changes made so far by the current Select-tool drag.
    pub stroke_changes: Changes,
    pub status_msg: String,
    pub status_timer: f32,
    pub show_new_dialog:    bool,
    pub show_resize_dialog: bool,
    pub show_about_dialog:  bool,
    pub about_tab:          usize, // 0 = About & Shortcuts, 1 = Author
    pub new_width:  usize,
    pub new_height: usize,
    pending_action: Option<PendingAction>,
    /// Set once the user agreed to quit, so the next close request goes through.
    allow_close: bool,
    window_title: String,
}

impl Default for AsciiArtEditor {
    fn default() -> Self {
        Self {
            doc:          Document::new(80, 40),
            history:      History::new(100),
            active_tool:  ToolType::Brush,
            tools:        ToolState::default(),
            canvas_state: CanvasState::default(),
            char_palette: CharPalette::default(),
            color_panel:  ColorPanel::default(),
            layer_panel:  LayerPanel::default(),
            tool_options: ToolOptionsPanel::default(),
            primary_char:   '#',
            secondary_char: '.',
            clipboard:      None,
            clipboard_text: String::new(),
            drawing:      false,
            last_primary: true,
            stroke_tool:    ToolType::Brush,
            stroke_changes: Changes::new(),
            status_msg:  String::from("Ready"),
            status_timer: 0.0,
            show_new_dialog:    false,
            show_resize_dialog: false,
            show_about_dialog:  false,
            about_tab:          0,
            new_width:  80,
            new_height: 40,
            pending_action: None,
            allow_close:    false,
            window_title:   String::new(),
        }
    }
}

impl AsciiArtEditor {
    fn draw_context(&self, is_primary: bool) -> DrawContext {
        DrawContext {
            primary_char:   self.primary_char,
            secondary_char: self.secondary_char,
            primary_fg:     self.color_panel.primary_fg.clone(),
            primary_bg:     self.color_panel.primary_bg.clone(),
            secondary_fg:   self.color_panel.secondary_fg.clone(),
            secondary_bg:   self.color_panel.secondary_bg.clone(),
            brush_size:     self.tool_options.brush_size,
            is_primary,
        }
    }

    /// Records changes that were already applied to the active layer.
    fn commit_draw(&mut self, changes: Changes) {
        if changes.is_empty() { return; }
        self.history.push(Command::Draw { layer_idx: self.doc.active_layer, changes });
        self.doc.modified = true;
    }

    /// Shows a message and returns `true` if the active layer can't be edited.
    fn active_layer_locked(&mut self) -> bool {
        let locked = self.doc.active_layer().locked;
        if locked {
            self.show_status("Layer is locked");
        }
        locked
    }

    fn handle_canvas_event(&mut self, event: CanvasEvent) {
        match event {
            CanvasEvent::Press { x, y, primary } => {
                if self.drawing { return; }
                let tool = self.active_tool;
                let edits = tool != ToolType::Select || self.tools.selection.hit(x, y);
                if edits && self.active_layer_locked() { return; }

                self.sync_tool_options();
                let ctx = self.draw_context(primary);

                if tool == ToolType::Fill {
                    let changes = self.tools.fill.flood_fill(&mut self.doc, x, y, &ctx);
                    self.commit_draw(changes);
                    return;
                }

                self.drawing = true;
                self.stroke_tool = tool;
                self.last_primary = primary;
                self.stroke_changes.clear();

                match tool {
                    ToolType::Brush     => { self.tools.brush.on_press(&mut self.doc, x, y, &ctx); }
                    ToolType::Eraser    => { self.tools.eraser.on_press(&mut self.doc, x, y, &ctx); }
                    ToolType::Rectangle => self.tools.rect.on_press(x, y),
                    ToolType::Ellipse   => self.tools.ellipse.on_press(x, y),
                    ToolType::Line      => self.tools.line.on_press(x, y),
                    ToolType::Border    => self.tools.border.on_press(x, y),
                    ToolType::Select    => { self.tools.selection.on_press(&self.doc, x, y); }
                    ToolType::Fill      => {}
                }
            }

            CanvasEvent::Drag { x, y } => {
                if !self.drawing { return; }
                let ctx = self.draw_context(self.last_primary);
                match self.stroke_tool {
                    ToolType::Brush  => { self.tools.brush.on_drag(&mut self.doc, x, y, &ctx); }
                    ToolType::Eraser => { self.tools.eraser.on_drag(&mut self.doc, x, y, &ctx); }
                    ToolType::Select => {
                        let changes = self.tools.selection.on_drag(&mut self.doc, x, y);
                        merge_changes(&mut self.stroke_changes, changes);
                    }
                    _ => {}
                }
            }

            CanvasEvent::Release { x, y } => {
                if !self.drawing { return; }
                self.drawing = false;

                let ctx = self.draw_context(self.last_primary);
                let changes = match self.stroke_tool {
                    ToolType::Brush     => self.tools.brush.on_release(),
                    ToolType::Eraser    => self.tools.eraser.on_release(),
                    ToolType::Rectangle => self.tools.rect.on_release(&mut self.doc, x, y, &ctx),
                    ToolType::Ellipse   => self.tools.ellipse.on_release(&mut self.doc, x, y, &ctx),
                    ToolType::Line      => self.tools.line.on_release(&mut self.doc, x, y, &ctx),
                    ToolType::Border    => self.tools.border.on_release(&mut self.doc, x, y, &ctx),
                    ToolType::Select    => self.tools.selection.on_release(&mut self.doc),
                    ToolType::Fill      => Changes::new(),
                };

                let mut all = std::mem::take(&mut self.stroke_changes);
                merge_changes(&mut all, changes);
                self.commit_draw(all);
            }
        }
    }

    fn sync_tool_options(&mut self) {
        self.tools.rect.filled    = self.tool_options.filled;
        self.tools.ellipse.filled = self.tool_options.filled;
        self.tools.border.style   = self.tool_options.border_style;
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Keys typed into a text field (char search, layer rename, …) belong to that field.
        if ctx.text_edit_focused() || self.pending_action.is_some() { return; }

        // Ctrl+C / Ctrl+X / Ctrl+V arrive as Copy / Cut / Paste events, not as key presses.
        let (copy, cut, paste) = ctx.input(|i| {
            let mut out = (false, false, None);
            for event in &i.events {
                match event {
                    Event::Copy        => out.0 = true,
                    Event::Cut         => out.1 = true,
                    Event::Paste(text) => out.2 = Some(text.clone()),
                    _ => {}
                }
            }
            out
        });
        if copy { self.copy(ctx); }
        if cut { self.cut(ctx); }
        if let Some(text) = paste { self.paste(Some(text)); }

        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        // Most specific shortcuts first: consume_key ignores extra Shift.
        let (save_as, save, open, new, redo, undo, zoom_in, zoom_out, zoom_reset) = ctx.input_mut(|i| (
            i.consume_key(cmd_shift, Key::S),
            i.consume_key(cmd, Key::S),
            i.consume_key(cmd, Key::O),
            i.consume_key(cmd, Key::N),
            i.consume_key(cmd_shift, Key::Z) || i.consume_key(cmd, Key::Y),
            i.consume_key(cmd, Key::Z),
            i.consume_key(cmd, Key::Plus) || i.consume_key(cmd, Key::Equals),
            i.consume_key(cmd, Key::Minus),
            i.consume_key(cmd, Key::Num0),
        ));

        if !self.drawing {
            if undo && self.history.undo(&mut self.doc) { self.show_status("Undo"); }
            if redo && self.history.redo(&mut self.doc) { self.show_status("Redo"); }
        }
        if new { self.request(PendingAction::New, ctx); }
        if open { self.request(PendingAction::Open, ctx); }
        if save_as { self.save_as(); } else if save { self.save(); }
        if zoom_in { self.canvas_state.zoom_view_center(1.25); }
        if zoom_out { self.canvas_state.zoom_view_center(1.0 / 1.25); }
        if zoom_reset { self.canvas_state.zoom = 1.0; }

        let tool_keys = [
            (Key::B, ToolType::Brush),
            (Key::E, ToolType::Eraser),
            (Key::F, ToolType::Fill),
            (Key::R, ToolType::Rectangle),
            (Key::O, ToolType::Ellipse),
            (Key::L, ToolType::Line),
            (Key::D, ToolType::Border),
            (Key::S, ToolType::Select),
        ];
        let (tool, bigger, smaller, swap, delete, deselect) = ctx.input(|i| {
            if i.modifiers.command || i.modifiers.alt {
                return (None, false, false, false, false, false);
            }
            (
                tool_keys.iter().find(|(key, _)| i.key_pressed(*key)).map(|&(_, t)| t),
                i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
                i.key_pressed(Key::Minus),
                i.key_pressed(Key::X),
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::Escape),
            )
        });

        if let Some(tool) = tool { self.active_tool = tool; }
        if bigger { self.tool_options.brush_size = (self.tool_options.brush_size + 1).min(20); }
        if smaller { self.tool_options.brush_size = self.tool_options.brush_size.saturating_sub(1).max(1); }
        if swap {
            std::mem::swap(&mut self.primary_char, &mut self.secondary_char);
            std::mem::swap(&mut self.color_panel.primary_fg, &mut self.color_panel.secondary_fg);
            std::mem::swap(&mut self.color_panel.primary_bg, &mut self.color_panel.secondary_bg);
        }
        if delete && !self.drawing { self.delete_selection(); }
        if deselect && !self.drawing { self.tools.selection.selection = None; }
    }

    // ── Clipboard ────────────────────────────────────────────────────────────

    /// Copies the selected part of the active layer, also as plain text to the system clipboard.
    fn copy(&mut self, ctx: &egui::Context) -> bool {
        let Some(content) = self.tools.selection.copy(&self.doc) else {
            self.show_status("Nothing selected");
            return false;
        };
        self.clipboard_text = content_to_text(&content);
        ctx.copy_text(self.clipboard_text.clone());
        self.clipboard = Some(content);
        self.show_status("Copied selection");
        true
    }

    fn cut(&mut self, ctx: &egui::Context) {
        if self.tools.selection.selection.is_none() {
            self.show_status("Nothing selected");
            return;
        }
        if self.active_layer_locked() { return; }
        if self.copy(ctx) {
            let changes = self.tools.selection.delete(&mut self.doc);
            self.commit_draw(changes);
            self.show_status("Cut selection");
        }
    }

    /// Pastes our own clipboard, or `text` from the system clipboard when it holds something else.
    fn paste(&mut self, text: Option<String>) {
        if self.active_layer_locked() { return; }
        let content = match (&self.clipboard, text) {
            (Some(own), Some(text)) if text.trim_end() == self.clipboard_text.trim_end() => own.clone(),
            (_, Some(text)) => text_to_content(&text, &self.color_panel.primary_fg, &self.color_panel.primary_bg),
            (Some(own), None) => own.clone(),
            (None, None) => {
                self.show_status("Clipboard is empty");
                return;
            }
        };
        if content.first().is_none_or(|row| row.is_empty()) {
            self.show_status("Clipboard is empty");
            return;
        }

        let (w, h) = (self.doc.width, self.doc.height);
        let (x, y) = self.canvas_state.cursor_cell
            .filter(|&(x, y)| x < w && y < h)
            .or(self.tools.selection.selection.map(|(x1, y1, _, _)| (x1, y1)))
            .unwrap_or((0, 0));
        let changes = self.tools.selection.paste(&mut self.doc, &content, x, y);
        self.commit_draw(changes);
        self.show_status("Pasted");
    }

    fn delete_selection(&mut self) {
        if self.tools.selection.selection.is_none() || self.active_layer_locked() { return; }
        let changes = self.tools.selection.delete(&mut self.doc);
        self.commit_draw(changes);
    }

    // ── Layers & canvas (all undoable) ───────────────────────────────────────

    fn apply_layer_action(&mut self, action: LayerAction) {
        let n = self.doc.layers.len();
        let active = self.doc.active_layer;

        let cmd = match action {
            LayerAction::Select(i) => {
                if i < n { self.doc.active_layer = i; }
                return;
            }
            LayerAction::ToggleVisible(i) => {
                if let Some(layer) = self.doc.layers.get_mut(i) {
                    layer.visible = !layer.visible;
                    self.doc.modified = true;
                }
                return;
            }
            LayerAction::ToggleLock(i) => {
                if let Some(layer) = self.doc.layers.get_mut(i) {
                    layer.locked = !layer.locked;
                    self.doc.modified = true;
                }
                return;
            }
            LayerAction::Add => Command::LayerAdd {
                index: active + 1,
                layer: Layer::new(self.doc.width, self.doc.height, self.doc.next_layer_name()),
            },
            LayerAction::Duplicate => {
                let mut layer = self.doc.active_layer().clone();
                layer.name = format!("{} (copy)", layer.name);
                Command::LayerAdd { index: active + 1, layer }
            }
            LayerAction::Remove => {
                if n <= 1 { return; }
                Command::LayerRemove { index: active, layer: self.doc.active_layer().clone() }
            }
            LayerAction::MergeDown => {
                if active == 0 { return; }
                if self.doc.layers[active].locked || self.doc.layers[active - 1].locked {
                    self.show_status("Can't merge a locked layer");
                    return;
                }
                Command::LayerMergeDown {
                    upper:   active,
                    layer:   self.doc.active_layer().clone(),
                    changes: self.doc.merge_down_changes(active),
                }
            }
            LayerAction::MoveUp => {
                if active + 1 >= n { return; }
                Command::LayerMove { from: active, to: active + 1 }
            }
            LayerAction::MoveDown => {
                if active == 0 { return; }
                Command::LayerMove { from: active, to: active - 1 }
            }
            LayerAction::Reorder(from, to) => {
                if from == to || from >= n || to >= n { return; }
                Command::LayerMove { from, to }
            }
            LayerAction::Rename(i, name) => {
                let name = name.trim().to_string();
                if i >= n || name.is_empty() || name == self.doc.layers[i].name { return; }
                Command::LayerRename { index: i, old: self.doc.layers[i].name.clone(), new: name }
            }
        };
        self.history.execute(cmd, &mut self.doc);
    }

    fn clear_layer(&mut self) {
        if self.active_layer_locked() { return; }
        let changes = self.doc.active_layer().clear_changes();
        if changes.is_empty() { return; }
        let layer_idx = self.doc.active_layer;
        self.history.execute(Command::Draw { layer_idx, changes }, &mut self.doc);
    }

    fn resize_canvas(&mut self, width: usize, height: usize) {
        if (width, height) == (self.doc.width, self.doc.height) { return; }
        let cmd = Command::Resize {
            old_size:   (self.doc.width, self.doc.height),
            new_size:   (width, height),
            old_layers: self.doc.layers.clone(),
        };
        self.history.execute(cmd, &mut self.doc);
        self.tools.selection.clear();
    }

    fn show_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_timer = 3.0;
    }

    // ── Unsaved changes ──────────────────────────────────────────────────────

    /// Runs `action` right away, or asks first if there are unsaved changes.
    fn request(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.doc.modified {
            self.pending_action = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    fn perform(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::New => {
                self.new_width  = self.doc.width;
                self.new_height = self.doc.height;
                self.show_new_dialog = true;
            }
            PendingAction::Open => self.open(),
            PendingAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    /// Intercepts closing the window while there are unsaved changes.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.doc.modified {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending_action = Some(PendingAction::Quit);
        }
    }

    fn unsaved_changes_modal(&mut self, ctx: &egui::Context) {
        enum Choice { Save, Discard, Cancel }

        let Some(action) = self.pending_action else { return };
        let what = match action {
            PendingAction::New  => "creating a new document",
            PendingAction::Open => "opening another file",
            PendingAction::Quit => "quitting",
        };
        let name = self.doc_name();

        let mut choice = None;
        let modal = Modal::new(Id::new("unsaved_changes")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Unsaved changes");
            ui.add_space(6.0);
            ui.label(format!("Save changes to \"{name}\" before {what}?"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked()       { choice = Some(Choice::Save); }
                if ui.button("Don't Save").clicked() { choice = Some(Choice::Discard); }
                if ui.button("Cancel").clicked()     { choice = Some(Choice::Cancel); }
            });
        });
        if choice.is_none() && modal.should_close() {
            choice = Some(Choice::Cancel);
        }

        match choice {
            Some(Choice::Save) => {
                self.pending_action = None;
                if self.save() { self.perform(action, ctx); }
            }
            Some(Choice::Discard) => {
                self.pending_action = None;
                self.perform(action, ctx);
            }
            Some(Choice::Cancel) => self.pending_action = None,
            None => {}
        }
    }

    fn doc_name(&self) -> String {
        self.doc.file_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.doc.modified { "*" } else { "" };
        let title = format!("{}{dirty} — {APP_NAME}", self.doc_name());
        if title != self.window_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    // ── Dialogs ──────────────────────────────────────────────────────────────

    fn new_doc_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_new_dialog;
        egui::Window::new("New Document")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(DragValue::new(&mut self.new_width).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(DragValue::new(&mut self.new_height).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    for (label, w, h) in [("40×25",40,25),("80×25",80,25),("80×40",80,40),("120×50",120,50)] {
                        if ui.button(label).clicked() { self.new_width = w; self.new_height = h; }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        self.doc = Document::new(self.new_width, self.new_height);
                        self.history.clear();
                        self.tools.selection.clear();
                        self.show_new_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new_dialog = false;
                    }
                });
            });
        self.show_new_dialog &= open;
    }

    fn resize_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_resize_dialog;
        egui::Window::new("Resize Canvas")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Current: {}×{}", self.doc.width, self.doc.height));
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(DragValue::new(&mut self.new_width).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(DragValue::new(&mut self.new_height).range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    if ui.button("Resize").clicked() {
                        self.resize_canvas(self.new_width, self.new_height);
                        self.show_resize_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_resize_dialog = false;
                    }
                });
            });
        self.show_resize_dialog &= open;
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about_dialog;
        egui::Window::new("About Void Canvas")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .min_width(440.0)
            .show(ctx, |ui| {
                // ── Tab bar ──────────────────────────────────────────────────
                ui.horizontal(|ui| {
                    for (idx, label) in ["About & Shortcuts", "Author"].iter().enumerate() {
                        let active = self.about_tab == idx;
                        let btn = egui::Button::new(
                            egui::RichText::new(*label).strong()
                        )
                        .fill(if active {
                            egui::Color32::from_rgb(0, 90, 180)
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        });
                        if ui.add(btn).clicked() {
                            self.about_tab = idx;
                        }
                    }
                });
                ui.separator();

                match self.about_tab {
                    // ── Tab 0: About & Shortcuts ─────────────────────────────
                    0 => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("🎨 Void Canvas")
                                    .size(28.0)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!("v{VERSION}"))
                                    .size(14.0)
                                    .color(egui::Color32::from_gray(160)),
                            );
                            ui.add_space(4.0);
                            ui.label("ASCII art editor with layers, history & rich tools.");
                            ui.add_space(12.0);
                        });

                        ui.separator();
                        ui.add_space(6.0);

                        egui::Grid::new("shortcuts_grid")
                            .num_columns(2)
                            .spacing([24.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                let header = |ui: &mut egui::Ui, text: &str| {
                                    ui.label(egui::RichText::new(text).strong().color(egui::Color32::from_rgb(100, 180, 255)));
                                };

                                header(ui, "── File ──");
                                ui.end_row();
                                ui.label("Ctrl+N"); ui.label("New document"); ui.end_row();
                                ui.label("Ctrl+O"); ui.label("Open…"); ui.end_row();
                                ui.label("Ctrl+S"); ui.label("Save"); ui.end_row();
                                ui.label("Ctrl+Shift+S"); ui.label("Save as…"); ui.end_row();

                                ui.add_space(4.0); ui.end_row();
                                header(ui, "── Edit ──");
                                ui.end_row();
                                ui.label("Ctrl+Z"); ui.label("Undo"); ui.end_row();
                                ui.label("Ctrl+Y / Ctrl+Shift+Z"); ui.label("Redo"); ui.end_row();
                                ui.label("Ctrl+C"); ui.label("Copy selection"); ui.end_row();
                                ui.label("Ctrl+X"); ui.label("Cut selection"); ui.end_row();
                                ui.label("Ctrl+V"); ui.label("Paste at cursor"); ui.end_row();
                                ui.label("Delete");  ui.label("Delete selection"); ui.end_row();
                                ui.label("Escape");  ui.label("Deselect"); ui.end_row();
                                ui.label("X");       ui.label("Swap primary / secondary"); ui.end_row();

                                ui.add_space(4.0); ui.end_row();
                                header(ui, "── Tools ──");
                                ui.end_row();
                                ui.label("B"); ui.label("Brush"); ui.end_row();
                                ui.label("E"); ui.label("Eraser"); ui.end_row();
                                ui.label("F"); ui.label("Fill"); ui.end_row();
                                ui.label("R"); ui.label("Rectangle"); ui.end_row();
                                ui.label("O"); ui.label("Ellipse"); ui.end_row();
                                ui.label("L"); ui.label("Line"); ui.end_row();
                                ui.label("D"); ui.label("Border box"); ui.end_row();
                                ui.label("S"); ui.label("Select"); ui.end_row();

                                ui.add_space(4.0); ui.end_row();
                                header(ui, "── Canvas ──");
                                ui.end_row();
                                ui.label("Scroll");      ui.label("Pan canvas"); ui.end_row();
                                ui.label("Ctrl+Scroll"); ui.label("Zoom in / out"); ui.end_row();
                                ui.label("Ctrl+ +/–/0"); ui.label("Zoom in / out / reset"); ui.end_row();
                                ui.label("Middle drag"); ui.label("Pan canvas"); ui.end_row();
                                ui.label("+  /  –");      ui.label("Brush size up / down"); ui.end_row();
                            });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("Built with Rust · egui · eframe")
                                    .size(11.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                        });
                    }

                    // ── Tab 1: Author ────────────────────────────────────────
                    _ => {
                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("✏ Author")
                                    .size(22.0)
                                    .strong(),
                            );
                        });
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // ── Fill in your own details below ──────────────────
                        egui::Grid::new("author_grid")
                            .num_columns(2)
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Name").strong());
                                ui.label("Fantomm");
                                ui.end_row();

                                ui.label(egui::RichText::new("GitHub").strong());
                                ui.label("https://github.com/fantommripper");
                                ui.end_row();

                                ui.label(egui::RichText::new("License").strong());
                                ui.label("Modified MIT");
                                ui.end_row();
                            });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Did you actually go to that tab? Okay, thanks for that! From here on out, I'll switch to a language I'm more comfortable using, but trust me, there's nothing important there.")
                                .size(11.0)
                                .color(egui::Color32::from_gray(100))
                                .italics(),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Чтож, если вы нарисуете хоть один рисунок в этой программе, вы меня уже очень обрадуете! Честно говоря, это, наверное, самое интересное, что я смог сделать. Я благодарен всем разработчикам инструментов, которые помогли мне в этом. Хотя я чувствую себя паршиво за использование ИИ в этом проекте. Да, конечно, вою архитектуру придумал я, выбрал инструменты я... но код всё же я своим не считаю. Я вносил правки, но это только правки. Мне не нравится, что люди думают, будто я \"отличный программист\", предполагают, что я \"мидл\", и так далее... Да, я что-то знаю и понимаю, что пишет ИИ и как этот код работает, но всё же лично я не считаю себя программистом. Если вы дочитали до сюда, я ещё раз поблагодарю за всё! А сейчас я пойду и выпью таблетки, пока я снова не придумал очередной никому не нужный проект)")
                                .size(11.0)
                                .color(egui::Color32::from_gray(100))
                                .italics(),
                        );
                    }
                }
            });
        self.show_about_dialog = open;
    }

    // ── Export ───────────────────────────────────────────────────────────────

    fn export_text(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .save_file()
        {
            let mut path = path;
            if path.extension().map_or(true, |ext| ext != "txt") {
                path.set_extension("txt");
            }
            let text = self.doc.to_text();
            if std::fs::write(&path, text).is_ok() {
                self.show_status(format!("Exported to {:?}", path));
            } else {
                self.show_status("Export failed!");
            }
        }
    }

    fn export_png(&mut self) {
        use ab_glyph::{Font, FontRef, PxScale};
        use image::{ImageBuffer, Rgba};

        let cell_w: u32 = 10;
        let cell_h: u32 = 18;
        let img_w = self.doc.width  as u32 * cell_w;
        let img_h = self.doc.height as u32 * cell_h;

        let font_bytes = include_bytes!("assets/FiraCode.ttf");
        let font = match FontRef::try_from_slice(font_bytes) {
            Ok(f) => f,
            Err(e) => {
                self.show_status(format!("PNG export: font error – {e}"));
                return;
            }
        };
        let scale = PxScale::from(cell_h as f32 * 0.82);

        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(img_w, img_h, Rgba([0, 0, 0, 255]));

        for cy in 0..self.doc.height {
            for cx in 0..self.doc.width {
                let px_base = cx as u32 * cell_w;
                let py_base = cy as u32 * cell_h;

                let cell = self.doc.composite_cell(cx, cy);

                if !cell.bg.is_transparent() {
                    let [r, g, b, a] = [cell.bg.r, cell.bg.g, cell.bg.b, cell.bg.a];
                    for dy in 0..cell_h {
                        for dx in 0..cell_w {
                            img.put_pixel(px_base + dx, py_base + dy, Rgba([r, g, b, a]));
                        }
                    }
                }

                if cell.ch != ' ' {
                    let glyph_id = font.glyph_id(cell.ch);
                    let glyph = glyph_id.with_scale_and_position(
                        scale,
                        ab_glyph::point(0.0, {
                            use ab_glyph::ScaleFont;
                            font.as_scaled(scale).ascent()
                        }),
                    );

                    if let Some(outlined) = font.outline_glyph(glyph) {
                        let bounds = outlined.px_bounds();
                        let g_w = bounds.width()  as u32;
                        let x_off = ((cell_w as i32) - (g_w as i32)) / 2 + bounds.min.x as i32;
                        let y_off = bounds.min.y as i32;

                        let [fr, fg_c, fb, _] = [cell.fg.r, cell.fg.g, cell.fg.b, cell.fg.a];

                        outlined.draw(|gx, gy, cov| {
                            if cov < 0.01 { return; }
                            let sx = px_base as i32 + gx as i32 + x_off;
                            let sy = py_base as i32 + gy as i32 + y_off;
                            if sx < 0 || sy < 0 { return; }
                            let (sx, sy) = (sx as u32, sy as u32);
                            if sx >= img_w || sy >= img_h { return; }

                            let bg_px = *img.get_pixel(sx, sy);
                            let blend = |bg: u8, fg: u8| -> u8 {
                                ((bg as f32) * (1.0 - cov) + (fg as f32) * cov) as u8
                            };
                            img.put_pixel(sx, sy, Rgba([
                                blend(bg_px[0], fr),
                                blend(bg_px[1], fg_c),
                                blend(bg_px[2], fb),
                                255,
                            ]));
                        });
                    }
                }
            }
        }

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG Image", &["png"])
            .save_file()
        {
            let mut path = path;
            if path.extension().map_or(true, |ext| ext != "png") {
                path.set_extension("png");
            }
            match img.save(&path) {
                Ok(_) => self.show_status(format!("Exported PNG: {}", path.display())),
                Err(e) => self.show_status(format!("PNG export failed: {e}")),
            }
        }
    }

    // ── File I/O ─────────────────────────────────────────────────────────────

    /// Saves to the current file (asks for a path the first time). Returns `true` on success.
    fn save(&mut self) -> bool {
        match self.doc.file_path.clone() {
            Some(path) => self.write_file(path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let file_name = format!(
            "{}.vc",
            std::path::Path::new(&self.doc_name()).file_stem().unwrap_or_default().to_string_lossy()
        );
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("Void Canvas", &["vc"])
            .set_file_name(file_name)
            .save_file()
        else {
            return false;
        };
        if path.extension().is_none_or(|ext| ext != "vc") {
            path.set_extension("vc");
        }
        self.write_file(path.to_string_lossy().to_string())
    }

    /// Writes next to the target first and then renames, so a failed write can't wipe the old file.
    fn write_file(&mut self, path: String) -> bool {
        let data = match serde_json::to_string_pretty(&self.doc) {
            Ok(data) => data,
            Err(e) => {
                self.show_status(format!("Save failed: {e}"));
                return false;
            }
        };
        let tmp = format!("{path}.tmp");
        let result = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &path));
        match result {
            Ok(()) => {
                self.doc.file_path = Some(path);
                self.doc.modified = false;
                self.show_status(format!("Saved: {}", self.doc_name()));
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.show_status(format!("Save failed: {e}"));
                false
            }
        }
    }

    fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Void Canvas", &["vc"])
            .pick_file()
        else {
            return;
        };
        let data = match std::fs::read_to_string(&path) {
            Ok(data) => data,
            Err(e) => {
                self.show_status(format!("Failed to read file: {e}"));
                return;
            }
        };
        let mut doc = match serde_json::from_str::<Document>(&data) {
            Ok(doc) => doc,
            Err(e) => {
                self.show_status(format!("Failed to parse file: {e}"));
                return;
            }
        };
        if let Err(e) = doc.validate() {
            self.show_status(format!("Can't open file: {e}"));
            return;
        }
        doc.file_path = Some(path.to_string_lossy().to_string());
        self.doc = doc;
        self.history.clear();
        self.tools.selection.clear();
        self.show_status(format!("Opened: {}", self.doc_name()));
    }
}

// ── eframe::App ──────────────────────────────────────────────────────────────

impl eframe::App for AsciiArtEditor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dt = ctx.input(|i| i.unstable_dt);
        self.canvas_state.march_phase = (self.canvas_state.march_phase + dt * 0.5) % 1.0;
        if self.tools.selection.selection.is_some() {
            ctx.request_repaint();
        }

        if self.status_timer > 0.0 {
            self.status_timer -= dt;
            ctx.request_repaint_after_secs(self.status_timer.max(0.0));
        }

        self.handle_close_request(&ctx);
        self.handle_keyboard(&ctx);
        self.new_doc_dialog(&ctx);
        self.resize_dialog(&ctx);
        self.about_dialog(&ctx);
        self.unsaved_changes_modal(&ctx);

        // ── Menu bar ─────────────────────────────────────────────────────────
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.add(Button::new("New…").shortcut_text("Ctrl+N")).clicked() {
                        self.request(PendingAction::New, &ctx);
                    }
                    if ui.add(Button::new("Open…").shortcut_text("Ctrl+O")).clicked() {
                        self.request(PendingAction::Open, &ctx);
                    }
                    if ui.add(Button::new("Save").shortcut_text("Ctrl+S")).clicked() { self.save(); }
                    if ui.add(Button::new("Save As…").shortcut_text("Ctrl+Shift+S")).clicked() { self.save_as(); }
                    ui.separator();
                    if ui.button("Export Text…").clicked() { self.export_text(); }
                    if ui.button("Export PNG…").clicked()  { self.export_png(); }
                    ui.separator();
                    if ui.button("Quit").clicked() { self.request(PendingAction::Quit, &ctx); }
                });

                ui.menu_button("Edit", |ui| {
                    let undo = Button::new("Undo").shortcut_text("Ctrl+Z");
                    if ui.add_enabled(self.history.can_undo(), undo).clicked() {
                        self.history.undo(&mut self.doc);
                    }
                    let redo = Button::new("Redo").shortcut_text("Ctrl+Y");
                    if ui.add_enabled(self.history.can_redo(), redo).clicked() {
                        self.history.redo(&mut self.doc);
                    }
                    ui.separator();
                    let has_selection = self.tools.selection.selection.is_some();
                    if ui.add_enabled(has_selection, Button::new("Copy").shortcut_text("Ctrl+C")).clicked() {
                        self.copy(&ctx);
                    }
                    if ui.add_enabled(has_selection, Button::new("Cut").shortcut_text("Ctrl+X")).clicked() {
                        self.cut(&ctx);
                    }
                    if ui.add_enabled(self.clipboard.is_some(), Button::new("Paste").shortcut_text("Ctrl+V")).clicked() {
                        self.paste(None);
                    }
                    if ui.add_enabled(has_selection, Button::new("Delete").shortcut_text("Del")).clicked() {
                        self.delete_selection();
                    }
                    ui.separator();
                    if ui.button("Clear Layer").clicked() { self.clear_layer(); }
                });

                ui.menu_button("Canvas", |ui| {
                    if ui.button("Resize…").clicked() {
                        self.new_width  = self.doc.width;
                        self.new_height = self.doc.height;
                        self.show_resize_dialog = true;
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.add(Button::new("Zoom In").shortcut_text("Ctrl++")).clicked() {
                        self.canvas_state.zoom_view_center(1.25);
                    }
                    if ui.add(Button::new("Zoom Out").shortcut_text("Ctrl+-")).clicked() {
                        self.canvas_state.zoom_view_center(1.0 / 1.25);
                    }
                    if ui.add(Button::new("Reset Zoom").shortcut_text("Ctrl+0")).clicked() {
                        self.canvas_state.zoom = 1.0;
                    }
                    ui.separator();
                    ui.checkbox(&mut self.canvas_state.show_grid, "Show Grid");
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.about_tab = 0;
                        self.show_about_dialog = true;
                    }
                    if ui.button("Author").clicked() {
                        self.about_tab = 1;
                        self.show_about_dialog = true;
                    }
                });
            });
        });

        // ── Status bar ───────────────────────────────────────────────────────
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some((x, y)) = self.canvas_state.cursor_cell {
                    ui.label(format!("({x}, {y})"));
                    ui.separator();
                }
                ui.label(format!("{}×{}", self.doc.width, self.doc.height));
                ui.separator();
                ui.label(format!("{:.0}%", self.canvas_state.zoom * 100.0));
                ui.separator();
                ui.label(self.active_tool.name());
                ui.separator();
                let layer = self.doc.active_layer();
                ui.label(format!("Layer: {}", layer.name));
                if layer.locked   { ui.label("🔒"); }
                if !layer.visible { ui.label("👁‍🗨"); }

                if self.status_timer > 0.0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(&self.status_msg);
                    });
                }

                if self.doc.modified {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("●").color(Color32::YELLOW));
                    });
                }
            });
        });

        // ── Left toolbar ─────────────────────────────────────────────────────
        egui::Panel::left("toolbar")
            .resizable(false)
            .exact_size(44.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    for &tool in ToolType::all() {
                        let is_active = self.active_tool == tool;
                        let btn = egui::Button::new(
                            RichText::new(tool.label()).strong().size(24.0)
                        )
                        .min_size(Vec2::splat(36.0))
                        .fill(if is_active {
                            Color32::from_rgb(0, 120, 212)
                        } else {
                            Color32::from_gray(60)
                        });

                        if ui.add(btn)
                            .on_hover_text(format!("{} ({})", tool.name(), tool.label()))
                            .clicked()
                        {
                            self.active_tool = tool;
                        }
                        ui.add_space(2.0);
                    }

                    ui.separator();

                    let grid_icon = if self.canvas_state.show_grid { "#" } else { "·" };
                    let grid_btn = egui::Button::new(
                        RichText::new(grid_icon).strong().size(24.0)
                    )
                    .min_size(Vec2::splat(36.0))
                    .fill(if self.canvas_state.show_grid {
                        Color32::from_gray(80)
                    } else {
                        Color32::from_gray(50)
                    });

                    if ui.add(grid_btn).on_hover_text("Toggle Grid").clicked() {
                        self.canvas_state.show_grid = !self.canvas_state.show_grid;
                    }
                });
            });

        // ── Right: layers panel ───────────────────────────────────────────────
        egui::Panel::right("layers")
            .resizable(true)
            .default_size(220.0)
            .size_range(160.0..=400.0)
            .show(ui, |ui| {
                ui.heading("Layers");
                ui.separator();

                if let Some(action) = self.layer_panel.show(ui, &self.doc) {
                    self.apply_layer_action(action);
                    ctx.request_repaint();
                }
            });

        // ── Left: inspector panel ─────────────────────────────────────────────
        egui::Panel::left("left_panel")
            .resizable(true)
            .default_size(280.0)
            .size_range(200.0..=480.0)
            .show(ui, |ui| {
                ui.heading("Inspector");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("🔤 Characters")
                        .default_open(true)
                        .show(ui, |ui| {
                            self.char_palette.show(ui, &mut self.primary_char, &mut self.secondary_char);
                        });
                    egui::CollapsingHeader::new("🎨 Colors")
                        .default_open(true)
                        .show(ui, |ui| {
                            self.color_panel.show(ui);
                        });
                    egui::CollapsingHeader::new("⚙ Tool Options")
                        .default_open(false)
                        .show(ui, |ui| {
                            self.tool_options.show(ui);
                        });
                });
            });

        // ── Central canvas ───────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_gray(28)))
            .show(ui, |ui| {
                let ctx_tool = self.draw_context(true);
                let events = draw_canvas(
                    ui,
                    &self.doc,
                    &mut self.canvas_state,
                    &ctx_tool,
                    self.active_tool,
                    &mut self.tools,
                );
                for event in events {
                    self.handle_canvas_event(event);
                }
            });

        self.update_title(&ctx);

        #[cfg(debug_assertions)]
        crate::dev::screenshot_hook(&ctx);
    }
}
