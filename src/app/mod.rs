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
    gradient::{GradientSettings, GradientTool},
    shapes::{Shape, ShapeTool},
    selection::{SelectionTool, content_to_text, text_to_content},
    text::TextTool,
    pixel::{PixelGrid, PixelTool, halves, pixel_set},
    shade::ShadeTool,
};
mod animation;
mod background;
pub(crate) mod banner;
mod chrome;
pub(crate) mod commands;
mod dialogs;
pub(crate) mod file_dialog;
pub(crate) mod file_io;
mod import;
pub(crate) mod library;
pub(crate) mod replace;
mod scripting;
pub(crate) mod storage;
pub(crate) mod tabs;
mod three_d;

use crate::ui::{
    canvas::{CanvasEvent, CanvasState, PreviewSettings, draw_canvas, shape_mods},
    char_palette::{CharPalette, SetAction},
    layer_panel::{LayerAction, LayerPanel},
    panels::{ColorPanel, ToolOptions},
};
pub(crate) use file_dialog::FileAction;
use chrome::TRANSFORMS;
use commands::Action;
use storage::GlyphSet;

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
    pub gradient:  GradientTool,
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
pub(crate) enum PendingAction {
    /// Close the tab in front.
    CloseTab,
    /// Close every tab, asking about each unsaved one, then the window.
    Quit,
}

/// Undo steps kept per document.
const HISTORY_STEPS: usize = 100;

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
    pub(crate) allow_close: bool,
    window_title: String,
    pub(crate) settings: storage::Settings,
    /// Unsaved work from a run that crashed, waiting for "Recover?".
    pub(crate) recovery: Option<storage::Autosave>,
    /// The Import Image dialog, while open.
    pub(crate) image_import: Option<import::ImageImport>,
    /// The Replace window, while open.
    replace: Option<replace::ReplaceDialog>,
    /// The Insert Banner window, while open.
    pub(crate) banner: Option<banner::BannerDialog>,
    /// The built-in file chooser, while open.
    pub(crate) file_browser: Option<file_dialog::FileBrowser>,
    /// The command palette, while open.
    pub(crate) command_palette: Option<commands::CommandPalette>,
    /// One entry per tab; the one in front is `None`, its document lives in the fields above.
    tabs: Vec<Option<tabs::Tab>>,
    pub(crate) playback: animation::Playback,
    /// The name typed to save the current brush as.
    brush_save_name: String,
    /// A layer's blend mode and opacity before the style edit in progress.
    style_before: Option<(usize, crate::document::Blend, f32)>,
    /// 3D frames rendering in the background.
    pub(crate) render_job: Option<background::RenderJob>,
    /// The system file dialog, while it is open.
    pending_dialog: Option<background::PendingDialog>,
    /// A picture being read for Import Image.
    loading_picture: Option<background::LoadingPicture>,
    /// The Library window, while open.
    pub(crate) library: Option<library::LibraryWindow>,
    /// The 3D window, while open.
    pub(crate) three_d: Option<three_d::ThreeDWindow>,
    /// The Script window, while open.
    pub(crate) script: Option<scripting::ScriptWindow>,
    active_tab: usize,
    /// Folder of the last file chosen; the built-in chooser starts there.
    pub(crate) last_dir: Option<std::path::PathBuf>,
    /// The system file dialog didn't open once, so the built-in one is used from then on.
    /// (Always in tests: they must never open a real dialog.)
    pub(crate) system_dialog_failed: bool,
    /// History version written by the last autosave.
    autosaved_version: u64,
    last_autosave: f64,
}

impl Default for AsciiArtEditor {
    fn default() -> Self {
        Self {
            doc:          Document::new(80, 40),
            history:      History::new(HISTORY_STEPS),
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
            replace:        None,
            banner:         None,
            file_browser:   None,
            command_palette: None,
            tabs:           vec![None],
            playback:       animation::Playback::default(),
            script:         None,
            three_d:        None,
            library:        None,
            render_job:     None,
            pending_dialog: None,
            loading_picture: None,
            style_before:   None,
            brush_save_name: String::new(),
            active_tab:     0,
            last_dir:       None,
            system_dialog_failed: cfg!(test),
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
        self.history.push_as(Command::Draw { layer_idx: self.doc.active_layer, frame: self.doc.frame, changes }, label);
        self.doc.modified = true;
    }

    /// Shows a message and returns `true` if the active layer can't be edited.
    fn active_layer_locked(&mut self) -> bool {
        let layer = self.doc.active_layer();
        if layer.scene.is_some() {
            self.show_status("A 3D layer shows its scene: change it in the 3D window, or rasterize it to paint");
            return true;
        }
        let locked = layer.locked;
        if locked {
            self.show_status("Layer is locked");
        }
        locked
    }

    fn handle_canvas_event(&mut self, event: CanvasEvent) {
        match event {
            CanvasEvent::Press { x, y, primary, mods, sub } => {
                if self.drawing { return; }
                self.stop_playback();
                self.nudge_group += 1;
                let tool = self.active_tool;
                self.last_primary = primary;

                // Alt+click with a drawing tool samples like the eyedropper (and keeps sampling while dragged).
                if tool == ToolType::Pixel && mods.alt {
                    self.pick_pixel(x, y, sub, primary);
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
                if tool.draws() && tool != ToolType::Eraser {
                    self.remember_used(&ctx);
                }

                if tool == ToolType::Fill {
                    let changes = self.tools.fill.flood_fill(&mut self.doc, x, y, &ctx);
                    self.commit_draw(changes, "Fill");
                    return;
                }

                self.drawing = true;
                self.stroke_tool = tool;
                self.stroke_changes.clear();

                match tool {
                    ToolType::Brush  => {
                        self.tools.brush.on_press(&mut self.doc, x, y, &self.tool_options.brush, &ctx);
                        self.report_brush_error();
                    }
                    ToolType::Pixel  => {
                        let grid = self.tool_options.pixel_grid;
                        self.tools.pixel.on_press(&mut self.doc, grid.pixel_at(x, y, sub), grid, &ctx);
                    }
                    ToolType::Shade  => {
                        self.tools.shade.on_press(&mut self.doc, x, y, &ctx, &self.tool_options.ramp);
                    }
                    ToolType::Eraser => { self.tools.eraser.on_press(&mut self.doc, x, y, &ctx); }
                    ToolType::Select => { self.tools.selection.on_press(&self.doc, x, y); }
                    ToolType::Gradient => self.tools.gradient.on_press(x, y),
                    t if t.is_shape() => self.tools.shape.on_press(x, y),
                    _ => {}
                }
            }

            CanvasEvent::Drag { x, y, sub } => {
                if !self.drawing { return; }
                let ctx = self.draw_context(self.last_primary);
                match self.stroke_tool {
                    ToolType::Brush  => {
                        self.tools.brush.on_drag(&mut self.doc, x, y, &self.tool_options.brush, &ctx);
                        self.report_brush_error();
                    }
                    ToolType::Pixel  => {
                        let grid = self.tool_options.pixel_grid;
                        self.tools.pixel.on_drag(&mut self.doc, grid.pixel_at(x, y, sub), grid, &ctx);
                    }
                    ToolType::Shade  => {
                        self.tools.shade.on_drag(&mut self.doc, x, y, &ctx, &self.tool_options.ramp);
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
                    ToolType::Gradient => {
                        let settings = gradient_settings(&self.tool_options, self.tools.selection.selection, &self.doc);
                        self.tools.gradient.on_release(&mut self.doc, (x, y), &ctx, settings)
                    }
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

    /// Puts what a stroke paints with into the Recent rows of the dock.
    fn remember_used(&mut self, ctx: &DrawContext) {
        let channels = ctx.channels;
        if channels.glyph && !matches!(self.active_tool, ToolType::Pixel | ToolType::Shade | ToolType::Gradient) {
            self.char_palette.remember(ctx.current_char());
        }
        if channels.fg || self.active_tool == ToolType::Pixel {
            self.color_panel.remember(&ctx.current_fg());
        }
        if channels.bg && self.active_tool != ToolType::Pixel {
            self.color_panel.remember(&ctx.current_bg());
        }
    }

    /// Alt+click with the Pixel tool: picks the colour of the pixel under the pointer.
    fn pick_pixel(&mut self, x: usize, y: usize, sub: (f32, f32), primary: bool) {
        if x >= self.doc.width || y >= self.doc.height { return; }
        let cell = self.doc.composite_cell(x, y);
        let grid = self.tool_options.pixel_grid;
        let color = if grid == PixelGrid::Halves {
            let (top, bottom) = halves(&cell);
            if sub.1 >= 0.5 { bottom } else { top }.unwrap_or(crate::document::Color::TRANSPARENT)
        } else {
            let (px, py) = grid.pixel_at(0, 0, sub);
            if pixel_set(&cell, grid, (px, py)) { cell.fg } else { cell.bg }
        };
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
            self.history.push_grouped(self.doc.active_layer, self.doc.frame, changes, self.tools.text.group, "Typing");
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
            || self.file_browser.is_some()
            || self.command_palette.is_some()
            || self.waiting()
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
        let (palette, select_all, deselect_all, replace, brush) = ctx.input_mut(|i| (
            i.consume_key(cmd_shift, Key::P) || i.consume_key(Modifiers::NONE, Key::F1),
            i.consume_key(cmd, Key::A),
            i.consume_key(cmd, Key::D),
            i.consume_key(cmd, Key::H),
            i.consume_key(cmd, Key::B),
        ));
        if brush { self.run(Action::BrushFromSelection, ctx); }
        let (close_tab, prev_tab, next_tab) = ctx.input_mut(|i| (
            i.consume_key(cmd, Key::W),
            i.consume_key(cmd_shift, Key::Tab),
            i.consume_key(cmd, Key::Tab),
        ));
        if palette { self.run(Action::CommandPalette, ctx); }
        if replace { self.run(Action::Replace, ctx); }
        if select_all { self.run(Action::SelectAll, ctx); }
        if deselect_all { self.run(Action::Deselect, ctx); }

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

        if undo { self.run(Action::Undo, ctx); }
        if redo { self.run(Action::Redo, ctx); }
        if new { self.run(Action::New, ctx); }
        if open { self.run(Action::Open, ctx); }
        if close_tab { self.run(Action::CloseTab, ctx); }
        if prev_tab { self.run(Action::PrevTab, ctx); } else if next_tab { self.run(Action::NextTab, ctx); }
        if save_as { self.choose_file(FileAction::SaveAs { then: None }, ctx); } else if save { self.save(ctx); }
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

        let (prev_frame, next_frame, play) = ctx.input(|i| {
            if i.modifiers.any() { return (false, false, false); }
            (i.key_pressed(Key::Comma), i.key_pressed(Key::Period), i.key_pressed(Key::Enter))
        });
        if prev_frame { self.run(Action::PrevFrame, ctx); }
        if next_frame { self.run(Action::NextFrame, ctx); }
        if play { self.run(Action::Play, ctx); }
    }

    /// A script brush that failed says why.
    fn report_brush_error(&mut self) {
        if let Some(e) = self.tools.brush.error.take() {
            self.show_warning(&format!("Script brush: {e}"));
        }
    }

    /// Ctrl+B: the selected part of the active layer becomes the Brush.
    pub(crate) fn brush_from_selection(&mut self) {
        let Some(cells) = self.tools.selection.copy(&self.doc) else {
            self.show_status("Select something to make a brush of");
            return;
        };
        if cells.iter().flatten().all(Cell::is_empty) {
            self.show_status("The selection is empty on this layer");
            return;
        }
        let size = format!("{}×{}", cells[0].len(), cells.len());
        self.tool_options.brush = crate::tools::brush::BrushKind::Stamp { cells, recolor: false };
        self.tool_options.brush_name = format!("Stamp {size}");
        self.active_tool = ToolType::Brush;
        self.show_status(format!("The {size} selection is the brush now"));
    }

    // ── The user's glyph sets ────────────────────────────────────────────────

    pub(crate) fn apply_set_action(&mut self, action: SetAction, ctx: &egui::Context) {
        let builtin = crate::ui::char_palette::CATEGORIES.len();
        match action {
            SetAction::New => {
                let sets = &self.settings.glyph_sets;
                let name = (1..)
                    .map(|n| format!("My Glyphs {n}"))
                    .find(|name| sets.iter().all(|s| &s.name != name))
                    .unwrap();
                self.settings.edit_glyph_sets(|sets| sets.push(GlyphSet { name, glyphs: String::new() }));
                self.char_palette.category = builtin + self.settings.glyph_sets.len() - 1;
                self.char_palette.editing = false;
            }
            SetAction::Rename(i, name) => {
                let name = name.trim().to_owned();
                if !name.is_empty() {
                    self.settings.edit_glyph_sets(|sets| sets[i].name = name);
                }
            }
            SetAction::Delete(i) => {
                self.settings.edit_glyph_sets(|sets| { sets.remove(i); });
                self.char_palette.category = 0;
            }
            SetAction::Add(i, text) => self.add_to_set(i, text.chars()),
            SetAction::Remove(i, ch) => self.settings.edit_glyph_sets(|sets| sets[i].glyphs.retain(|c| c != ch)),
            SetAction::FromSelection(i) => {
                let (x0, y0, x1, y1) = match self.tools.selection.selection {
                    Some((x1, y1, x2, y2)) => (x1, y1, x2 + 1, y2 + 1),
                    None => (0, 0, self.doc.width, self.doc.height),
                };
                let layer = self.doc.active_layer();
                let used: Vec<char> = (y0..y1).flat_map(|y| (x0..x1).filter_map(move |x| layer.get(x, y).map(|c| c.ch))).collect();
                self.add_to_set(i, used.into_iter().filter(|&c| c != ' '));
            }
            SetAction::Manage => self.open_library(library::LibraryTab::GlyphSets),
            SetAction::Copy(i) => {
                ctx.copy_text(self.settings.glyph_sets[i].glyphs.clone());
                self.show_status("Copied the set's glyphs");
            }
        }
    }

    /// Adds the glyphs of `chars` that `set` doesn't have yet, in order.
    fn add_to_set(&mut self, set: usize, chars: impl Iterator<Item = char>) {
        let mut glyphs = self.settings.glyph_sets[set].glyphs.clone();
        let before = glyphs.chars().count();
        for ch in chars.filter(|c| !c.is_control() && !c.is_whitespace()) {
            if !glyphs.contains(ch) {
                glyphs.push(ch);
            }
        }
        let added = glyphs.chars().count() - before;
        self.settings.edit_glyph_sets(|sets| sets[set].glyphs = glyphs);
        self.show_status(match added {
            0 => "The set has those glyphs already".to_owned(),
            1 => "Added 1 glyph".to_owned(),
            n => format!("Added {n} glyphs"),
        });
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

    /// Copies the selection (or the whole picture) as plain text or ANSI-coloured text.
    fn copy_as(&mut self, ctx: &egui::Context, ansi: bool) {
        self.finish_3d_now();
        let region = match self.tools.selection.selection {
            Some((x1, y1, x2, y2)) => (x1, y1, x2 + 1, y2 + 1),
            None => crate::export::whole(&self.doc),
        };
        let text = if ansi {
            crate::export::to_ansi(&self.doc, region)
        } else {
            crate::export::to_text(&self.doc, region)
        };
        ctx.copy_text(text);
        let what = if self.tools.selection.selection.is_some() { "selection" } else { "picture" };
        self.show_status(format!("Copied the {what} as {}", if ansi { "ANSI" } else { "text" }));
        self.warn_if_overlaid();
    }

    /// After writing text: say so if overlaid glyphs had to give way to the top one.
    pub(super) fn warn_if_overlaid(&mut self) {
        if self.doc.has_overlaid_glyphs(self.doc.frame) {
            self.show_warning("Text holds one glyph per cell: where an Overlay layer draws over another glyph, only the top one was kept");
        }
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
            self.history.push_grouped(self.doc.active_layer, self.doc.frame, changes, self.nudge_group, "Move");
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
        // The 3D window holds a layer's place; anything that moves layers closes it first.
        if !matches!(action, LayerAction::Select(_) | LayerAction::ToggleVisible(_) | LayerAction::ToggleLock(_) | LayerAction::Rename(..) | LayerAction::Style { .. } | LayerAction::StartRename(_) | LayerAction::SelectContent(_)) {
            self.close_3d();
        }

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
                self.finish_3d_now();
                if self.doc.layers[active].blend == crate::document::Blend::Overlay {
                    self.show_warning("A cell holds one glyph: where this Overlay layer drew over another glyph, only its own was kept");
                }
                Command::LayerMergeDown {
                    upper:  active,
                    layer:  Box::new(self.doc.active_layer().clone()),
                    below:  Box::new(self.doc.layers[active - 1].clone()),
                    merged: Box::new(self.doc.merged_down(active)),
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
            LayerAction::New3D => {
                self.add_3d_layer("3D", crate::document::Scene3D::default());
                return;
            }
            LayerAction::Edit3D(i) => {
                if i < n { self.doc.active_layer = i; }
                self.edit_3d(i);
                return;
            }
            LayerAction::Rasterize3D(i) => {
                if i < n { self.doc.active_layer = i; }
                self.rasterize_3d();
                return;
            }
            LayerAction::StartRename(i) => {
                if let Some(layer) = self.doc.layers.get(i) {
                    self.layer_panel.start_rename(i, &layer.name);
                }
                return;
            }
            LayerAction::SelectContent(i) => {
                let Some(layer) = self.doc.layers.get(i) else { return };
                let filled = (0..self.doc.height).flat_map(|y| (0..self.doc.width).map(move |x| (x, y))).filter(|&(x, y)| layer.get(x, y).is_some_and(|c| !c.is_empty()));
                let bounds = filled.fold(None, |b: Option<(usize, usize, usize, usize)>, (x, y)| {
                    Some(b.map_or((x, y, x, y), |(x1, y1, x2, y2)| (x1.min(x), y1.min(y), x2.max(x), y2.max(y))))
                });
                match bounds {
                    Some(sel) => {
                        self.tools.selection.clear();
                        self.tools.selection.selection = Some(sel);
                        self.active_tool = ToolType::Select;
                    }
                    None => self.show_status("Nothing is drawn on this layer"),
                }
                return;
            }
            LayerAction::Style { index, blend, opacity, done } => {
                // Applied live; the whole edit becomes one undo step when it's done.
                let Some(layer) = self.doc.layers.get_mut(index) else { return };
                let before = *self.style_before.get_or_insert((index, layer.blend, layer.opacity));
                (layer.blend, layer.opacity) = (blend, opacity.clamp(0.0, 1.0));
                self.doc.modified = true;
                if done {
                    self.style_before = None;
                    let (old, new) = ((before.1, before.2), (layer.blend, layer.opacity));
                    if before.0 == index && old != new {
                        let label = if old.0 != new.0 { "Blend Mode" } else { "Layer Opacity" };
                        self.history.push_as(Command::LayerStyle { index, old, new }, label);
                    }
                }
                return;
            }
        };
        self.history.execute(cmd, &mut self.doc);
    }

    fn clear_layer(&mut self) {
        if self.active_layer_locked() { return; }
        let changes = self.doc.active_layer().clear_changes();
        if changes.is_empty() { return; }
        let layer_idx = self.doc.active_layer;
        self.history.execute_as(Command::Draw { layer_idx, frame: self.doc.frame, changes }, "Clear Layer", &mut self.doc);
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
    /// Writes unsaved work to the recovery file a little while after it changed.
    fn autosave(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if !self.autosave_due() {
            self.last_autosave = now;
            return;
        }
        let due = self.last_autosave + AUTOSAVE_DELAY;
        if now >= due && !self.drawing {
            self.refresh_autosave();
            self.last_autosave = now;
        } else {
            ctx.request_repaint_after_secs((due - now).max(0.1) as f32);
        }
    }

    /// Files dropped on the window open like File → Open.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.open_path(&path);
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop_overlay")));
            let screen = ctx.content_rect();
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
            painter.rect_stroke(screen.shrink(16.0), 12.0, Stroke::new(2.0, theme::ACCENT), StrokeKind::Inside);
            painter.text(screen.center(), Align2::CENTER_CENTER, "Drop a .vc / .txt file to open, a picture to import or an .obj model", theme::semibold(20.0), theme::TEXT);
        }
    }

    fn show_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_timer = 3.0;
    }

    /// A status message that stays longer.
    fn show_warning(&mut self, msg: &str) {
        self.status_msg = format!("{}  {msg}", crate::icons::ALERT);
        self.status_timer = 8.0;
    }

    // ── Unsaved changes ──────────────────────────────────────────────────────

    /// Runs `action` right away, or asks first about unsaved changes.
    fn request(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::CloseTab if self.doc.modified => self.pending_action = Some(action),
            PendingAction::CloseTab => self.close_front_tab(),
            PendingAction::Quit => match self.first_unsaved_tab() {
                Some(i) => {
                    self.switch_tab(i);
                    self.pending_action = Some(action);
                }
                None => {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            },
        }
    }

    /// Carries on with `action` once the unsaved changes in front were saved or given up.
    fn perform(&mut self, action: PendingAction, ctx: &egui::Context) {
        self.close_front_tab();
        if action == PendingAction::Quit {
            self.request(action, ctx);
        }
    }

    /// Intercepts closing the window while there are unsaved changes.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) { return; }
        if !self.allow_close && self.first_unsaved_tab().is_some() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.request(PendingAction::Quit, ctx);
        } else {
            // A clean exit: nothing to recover next time.
            storage::Autosave::remove();
        }
    }

    fn doc_name(&self) -> String {
        tabs::display_name(&self.doc)
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

/// The Gradient tool covers the selection, or the whole layer.
fn gradient_settings<'a>(options: &'a ToolOptions, selection: Option<(usize, usize, usize, usize)>, doc: &Document) -> GradientSettings<'a> {
    let region = match selection {
        Some((x1, y1, x2, y2)) => (x1, y1, x2 + 1, y2 + 1),
        None => (0, 0, doc.width, doc.height),
    };
    GradientSettings { shape: options.gradient, ramp: &options.ramp, dither: options.dither, region }
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
        self.poll_file_dialog(&ctx);
        self.poll_loading_picture(&ctx);
        self.run_render_job(&ctx);
        self.handle_keyboard(&ctx);
        self.advance_playback(&ctx);
        self.new_doc_dialog(&ctx);
        self.resize_dialog(&ctx);
        self.about_dialog(&ctx);
        self.unsaved_changes_modal(&ctx);
        self.recovery_modal(&ctx);
        self.image_import_dialog(&ctx);
        self.replace_dialog(&ctx);
        self.banner_dialog(&ctx);
        self.script_window(&ctx);
        self.three_d_window(&ctx);
        self.library_window(&ctx);
        self.waiting_window(&ctx);
        self.file_browser_dialog(&ctx);
        self.command_palette_dialog(&ctx);

        if self.active_tool != ToolType::Text {
            self.tools.text.stop();
        }

        // Order matters: panels claim space from the outside in, the canvas gets the rest.
        self.menu_bar(ui);
        self.options_bar(ui);
        self.status_bar(ui);
        self.tool_strip(ui);
        self.dock(ui);
        self.tab_bar(ui);
        self.timeline(ui);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG_DEEP))
            .show(ui, |ui| {
                let ctx_tool = self.draw_context(true);
                let settings = PreviewSettings {
                    shape: self.shape_for(self.active_tool),
                    ramp: &self.tool_options.ramp,
                    gradient: gradient_settings(&self.tool_options, self.tools.selection.selection, &self.doc),
                    pixel_grid: self.tool_options.pixel_grid,
                    brush: &self.tool_options.brush,
                };
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
