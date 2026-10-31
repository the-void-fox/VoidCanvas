//! File → Import Image: converts a picture into a new layer, with a live preview.

use eframe::egui::{self, *};
use image::{RgbaImage, imageops};

use super::AsciiArtEditor;
use crate::convert::{ConvertMode, ConvertSettings, convert, height_for};
use crate::document::{Cell, Layer};
use crate::history::Command;
use crate::theme;
use crate::ui::{canvas::draw_cell, glyph_render};

/// Image file extensions the importer reads.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Larger pictures are scaled down once on load, so previews stay quick.
const MAX_SOURCE_SIDE: u32 = 1024;

pub struct ImageImport {
    name: String,
    source: RgbaImage,
    pub(crate) settings: ConvertSettings,
    result: Vec<Vec<Cell>>,
    /// Settings `result` was made with.
    converted: Option<ConvertSettings>,
    grow_canvas: bool,
}

pub fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

impl AsciiArtEditor {
    pub(super) fn choose_image_to_import(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("Images", IMAGE_EXTENSIONS).pick_file() {
            self.start_image_import(&path);
        }
    }

    /// Loads the picture and opens the import dialog.
    pub fn start_image_import(&mut self, path: &std::path::Path) {
        let image = match image::open(path) {
            Ok(image) => image.to_rgba8(),
            Err(e) => {
                self.show_status(format!("Can't read image: {e}"));
                return;
            }
        };
        let (w, h) = image.dimensions();
        let source = if w.max(h) > MAX_SOURCE_SIDE {
            let scale = MAX_SOURCE_SIDE as f32 / w.max(h) as f32;
            imageops::thumbnail(&image, (w as f32 * scale) as u32, (h as f32 * scale) as u32)
        } else {
            image
        };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let settings = ConvertSettings { width: self.doc.width.min(80), ..ConvertSettings::default() };
        self.image_import = Some(ImageImport { name, source, settings, result: Vec::new(), converted: None, grow_canvas: false });
    }

    pub(super) fn image_import_dialog(&mut self, ctx: &egui::Context) {
        let Some(import) = &mut self.image_import else { return };
        if import.converted.as_ref() != Some(&import.settings) {
            import.result = convert(&import.source, &import.settings);
            import.converted = Some(import.settings.clone());
        }
        let rows = height_for(&import.source, import.settings.width);
        let too_big = import.settings.width > self.doc.width || rows > self.doc.height;

        let mut done = None;
        let modal = Modal::new(Id::new("import_image")).show(ctx, |ui| {
            ui.set_width(600.0);
            ui.heading(format!("Import {}", import.name));
            ui.add_space(8.0);
            preview(ui, &import.result, vec2(600.0, 320.0));
            ui.add_space(10.0);

            let s = &mut import.settings;
            Grid::new("import_settings").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.label("Mode");
                ui.horizontal(|ui| {
                    for mode in ConvertMode::ALL {
                        ui.selectable_value(&mut s.mode, mode, mode.name());
                    }
                });
                ui.end_row();

                ui.label("Size");
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut s.width).range(4..=500).suffix(" columns"));
                    ui.label(RichText::new(format!("× {rows} rows")).color(theme::TEXT_DIM));
                });
                ui.end_row();

                ui.label("Look");
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut s.color, "Colour");
                    ui.toggle_value(&mut s.invert, "Invert");
                    if s.mode != ConvertMode::HalfBlocks {
                        ui.toggle_value(&mut s.dither, "Dither");
                    }
                });
                ui.end_row();

                ui.label("Brightness");
                ui.add(Slider::new(&mut s.brightness, -0.5..=0.5).show_value(false));
                ui.end_row();

                ui.label("Contrast");
                ui.add(Slider::new(&mut s.contrast, 0.3..=3.0).show_value(false).logarithmic(true));
                ui.end_row();
            });

            if too_big {
                ui.add_space(4.0);
                ui.checkbox(&mut import.grow_canvas, "Grow the canvas to fit the picture");
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Import as layer").clicked() { done = Some(true); }
                if ui.button("Cancel").clicked() { done = Some(false); }
            });
        });
        if done.is_none() && modal.should_close() {
            done = Some(false);
        }

        match done {
            Some(true) => self.finish_image_import(),
            Some(false) => self.image_import = None,
            None => {}
        }
    }

    fn finish_image_import(&mut self) {
        let Some(import) = self.image_import.take() else { return };
        let rows = import.result.len();
        let cols = import.result.first().map_or(0, |r| r.len());
        if import.grow_canvas {
            self.resize_canvas(self.doc.width.max(cols), self.doc.height.max(rows));
        }
        let mut layer = Layer::new(self.doc.width, self.doc.height, format!("Image: {}", import.name));
        for (y, row) in import.result.into_iter().enumerate() {
            for (x, cell) in row.into_iter().enumerate() {
                layer.put(x, y, cell);
            }
        }
        let cmd = Command::LayerAdd { index: self.doc.active_layer + 1, layer };
        self.history.execute_as(cmd, "Import Image", &mut self.doc);
        self.show_status(format!("Imported {} as {cols}×{rows}", import.name));
    }
}

/// The converted cells, scaled to fit `size`.
fn preview(ui: &mut Ui, cells: &[Vec<Cell>], size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, theme::CANVAS_BG);
    let rows = cells.len();
    let cols = cells.first().map_or(0, |r| r.len());
    if rows == 0 || cols == 0 { return; }

    let cell_w = (size.x / cols as f32).min(size.y / (rows as f32 * 2.0));
    let cell = vec2(cell_w, cell_w * 2.0);
    let origin = rect.center() - vec2(cols as f32, rows as f32) * cell / 2.0;
    let ppp = ui.ctx().pixels_per_point();
    let strokes = glyph_render::Strokes::new(cell.x, ppp);
    let font = theme::mono(cell.y);
    for (y, row) in cells.iter().enumerate() {
        for (x, c) in row.iter().enumerate() {
            if c.is_empty() { continue; }
            let r = Rect::from_min_size(origin + vec2(x as f32, y as f32) * cell, cell);
            draw_cell(&painter, glyph_render::snap_rect(r, ppp), c.ch, c.fg.to_egui(), c.bg.to_egui(), &font, strokes);
        }
    }
}
