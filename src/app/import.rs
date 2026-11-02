//! File → Import Image: converts a picture into a new layer, with a live preview. An animated
//! GIF comes in whole: every frame into the animation's frames, or as an animation of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, *};
use image::RgbaImage;

use super::AsciiArtEditor;
use crate::convert::{ConvertMode, ConvertSettings, convert, height_for};
use crate::document::{Blend, Cell, Document, FrameData, Layer};
use crate::history::Command;
use crate::theme;
use crate::ui::{canvas::draw_cell, glyph_render};

/// Image file extensions the importer reads.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// A picture as read: one frame, or every frame of an animated GIF and how long each shows.
pub struct Picture {
    pub frames: Vec<RgbaImage>,
    /// Milliseconds per frame.
    pub delays: Vec<u32>,
    /// Only the first frames were kept: the whole animation was too large.
    pub truncated: bool,
}

pub struct ImageImport {
    name: String,
    picture: Picture,
    /// The frame in the preview.
    shown: usize,
    /// An animation comes in whole, not only the frame in the preview.
    all_frames: bool,
    pub(crate) settings: ConvertSettings,
    result: Vec<Vec<Cell>>,
    /// Settings and frame `result` was made with.
    converted: Option<(ConvertSettings, usize)>,
    grow_canvas: bool,
}

/// Where the frames of an animation go.
#[derive(Clone, Copy, PartialEq)]
pub enum ImportInto {
    /// A new layer, frame by frame from the frame in view; frames are added as needed.
    Layer { grow_canvas: bool },
    /// An animation of its own, in a new tab.
    NewDocument,
}

/// Every frame of an animation being converted, on other threads.
pub struct ConvertJob {
    pub(super) name: String,
    into: ImportInto,
    delays: Vec<u32>,
    pub(super) done: Arc<AtomicUsize>,
    pub(super) total: usize,
    pub(super) stop: Arc<AtomicBool>,
    pub(super) started: Instant,
    /// The frames as rows of cells, or nothing if it was stopped.
    frames: Receiver<Option<Vec<Vec<Vec<Cell>>>>>,
}

pub fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Converts every picture, on every core; `done` counts the frames finished. Nothing comes back
/// once `stop` is set.
pub fn convert_all(pictures: &[RgbaImage], settings: &ConvertSettings, done: &AtomicUsize, stop: &AtomicBool) -> Option<Vec<Vec<Vec<Cell>>>> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(pictures.len()).max(1);
    let chunk = pictures.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let parts: Vec<_> = pictures
            .chunks(chunk.max(1))
            .map(|part| {
                scope.spawn(move || {
                    let mut frames = Vec::with_capacity(part.len());
                    for picture in part {
                        if stop.load(Ordering::Relaxed) { return None; }
                        frames.push(convert(picture, settings));
                        done.fetch_add(1, Ordering::Relaxed);
                    }
                    Some(frames)
                })
            })
            .collect();
        let mut frames = Vec::with_capacity(pictures.len());
        for part in parts {
            frames.extend(part.join().ok()??);
        }
        Some(frames)
    })
}

impl AsciiArtEditor {
    /// The Import Image dialog for a picture that has been read.
    pub(super) fn open_image_import(&mut self, name: String, picture: Picture) {
        let settings = ConvertSettings { width: self.doc.width.min(80), ..ConvertSettings::default() };
        self.image_import = Some(ImageImport { name, picture, shown: 0, all_frames: true, settings, result: Vec::new(), converted: None, grow_canvas: false });
    }

    pub(super) fn image_import_dialog(&mut self, ctx: &egui::Context) {
        if self.image_import.is_none() { return; }
        let ramps = self.settings.all_ramps();
        let mut manage_ramps = false;
        let Some(import) = &mut self.image_import else { return };
        if import.converted.as_ref() != Some(&(import.settings.clone(), import.shown)) {
            import.result = convert(&import.picture.frames[import.shown], &import.settings);
            import.converted = Some((import.settings.clone(), import.shown));
        }
        let frames = import.picture.frames.len();
        let animated = frames > 1;
        let rows = height_for(&import.picture.frames[0], import.settings.width);
        let too_big = import.settings.width > self.doc.width || rows > self.doc.height;

        #[derive(PartialEq)]
        enum Done { Layer, NewDocument, Cancel }
        let mut done = None;
        let modal = Modal::new(Id::new("import_image")).show(ctx, |ui| {
            ui.set_width(600.0);
            ui.heading(format!("Import {}", import.name));
            if animated {
                let seconds = import.picture.delays.iter().sum::<u32>() as f32 / 1000.0;
                let cut = if import.picture.truncated { " — the first ones; the rest is too large to take in" } else { "" };
                ui.label(RichText::new(format!("Animation: {frames} frames, {seconds:.1} s{cut}")).color(theme::TEXT_DIM));
            }
            ui.add_space(8.0);
            preview(ui, &import.result, vec2(600.0, 320.0));
            ui.add_space(10.0);

            let s = &mut import.settings;
            Grid::new("import_settings").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                if animated {
                    ui.label("Frame");
                    ui.horizontal(|ui| {
                        let mut number = import.shown + 1;
                        ui.add(Slider::new(&mut number, 1..=frames));
                        import.shown = number - 1;
                        ui.checkbox(&mut import.all_frames, format!("All {frames} frames"));
                    });
                    ui.end_row();
                }

                ui.label("Mode");
                ui.horizontal(|ui| {
                    for mode in ConvertMode::ALL {
                        ui.selectable_value(&mut s.mode, mode, mode.name());
                    }
                    if s.mode == ConvertMode::Ascii {
                        let name = ramps.iter().find(|r| r.glyphs == s.ramp).map_or("Custom", |r| r.name.as_str()).to_owned();
                        match super::chrome::ramp_menu(ui, "import_ramp", &name, &ramps) {
                            Some(Some(ramp)) => s.ramp = ramp.glyphs,
                            Some(None) => manage_ramps = true,
                            None => {}
                        }
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
                let whole = animated && import.all_frames;
                let layer = ui.button("Import as layer");
                let layer = if whole { layer.on_hover_text("Frame by frame from the frame in view; frames are added as needed") } else { layer };
                if layer.clicked() { done = Some(Done::Layer); }
                if whole && ui.button("New animation").on_hover_text("The frames as a document of their own, with the GIF's timing").clicked() {
                    done = Some(Done::NewDocument);
                }
                if ui.button("Cancel").clicked() { done = Some(Done::Cancel); }
            });
        });
        if done.is_none() && modal.should_close() {
            done = Some(Done::Cancel);
        }

        let whole = self.image_import.as_ref().is_some_and(|i| i.all_frames && i.picture.frames.len() > 1);
        match done {
            Some(Done::Layer) if whole => {
                let grow_canvas = self.image_import.as_ref().is_some_and(|i| i.grow_canvas && too_big);
                self.start_converting(ImportInto::Layer { grow_canvas });
            }
            Some(Done::Layer) => self.finish_image_import(),
            Some(Done::NewDocument) => self.start_converting(ImportInto::NewDocument),
            Some(Done::Cancel) => self.image_import = None,
            None => {}
        }
        if manage_ramps {
            self.open_library(super::library::LibraryTab::Ramps);
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

    /// Converts every frame of the animation on other threads; the waiting window shows how far.
    fn start_converting(&mut self, into: ImportInto) {
        let Some(import) = self.image_import.take() else { return };
        let Picture { frames: pictures, delays, .. } = import.picture;
        let (done, stop) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicBool::new(false)));
        let (send, frames) = channel();
        let (counter, stopper, settings) = (done.clone(), stop.clone(), import.settings);
        let total = pictures.len();
        std::thread::spawn(move || {
            let _ = send.send(convert_all(&pictures, &settings, &counter, &stopper));
        });
        self.convert_job = Some(ConvertJob { name: import.name, into, delays, done, total, stop, started: Instant::now(), frames });
    }

    /// Are the frames of an animation being converted?
    #[cfg(test)]
    pub(crate) fn converting(&self) -> bool {
        self.convert_job.is_some()
    }

    pub(super) fn poll_convert_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.convert_job else { return };
        let frames = match job.frames.try_recv() {
            Ok(frames) => frames,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(TryRecvError::Disconnected) => None,
        };
        let job = self.convert_job.take().unwrap();
        let Some(frames) = frames.filter(|f| !f.is_empty()) else {
            return self.show_status(format!("Import of {} cancelled", job.name));
        };
        match job.into {
            ImportInto::Layer { grow_canvas } => self.import_frames_as_layer(&job.name, frames, &job.delays, grow_canvas),
            ImportInto::NewDocument => self.open_frames_as_document(&job.name, frames, job.delays),
        }
    }

    /// A new layer holding the frames from the frame in view on; frames are added at the end
    /// as needed, with the animation's timing (a still picture takes it all). One undo step.
    fn import_frames_as_layer(&mut self, name: &str, frames: Vec<Vec<Vec<Cell>>>, delays: &[u32], grow_canvas: bool) {
        let imported = frames.len();
        let rows = frames[0].len();
        let cols = frames[0].first().map_or(0, |r| r.len());
        if grow_canvas {
            self.resize_canvas(self.doc.width.max(cols), self.doc.height.max(rows));
        }
        let (w, h) = (self.doc.width, self.doc.height);
        let (count, start) = (self.doc.frame_count(), self.doc.frame);
        let total = count.max(start + frames.len());
        let blank = vec![Cell::default(); w * h];
        let mut steps: Vec<Command> = (count..total)
            .map(|f| Command::FrameAdd { index: f, data: FrameData { duration: delays[f - start], cells: vec![blank.clone(); self.doc.layers.len()] } })
            .collect();
        if count == 1 {
            let old: Vec<u32> = self.doc.durations.iter().copied().chain((count..total).map(|f| delays[f - start])).collect();
            let mut new = old.clone();
            new[0] = delays[0];
            steps.push(Command::FrameTiming { old, new });
        }
        let mut cells = vec![blank; total];
        for (i, frame) in frames.into_iter().enumerate() {
            let target = &mut cells[start + i];
            for (y, row) in frame.into_iter().enumerate().take(h) {
                for (x, cell) in row.into_iter().enumerate().take(w) {
                    target[y * w + x] = cell;
                }
            }
        }
        let layer = Layer::from_frames(format!("Image: {name}"), w, h, true, false, 1.0, Blend::Normal, None, cells, 0);
        steps.push(Command::LayerAdd { index: self.doc.active_layer + 1, layer });
        self.history.execute_as(Command::Group(steps), "Import Animation", &mut self.doc);
        self.doc.show_frame(start);
        let added = total - count;
        let note = if added > 0 { format!(", {added} frames added") } else { String::new() };
        self.show_status(format!("Imported {name}: {imported} frames of {cols}×{rows}{note}"));
    }

    /// The frames as an animation of its own, in a new tab.
    fn open_frames_as_document(&mut self, name: &str, frames: Vec<Vec<Vec<Cell>>>, delays: Vec<u32>) {
        let rows = frames[0].len().max(1);
        let cols = frames[0].first().map_or(0, |r| r.len()).max(1);
        let mut doc = Document::new(cols, rows);
        let cells: Vec<Vec<Cell>> = frames
            .into_iter()
            .map(|frame| {
                let mut cells = vec![Cell::default(); cols * rows];
                for (y, row) in frame.into_iter().enumerate().take(rows) {
                    for (x, cell) in row.into_iter().enumerate().take(cols) {
                        cells[y * cols + x] = cell;
                    }
                }
                cells
            })
            .collect();
        let layer = &doc.layers[0];
        doc.layers[0] = Layer::from_frames(layer.name.clone(), cols, rows, true, false, 1.0, Blend::Normal, None, cells, 0);
        doc.durations = delays;
        doc.modified = true;
        self.open_in_tab(doc);
        self.show_status(format!("Opened {name} as an animation: {} frames of {cols}×{rows}", self.doc.frame_count()));
    }
}

/// Cells drawn like on the canvas (no font ligatures), scaled to fit `size`.
pub(super) fn preview(ui: &mut Ui, cells: &[Vec<Cell>], size: Vec2) {
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
