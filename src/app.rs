use eframe::egui::{self, *};

use crate::document::{Cell, Document};
use crate::history::{Command, History};
use crate::tools::{
    DrawContext, ToolType,
    brush::BrushTool,
    eraser::EraserTool,
    fill::FillTool,
    shapes::{RectangleTool, EllipseTool, LineTool},
    border::BorderTool,
    selection::SelectionTool,
};
use crate::ui::{
    canvas::{CanvasEvent, CanvasState, draw_canvas},
    char_palette::CharPalette,
    layer_panel::{LayerAction, LayerPanel},
    panels::{ColorPanel, ToolOptionsPanel},
};

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
    pub drawing: bool,
    pub last_primary: bool,
    pub status_msg: String,
    pub status_timer: f32,
    pub show_new_dialog:    bool,
    pub show_resize_dialog: bool,
    pub show_about_dialog:  bool,
    pub about_tab:          usize, // 0 = About & Shortcuts, 1 = Author
    pub new_width:  usize,
    pub new_height: usize,
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
            clipboard:    None,
            drawing:      false,
            last_primary: true,
            status_msg:  String::from("Ready"),
            status_timer: 0.0,
            show_new_dialog:    false,
            show_resize_dialog: false,
            show_about_dialog:  false,
            about_tab:          0,
            new_width:  80,
            new_height: 40,
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

    fn handle_canvas_event(&mut self, event: CanvasEvent) {
        match event {
            CanvasEvent::Press { x, y, primary } => {
                self.drawing = true;
                self.last_primary = primary;
                let ctx = self.draw_context(primary);
                self.sync_tool_options();

                let changes = match self.active_tool {
                    ToolType::Brush  => self.tools.brush.on_press(&mut self.doc, x, y, &ctx),
                    ToolType::Eraser => self.tools.eraser.on_press(&mut self.doc, x, y, &ctx),
                    ToolType::Fill => {
                        let c = self.tools.fill.flood_fill(&mut self.doc, x, y, &ctx);
                        if !c.is_empty() {
                            self.history.push(Command::Draw {
                                layer_idx: self.doc.active_layer,
                                changes: c,
                            });
                            self.doc.modified = true;
                        }
                        return;
                    }
                    ToolType::Rectangle => { self.tools.rect.on_press(x, y);    return; }
                    ToolType::Ellipse   => { self.tools.ellipse.on_press(x, y); return; }
                    ToolType::Line      => { self.tools.line.on_press(x, y);    return; }
                    ToolType::Border    => { self.tools.border.on_press(x, y);  return; }
                    ToolType::Select => {
                        self.tools.selection.on_press(&self.doc, x, y);
                        return;
                    }
                };

                if !changes.is_empty() { self.doc.modified = true; }
            }

            CanvasEvent::Drag { x, y, primary } => {
                let ctx = self.draw_context(primary);
                match self.active_tool {
                    ToolType::Brush  => { self.tools.brush.on_drag(&mut self.doc, x, y, &ctx); }
                    ToolType::Eraser => { self.tools.eraser.on_drag(&mut self.doc, x, y, &ctx); }
                    ToolType::Select => { self.tools.selection.on_drag(&mut self.doc, x, y); }
                    _ => {}
                }
                self.doc.modified = true;
            }

            CanvasEvent::Release => {
                if !self.drawing { return; }
                self.drawing = false;

                let primary = self.last_primary;
                let ctx = self.draw_context(primary);
                let cur = self.canvas_state.cursor_cell.unwrap_or((0, 0));

                let changes = match self.active_tool {
                    ToolType::Brush  => self.tools.brush.on_release(),
                    ToolType::Eraser => self.tools.eraser.on_release(),
                    ToolType::Rectangle => self.tools.rect.on_release(&mut self.doc, cur.0, cur.1, &ctx),
                    ToolType::Ellipse   => self.tools.ellipse.on_release(&mut self.doc, cur.0, cur.1, &ctx),
                    ToolType::Line      => self.tools.line.on_release(&mut self.doc, cur.0, cur.1, &ctx),
                    ToolType::Border    => self.tools.border.on_release(&mut self.doc, cur.0, cur.1, &ctx),
                    ToolType::Select    => self.tools.selection.on_release(&mut self.doc),
                    _ => Default::default(),
                };

                if !changes.is_empty() {
                    self.history.push(Command::Draw {
                        layer_idx: self.doc.active_layer,
                        changes,
                    });
                    self.doc.modified = true;
                }
            }
        }
    }

    fn sync_tool_options(&mut self) {
        self.tools.rect.filled    = self.tool_options.filled;
        self.tools.ellipse.filled = self.tool_options.filled;
        self.tools.border.style   = self.tool_options.border_style;
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let (ctrl_c, ctrl_x, ctrl_v, ctrl_z, ctrl_y, ctrl_n, ctrl_s, ctrl_o) = ctx.input_mut(|i| {
            let c = i.consume_key(egui::Modifiers::CTRL, Key::C);
            let x = i.consume_key(egui::Modifiers::CTRL, Key::X);
            let v = i.consume_key(egui::Modifiers::CTRL, Key::V);
            let z = i.consume_key(egui::Modifiers::CTRL, Key::Z);
            let y = i.consume_key(egui::Modifiers::CTRL, Key::Y);
            let n = i.consume_key(egui::Modifiers::CTRL, Key::N);
            let s = i.consume_key(egui::Modifiers::CTRL, Key::S);
            let o = i.consume_key(egui::Modifiers::CTRL, Key::O);
            (c, x, v, z, y, n, s, o)
        });

        if ctrl_c { self.copy(); }
        if ctrl_x { self.cut(); }
        if ctrl_v { self.paste(); }
        if ctrl_z {
            if self.history.undo(&mut self.doc) {
                self.show_status("Undo");
            }
        }
        if ctrl_y {
            if self.history.redo(&mut self.doc) {
                self.show_status("Redo");
            }
        }
        if ctrl_n {
            self.new_width  = self.doc.width;
            self.new_height = self.doc.height;
            self.show_new_dialog = true;
        }
        if ctrl_s { self.save(); }
        if ctrl_o { self.open(); }

        ctx.input(|i| {
            if i.key_pressed(Key::B) { self.active_tool = ToolType::Brush; }
            if i.key_pressed(Key::E) { self.active_tool = ToolType::Eraser; }
            if i.key_pressed(Key::F) { self.active_tool = ToolType::Fill; }
            if i.key_pressed(Key::R) { self.active_tool = ToolType::Rectangle; }
            if i.key_pressed(Key::O) { self.active_tool = ToolType::Ellipse; }
            if i.key_pressed(Key::L) { self.active_tool = ToolType::Line; }
            if i.key_pressed(Key::D) { self.active_tool = ToolType::Border; }
            if i.key_pressed(Key::S) { self.active_tool = ToolType::Select; }

            if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                self.tool_options.brush_size = (self.tool_options.brush_size + 1).min(20);
            }
            if i.key_pressed(Key::Minus) {
                self.tool_options.brush_size = self.tool_options.brush_size.saturating_sub(1).max(1);
            }

            if i.key_pressed(Key::X) && !i.modifiers.ctrl {
                std::mem::swap(&mut self.primary_char, &mut self.secondary_char);
                std::mem::swap(&mut self.color_panel.primary_fg, &mut self.color_panel.secondary_fg);
                std::mem::swap(&mut self.color_panel.primary_bg, &mut self.color_panel.secondary_bg);
            }

            if i.key_pressed(Key::Delete) { self.delete_selection(); }
            if i.key_pressed(Key::Escape) {
                self.tools.selection.selection = None;
            }
        });
    }

    fn copy(&mut self) {
        self.clipboard = self.tools.selection.copy(&self.doc);
        if self.clipboard.is_some() {
            self.show_status("Copied selection");
        }
    }

    fn cut(&mut self) {
        let (content, changes) = self.tools.selection.cut(&mut self.doc);
        if let Some(c) = content {
            self.clipboard = Some(c);
            if !changes.is_empty() {
                self.history.push(Command::Draw { layer_idx: self.doc.active_layer, changes });
                self.doc.modified = true;
            }
            self.show_status("Cut selection");
        }
    }

    fn paste(&mut self) {
        if let Some(content) = self.clipboard.clone() {
            let (x, y) = self.canvas_state.cursor_cell.unwrap_or((0, 0));
            let changes = self.tools.selection.paste(&mut self.doc, &content, x, y);
            if !changes.is_empty() {
                self.history.push(Command::Draw { layer_idx: self.doc.active_layer, changes });
                self.doc.modified = true;
                self.show_status("Pasted");
            }
        } else {
            self.show_status("Clipboard is empty");
        }
    }

    fn delete_selection(&mut self) {
        let changes = self.tools.selection.delete(&mut self.doc);
        if !changes.is_empty() {
            self.history.push(Command::Draw { layer_idx: self.doc.active_layer, changes });
            self.doc.modified = true;
        }
    }

    fn show_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_timer = 3.0;
    }

    fn new_doc_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.show_new_dialog;
        egui::Window::new("New Document")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(DragValue::new(&mut self.new_width).clamp_range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(DragValue::new(&mut self.new_height).clamp_range(1..=500usize));
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
                        self.show_new_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_new_dialog = false;
                    }
                });
            });
        self.show_new_dialog = open;
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
                    ui.add(DragValue::new(&mut self.new_width).clamp_range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(DragValue::new(&mut self.new_height).clamp_range(1..=500usize));
                });
                ui.horizontal(|ui| {
                    if ui.button("Resize").clicked() {
                        self.doc.resize(self.new_width, self.new_height);
                        self.show_resize_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_resize_dialog = false;
                    }
                });
            });
        self.show_resize_dialog = open;
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
                                egui::RichText::new("v1.0.1")
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
                                ui.label("Ctrl+S"); ui.label("Save / Save as…"); ui.end_row();

                                ui.add_space(4.0); ui.end_row();
                                header(ui, "── Edit ──");
                                ui.end_row();
                                ui.label("Ctrl+Z"); ui.label("Undo"); ui.end_row();
                                ui.label("Ctrl+Y"); ui.label("Redo"); ui.end_row();
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

                let Some(cell) = self.doc.composite_cell(cx, cy) else { continue };

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

    fn save(&mut self) {
        let path = if let Some(p) = &self.doc.file_path {
            Some(p.clone())
        } else {
            rfd::FileDialog::new()
                .add_filter("Void Canvas", &["vc"])
                .save_file()
                .map(|p| {
                    let mut p = p;
                    if p.extension().map_or(true, |ext| ext != "vc") {
                        p.set_extension("vc");
                    }
                    p.to_string_lossy().to_string()
                })
        };

        if let Some(path) = path {
            let data = serde_json::to_string_pretty(&self.doc).unwrap_or_default();
            if std::fs::write(&path, data).is_ok() {
                self.doc.file_path = Some(path.clone());
                self.doc.modified = false;
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(path.clone());
                self.show_status(format!("Saved: {name}"));
            } else {
                self.show_status("Save failed!");
            }
        }
    }

    fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Void Canvas", &["vc"])
            .pick_file()
        {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(mut doc) = serde_json::from_str::<Document>(&data) {
                    doc.file_path = Some(path.to_string_lossy().to_string());
                    self.doc = doc;
                    self.history.clear();
                    let name = path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.show_status(format!("Opened: {name}"));
                } else {
                    self.show_status("Failed to parse file");
                }
            } else {
                self.show_status("Failed to read file");
            }
        }
    }
}

// ── eframe::App ──────────────────────────────────────────────────────────────

impl eframe::App for AsciiArtEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.unstable_dt);
        self.canvas_state.march_phase = (self.canvas_state.march_phase + dt * 0.5) % 1.0;
        if self.tools.selection.selection.is_some() {
            ctx.request_repaint();
        }

        if self.status_timer > 0.0 {
            self.status_timer -= dt;
        }

        self.handle_keyboard(ctx);
        self.new_doc_dialog(ctx);
        self.resize_dialog(ctx);
        self.about_dialog(ctx);

        // ── Menu bar ─────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New…       Ctrl+N").clicked() {
                        self.new_width  = self.doc.width;
                        self.new_height = self.doc.height;
                        self.show_new_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("Open…      Ctrl+O").clicked() { self.open(); ui.close_menu(); }
                    if ui.button("Save        Ctrl+S").clicked() { self.save(); ui.close_menu(); }
                    ui.separator();
                    if ui.button("Export Text…").clicked() { self.export_text(); ui.close_menu(); }
                    if ui.button("Export PNG…").clicked()  { self.export_png();  ui.close_menu(); }
                    ui.separator();
                    if ui.button("Quit").clicked() { std::process::exit(0); }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(self.history.can_undo(), egui::Button::new("Undo  Ctrl+Z")).clicked() {
                        self.history.undo(&mut self.doc);
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.history.can_redo(), egui::Button::new("Redo  Ctrl+Y")).clicked() {
                        self.history.redo(&mut self.doc);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Copy    Ctrl+C").clicked() { self.copy();             ui.close_menu(); }
                    if ui.button("Cut     Ctrl+X").clicked() { self.cut();              ui.close_menu(); }
                    if ui.button("Paste   Ctrl+V").clicked() { self.paste();            ui.close_menu(); }
                    if ui.button("Delete").clicked()         { self.delete_selection(); ui.close_menu(); }
                    ui.separator();
                    if ui.button("Clear Layer").clicked() {
                        self.doc.active_layer_mut().clear();
                        self.doc.modified = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Canvas", |ui| {
                    if ui.button("Resize…").clicked() {
                        self.new_width  = self.doc.width;
                        self.new_height = self.doc.height;
                        self.show_resize_dialog = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Zoom In   Ctrl +").clicked() {
                        self.canvas_state.zoom = (self.canvas_state.zoom * 1.25).min(6.0);
                        ui.close_menu();
                    }
                    if ui.button("Zoom Out  Ctrl -").clicked() {
                        self.canvas_state.zoom = (self.canvas_state.zoom / 1.25).max(0.2);
                        ui.close_menu();
                    }
                    if ui.button("Reset Zoom").clicked() {
                        self.canvas_state.zoom = 1.0;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.checkbox(&mut self.canvas_state.show_grid, "Show Grid");
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.about_tab = 0;
                        self.show_about_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("Author").clicked() {
                        self.about_tab = 1;
                        self.show_about_dialog = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // ── Status bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
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
        egui::SidePanel::left("toolbar")
            .resizable(false)
            .exact_width(44.0)
            .show(ctx, |ui| {
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
        egui::SidePanel::right("layers")
            .resizable(true)
            .default_width(220.0)
            .width_range(160.0..=400.0)
            .show(ctx, |ui| {
                ui.heading("Layers");
                ui.separator();

                if let Some(action) = self.layer_panel.show(ui, &self.doc) {
                    match action {
                        LayerAction::Select(i) => { self.doc.active_layer = i; }
                        LayerAction::Add       => { self.doc.add_layer(); }
                        LayerAction::Remove    => { self.doc.remove_layer(); }
                        LayerAction::Duplicate => { self.doc.duplicate_layer(); }
                        LayerAction::MergeDown => { self.doc.merge_down(); }
                        LayerAction::MoveUp => {
                            let i = self.doc.active_layer;
                            if i + 1 < self.doc.layers.len() { self.doc.move_layer(i, i + 1); }
                        }
                        LayerAction::MoveDown => {
                            let i = self.doc.active_layer;
                            if i > 0 { self.doc.move_layer(i, i - 1); }
                        }
                        LayerAction::ToggleVisible(i) => { self.doc.layers[i].visible = !self.doc.layers[i].visible; }
                        LayerAction::ToggleLock(i)    => { self.doc.layers[i].locked  = !self.doc.layers[i].locked; }
                        LayerAction::Rename(i, name)  => {
                            if i < self.doc.layers.len() { self.doc.layers[i].name = name; }
                        }
                        LayerAction::Reorder(from, to) => {
                            if from < self.doc.layers.len() && to < self.doc.layers.len() {
                                self.doc.move_layer(from, to);
                            }
                        }
                    }
                    ctx.request_repaint();
                }
            });

        // ── Left: inspector panel ─────────────────────────────────────────────
        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(280.0)
            .width_range(200.0..=480.0)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                egui::TopBottomPanel::top("left_tabs_wrapper")
                    .show_inside(ui, |_| {});
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::collapsing_header::CollapsingHeader::new("🔤 Characters")
                        .default_open(true)
                        .show(ui, |ui| {
                            self.char_palette.show(ui, &mut self.primary_char, &mut self.secondary_char);
                        });
                    egui::collapsing_header::CollapsingHeader::new("🎨 Colors")
                        .default_open(true)
                        .show(ui, |ui| {
                            self.color_panel.show(ui);
                        });
                    egui::collapsing_header::CollapsingHeader::new("⚙ Tool Options")
                        .default_open(false)
                        .show(ui, |ui| {
                            self.tool_options.show(ui);
                        });
                });
            });

        // ── Central canvas ───────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_gray(28)))
            .show(ctx, |ui| {
                let ctx_tool = self.draw_context(true);
                let event = draw_canvas(
                    ui,
                    &self.doc,
                    &mut self.canvas_state,
                    &ctx_tool,
                    self.active_tool,
                    &mut self.tools,
                );
                if let Some(ev) = event {
                    self.handle_canvas_event(ev);
                }
            });
    }
}