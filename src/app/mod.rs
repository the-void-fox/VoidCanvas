use eframe::egui::{self, *};

use crate::document::{Cell, Changes, Document, Layer};
use crate::glyphs::{Transform, transform_block};
use crate::history::{Command, History};
use crate::theme;
use crate::tools::{
    DrawContext, ToolType, merge_changes,
    brush::BrushTool,
    eraser::EraserTool,
    fill::FillTool,
    shapes::{Shape, ShapeTool},
    selection::{SelectionTool, content_to_text, text_to_content},
    text::TextTool,
    pixel::{PixelTool, halves},
    shade::{RAMPS, ShadeTool},
};
mod chrome;
mod dialogs;
mod file_io;
mod import;
pub(crate) mod storage;

use crate::ui::{
    canvas::{CanvasEvent, CanvasState, PreviewSettings, draw_canvas, shape_mods},
    char_palette::CharPalette,
    layer_panel::{LayerAction, LayerPanel},
    panels::{ColorPanel, ToolOptions},
};

const APP_NAME: &str = "Void Canvas";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Undo groups of arrow-key moves live apart from typing sessions' groups.
const NUDGE_GROUPS: u64 = 1 << 63;

#[derive(Default)]
pub struct ToolState {
    pub brush:     BrushTool,
    pub pixel:     PixelTool,
    pub shade:     ShadeTool,
    pub eraser:    EraserTool,
    pub fill:      FillTool,
    pub shape:     ShapeTool,
    pub selection: SelectionTool,
    pub text:      TextTool,
}


/// Which panel the top of the dock shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Glyphs,
    Colors,
    History,
}

/// An action that would throw away unsaved work; it waits for the user's answer.
#[derive(Clone, PartialEq)]
enum PendingAction {
    New,
    Open,
    OpenPath(std::path::PathBuf),
    Quit,
}

/// Seconds after an edit before unsaved work is written to the recovery file.
const AUTOSAVE_DELAY: f64 = 30.0;

pub struct AsciiArtEditor {
    pub doc:          Document,
    pub history:      History,
    pub active_tool:  ToolType,
    pub tools:        ToolState,
    pub canvas_state: CanvasState,
    pub char_palette: CharPalette,
    pub color_panel:  ColorPanel,
    pub layer_panel:  LayerPanel,
    pub tool_options: ToolOptions,
    pub dock_tab:     DockTab,
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
    /// Undo group of arrow-key moves; a new one starts with every click on the canvas.
    nudge_group: u64,
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
    settings: storage::Settings,
    /// Unsaved work from a run that crashed, waiting for "Recover?".
    pub(crate) recovery: Option<storage::Autosave>,
    /// The Import Image dialog, while open.
    pub(crate) image_import: Option<import::ImageImport>,
    /// History version written by the last autosave.
    autosaved_version: u64,
    last_autosave: f64,
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
            tool_options: ToolOptions::default(),
            dock_tab:     DockTab::Glyphs,
            primary_char:   '#',
            secondary_char: '.',
            clipboard:      None,
            clipboard_text: String::new(),
            drawing:      false,
            last_primary: true,
            stroke_tool:    ToolType::Brush,
            stroke_changes: Changes::new(),
            nudge_group:    NUDGE_GROUPS,
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
            settings:       storage::Settings::default(),
            recovery:       None,
            image_import:   None,
            autosaved_version: 0,
            last_autosave:  0.0,
        }
    }
}

impl AsciiArtEditor {
    /// Sets up the UI (fonts, theme, input options) and creates the editor.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        // Ctrl +/-/0 zoom the canvas, not the whole UI.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        Self {
            settings: storage::Settings::load(),
            recovery: storage::Autosave::load(),
            ..Self::default()
        }
    }

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
            channels:       self.tool_options.channels,
            mirror:         self.tool_options.mirror,
        }
    }

    /// Records changes that were already applied to the active layer.
    fn commit_draw(&mut self, changes: Changes, label: &str) {
        if changes.is_empty() { return; }
        self.history.push_as(Command::Draw { layer_idx: self.doc.active_layer, changes }, label);
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
            CanvasEvent::Press { x, y, primary, mods, lower } => {
                if self.drawing { return; }
                self.nudge_group += 1;
                let tool = self.active_tool;
                self.last_primary = primary;

                // Alt+click with a drawing tool samples like the eyedropper (and keeps sampling while dragged).
                if tool == ToolType::Pixel && mods.alt {
                    self.pick_pixel(x, y, lower, primary);
                    return;
                }
                if tool == ToolType::Eyedropper || (mods.alt && tool.draws()) {
                    self.pick(x, y, primary);
                    self.drawing = true;
                    self.stroke_tool = ToolType::Eyedropper;
                    return;
                }

                if tool == ToolType::Text {
                    self.tools.text.start(x, y, !primary);
                    return;
                }

                let edits = tool != ToolType::Select || self.tools.selection.hit(x, y);
                if edits && self.active_layer_locked() { return; }
                let ctx = self.draw_context(primary);

                if tool == ToolType::Fill {
                    let changes = self.tools.fill.flood_fill(&mut self.doc, x, y, &ctx);
                    self.commit_draw(changes, "Fill");
                    return;
                }

                self.drawing = true;
                self.stroke_tool = tool;
                self.stroke_changes.clear();

                match tool {
                    ToolType::Brush  => { self.tools.brush.on_press(&mut self.doc, x, y, &ctx); }
                    ToolType::Pixel  => self.tools.pixel.on_press(&mut self.doc, x, y * 2 + lower as usize, &ctx),
                    ToolType::Shade  => {
                        let ramp = self.shade_ramp();
                        self.tools.shade.on_press(&mut self.doc, x, y, &ctx, ramp);
                    }
                    ToolType::Eraser => { self.tools.eraser.on_press(&mut self.doc, x, y, &ctx); }
                    ToolType::Select => { self.tools.selection.on_press(&self.doc, x, y); }
                    t if t.is_shape() => self.tools.shape.on_press(x, y),
                    _ => {}
                }
            }

            CanvasEvent::Drag { x, y, lower } => {
                if !self.drawing { return; }
                let ctx = self.draw_context(self.last_primary);
                match self.stroke_tool {
                    ToolType::Brush  => { self.tools.brush.on_drag(&mut self.doc, x, y, &ctx); }
                    ToolType::Pixel  => self.tools.pixel.on_drag(&mut self.doc, x, y * 2 + lower as usize, &ctx),
                    ToolType::Shade  => {
                        let ramp = self.shade_ramp();
                        self.tools.shade.on_drag(&mut self.doc, x, y, &ctx, ramp);
                    }
                    ToolType::Eraser => { self.tools.eraser.on_drag(&mut self.doc, x, y, &ctx); }
                    ToolType::Eyedropper => self.pick(x, y, self.last_primary),
                    ToolType::Select => {
                        let changes = self.tools.selection.on_drag(&mut self.doc, x, y);
                        merge_changes(&mut self.stroke_changes, changes);
                    }
                    _ => {}
                }
            }

            CanvasEvent::Release { x, y, mods } => {
                if !self.drawing { return; }
                self.drawing = false;

                let ctx = self.draw_context(self.last_primary);
                let changes = match self.stroke_tool {
                    ToolType::Brush  => self.tools.brush.on_release(),
                    ToolType::Pixel  => self.tools.pixel.on_release(),
                    ToolType::Shade  => self.tools.shade.on_release(),
                    ToolType::Eraser => self.tools.eraser.on_release(),
                    ToolType::Select => self.tools.selection.on_release(&mut self.doc),
                    tool => match self.shape_for(tool) {
                        Some(shape) => self.tools.shape.on_release(&mut self.doc, shape, (x, y), shape_mods(mods), &ctx),
                        None => Changes::new(),
                    },
                };

                let mut all = std::mem::take(&mut self.stroke_changes);
                merge_changes(&mut all, changes);
                let label = if self.stroke_tool == ToolType::Select { "Move" } else { self.stroke_tool.name() };
                self.commit_draw(all, label);
            }
        }
    }

    fn shade_ramp(&self) -> &'static str {
        RAMPS[self.tool_options.shade_ramp.min(RAMPS.len() - 1)].1
    }

    /// The shape a shape tool draws with the current options.
    fn shape_for(&self, tool: ToolType) -> Option<Shape> {
        let options = &self.tool_options;
        Some(match tool {
            ToolType::Line if options.line_box => Shape::BoxLine(options.border_style),
            ToolType::Line      => Shape::Line,
            ToolType::Rectangle => Shape::Rect { filled: options.filled },
            ToolType::Ellipse   => Shape::Ellipse { filled: options.filled },
            ToolType::Border    => Shape::Border(options.border_style),
            _ => return None,
        })
    }

    /// Alt+click with the Pixel tool: picks the colour of the half-block pixel.
    fn pick_pixel(&mut self, x: usize, y: usize, lower: bool, primary: bool) {
        if x >= self.doc.width || y >= self.doc.height { return; }
        let (top, bottom) = halves(&self.doc.composite_cell(x, y));
        let color = if lower { bottom } else { top }.unwrap_or(crate::document::Color::TRANSPARENT);
        let slot = if primary { &mut self.color_panel.primary_fg } else { &mut self.color_panel.secondary_fg };
        *slot = color;
    }

    /// Eyedropper: copies the enabled channels of the visible cell into a slot.
    fn pick(&mut self, x: usize, y: usize, primary: bool) {
        if x >= self.doc.width || y >= self.doc.height { return; }
        let cell = self.doc.composite_cell(x, y);
        let channels = self.tool_options.channels;
        let panel = &mut self.color_panel;
        let (glyph, fg, bg) = if primary {
            (&mut self.primary_char, &mut panel.primary_fg, &mut panel.primary_bg)
        } else {
            (&mut self.secondary_char, &mut panel.secondary_fg, &mut panel.secondary_bg)
        };
        if channels.glyph { *glyph = cell.ch; }
        if channels.fg { *fg = cell.fg; }
        if channels.bg { *bg = cell.bg; }
    }

    /// The Text tool has a cursor on the canvas: keys go to the text.
    fn typing(&self) -> bool {
        self.active_tool == ToolType::Text && self.tools.text.cursor.is_some()
    }

    /// Text, pasted text and editing keys while typing; one typing session is one undo step.
    fn handle_typing(&mut self, ctx: &egui::Context) {
        enum Edit { Text(String), Newline, Backspace, Delete, Move(isize, isize), Stop }

        let edits: Vec<Edit> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    Event::Text(text) | Event::Paste(text) => Some(Edit::Text(text.clone())),
                    Event::Key { key, pressed: true, modifiers, .. } if !modifiers.command => match key {
                        Key::Enter      => Some(Edit::Newline),
                        Key::Backspace  => Some(Edit::Backspace),
                        Key::Delete     => Some(Edit::Delete),
                        Key::ArrowLeft  => Some(Edit::Move(-1, 0)),
                        Key::ArrowRight => Some(Edit::Move(1, 0)),
                        Key::ArrowUp    => Some(Edit::Move(0, -1)),
                        Key::ArrowDown  => Some(Edit::Move(0, 1)),
                        Key::Escape     => Some(Edit::Stop),
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        });
        if edits.is_empty() { return; }

        let writes = edits.iter().any(|e| matches!(e, Edit::Text(_) | Edit::Backspace | Edit::Delete));
        let locked = writes && self.active_layer_locked();
        let draw = self.draw_context(!self.tools.text.secondary);
        let text = &mut self.tools.text;
        let mut changes = Changes::new();
        for edit in edits {
            match edit {
                Edit::Text(t) if !locked => merge_changes(&mut changes, text.type_text(&mut self.doc, &t, &draw)),
                Edit::Backspace if !locked => merge_changes(&mut changes, text.backspace(&mut self.doc, &draw)),
                Edit::Delete if !locked => merge_changes(&mut changes, text.erase_at_cursor(&mut self.doc, &draw)),
                Edit::Newline => text.newline(&self.doc),
                Edit::Move(dx, dy) => text.move_by(dx, dy, &self.doc),
                Edit::Stop => text.stop(),
                _ => {}
            }
        }
        if !changes.is_empty() {
            self.history.push_grouped(self.doc.active_layer, changes, self.tools.text.group, "Typing");
            self.doc.modified = true;
        }
    }

    /// Swaps the primary and secondary glyph and colours.
    fn swap_slots(&mut self) {
        std::mem::swap(&mut self.primary_char, &mut self.secondary_char);
        self.color_panel.swap();
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Keys typed into a text field (char search, layer rename, …) belong to that field.
        if ctx.text_edit_focused()
            || self.pending_action.is_some()
            || self.recovery.is_some()
            || self.image_import.is_some()
        {
            return;
        }

        let typing = self.typing();
        if typing {
            self.handle_typing(ctx);
        }

        // Ctrl+C / Ctrl+X / Ctrl+V arrive as Copy / Cut / Paste events, not as key presses.
        let (copy, cut, paste) = if typing { (false, false, None) } else { ctx.input(|i| {
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
        }) };
        if copy { self.copy(ctx); }
        if cut { self.cut(ctx); }
        if let Some(text) = paste { self.paste(Some(text)); }

        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        // Most specific shortcuts first: consume_key ignores extra Shift.
        let (select_all, deselect_all) = ctx.input_mut(|i| (i.consume_key(cmd, Key::A), i.consume_key(cmd, Key::D)));
        if select_all && !self.drawing {
            self.tools.selection.clear();
            self.tools.selection.selection = Some((0, 0, self.doc.width - 1, self.doc.height - 1));
        }
        if deselect_all && !self.drawing { self.tools.selection.selection = None; }

        let (save_as, save, open, new, redo, undo, zoom_in, zoom_out, fit, actual) = ctx.input_mut(|i| (
            i.consume_key(cmd_shift, Key::S),
            i.consume_key(cmd, Key::S),
            i.consume_key(cmd, Key::O),
            i.consume_key(cmd, Key::N),
            i.consume_key(cmd_shift, Key::Z) || i.consume_key(cmd, Key::Y),
            i.consume_key(cmd, Key::Z),
            i.consume_key(cmd, Key::Plus) || i.consume_key(cmd, Key::Equals),
            i.consume_key(cmd, Key::Minus),
            i.consume_key(cmd, Key::Num0),
            i.consume_key(cmd, Key::Num1),
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
        if fit { self.canvas_state.request_fit(); }
        if actual { self.canvas_state.actual_size(); }

        // Letters, Space and Delete belong to the text being typed.
        if typing { return; }

        let tool_keys: Vec<(Key, ToolType)> = ToolType::groups()
            .iter()
            .flat_map(|group| group.iter())
            .filter_map(|&t| Some((Key::from_name(&t.key().to_string())?, t)))
            .collect();
        let selecting = self.active_tool == ToolType::Select && self.tools.selection.selection.is_some();
        let (nudge, flip) = ctx.input(|i| {
            if !selecting || i.modifiers.command || i.modifiers.alt {
                return ((0, 0), None);
            }
            let step = if i.modifiers.shift { 5 } else { 1 };
            let axis = |minus: Key, plus: Key| {
                (i.key_pressed(plus) as isize - i.key_pressed(minus) as isize) * step
            };
            let flip = if i.key_pressed(Key::H) {
                Some(Transform::FlipHorizontal)
            } else if i.key_pressed(Key::V) {
                Some(Transform::FlipVertical)
            } else {
                None
            };
            ((axis(Key::ArrowLeft, Key::ArrowRight), axis(Key::ArrowUp, Key::ArrowDown)), flip)
        });
        if nudge != (0, 0) && !self.drawing { self.nudge_selection(nudge.0, nudge.1); }
        if let Some(t) = flip { self.transform_selection(t); }

        let (tool, bigger, smaller, swap, delete, deselect) = ctx.input(|i| {
            if i.modifiers.command || i.modifiers.alt {
                return (None, false, false, false, false, false);
            }
            (
                tool_keys.iter().find(|(key, _)| i.key_pressed(*key)).map(|&(_, t)| t),
                i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) || i.key_pressed(Key::CloseBracket),
                i.key_pressed(Key::Minus) || i.key_pressed(Key::OpenBracket),
                i.key_pressed(Key::X),
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::Escape),
            )
        });

        if let Some(tool) = tool { self.active_tool = tool; }
        if bigger { self.tool_options.brush_size = (self.tool_options.brush_size + 1).min(20); }
        if smaller { self.tool_options.brush_size = self.tool_options.brush_size.saturating_sub(1).max(1); }
        if swap { self.swap_slots(); }
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
            self.commit_draw(changes, "Cut");
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
        self.commit_draw(changes, "Paste");
        self.show_status("Pasted");
    }

    /// Mirrors or rotates the selected block, turning glyphs to match.
    fn transform_selection(&mut self, t: Transform) {
        if self.tools.selection.selection.is_none() {
            self.show_status("Nothing selected");
            return;
        }
        if self.drawing || self.active_layer_locked() { return; }
        let Some(block) = self.tools.selection.copy(&self.doc) else { return };
        let changes = self.tools.selection.replace(&mut self.doc, &transform_block(&block, t));
        self.commit_draw(changes, t.label());
    }

    /// Arrow keys: moves the selected block; a run of moves is one undo step.
    fn nudge_selection(&mut self, dx: isize, dy: isize) {
        if self.active_layer_locked() { return; }
        let changes = self.tools.selection.nudge(&mut self.doc, dx, dy);
        if !changes.is_empty() {
            self.history.push_grouped(self.doc.active_layer, changes, self.nudge_group, "Move");
            self.doc.modified = true;
        }
    }

    fn delete_selection(&mut self) {
        if self.tools.selection.selection.is_none() || self.active_layer_locked() { return; }
        let changes = self.tools.selection.delete(&mut self.doc);
        self.commit_draw(changes, "Delete");
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
                let cmd = Command::LayerAdd { index: active + 1, layer };
                self.history.execute_as(cmd, "Duplicate Layer", &mut self.doc);
                return;
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
        self.history.execute_as(Command::Draw { layer_idx, changes }, "Clear Layer", &mut self.doc);
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

    /// Replaces the document (new / opened file): fresh history, no selection, canvas re-centred.
    fn load_document(&mut self, doc: Document) {
        self.doc = doc;
        self.history.clear();
        self.tools.selection.clear();
        self.canvas_state.request_fit();
        // The previous document's unsaved work was given up on purpose.
        storage::Autosave::remove();
        self.autosaved_version = self.history.version();
    }

    /// Writes unsaved work to the recovery file a little while after it changed.
    fn autosave(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if !self.doc.modified || self.history.version() == self.autosaved_version {
            self.last_autosave = now;
            return;
        }
        let due = self.last_autosave + AUTOSAVE_DELAY;
        if now >= due && !self.drawing {
            storage::Autosave::write(&self.doc);
            self.autosaved_version = self.history.version();
            self.last_autosave = now;
        } else {
            ctx.request_repaint_after_secs((due - now).max(0.1) as f32);
        }
    }

    /// Files dropped on the window open like File → Open.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            if import::is_image(&path) {
                self.start_image_import(&path);
            } else {
                self.request(PendingAction::OpenPath(path), ctx);
            }
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop_overlay")));
            let screen = ctx.content_rect();
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
            painter.rect_stroke(screen.shrink(16.0), 12.0, Stroke::new(2.0, theme::ACCENT), StrokeKind::Inside);
            painter.text(screen.center(), Align2::CENTER_CENTER, "Drop a .vc / .txt file to open, or a picture to import", theme::semibold(20.0), theme::TEXT);
        }
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
            PendingAction::OpenPath(path) => self.open_path(&path),
            PendingAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    /// Intercepts closing the window while there are unsaved changes.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) { return; }
        if !self.allow_close && self.doc.modified {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending_action = Some(PendingAction::Quit);
        } else {
            // A clean exit: nothing to recover next time.
            storage::Autosave::remove();
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
        self.handle_dropped_files(&ctx);
        self.autosave(&ctx);
        self.handle_keyboard(&ctx);
        self.new_doc_dialog(&ctx);
        self.resize_dialog(&ctx);
        self.about_dialog(&ctx);
        self.unsaved_changes_modal(&ctx);
        self.recovery_modal(&ctx);
        self.image_import_dialog(&ctx);

        if self.active_tool != ToolType::Text {
            self.tools.text.stop();
        }

        // Order matters: panels claim space from the outside in, the canvas gets the rest.
        self.menu_bar(ui);
        self.options_bar(ui);
        self.status_bar(ui);
        self.tool_strip(ui);
        self.dock(ui);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG_DEEP))
            .show(ui, |ui| {
                let ctx_tool = self.draw_context(true);
                let settings = PreviewSettings { shape: self.shape_for(self.active_tool), ramp: self.shade_ramp() };
                // Space + drag pans, like in Photoshop.
                let pan_mode = !self.drawing
                    && !self.typing()
                    && !ctx.text_edit_focused()
                    && ctx.input(|i| i.key_down(Key::Space));
                let events = draw_canvas(
                    ui,
                    &self.doc,
                    &mut self.canvas_state,
                    &ctx_tool,
                    self.active_tool,
                    settings,
                    pan_mode,
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
