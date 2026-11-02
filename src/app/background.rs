//! Work done off the UI thread, so the window keeps answering: rendering 3D layers, the system
//! file dialog, opening documents, reading pictures and exporting images. A small window (or, for
//! documents, a tab with a spinner) shows what is going on meanwhile.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, *};
use image::RgbaImage;

use super::{AsciiArtEditor, PendingAction};
use super::file_dialog::FileAction;
use crate::document::{Cell, Document};
use crate::{export, theme};

/// 3D frames being rendered in the background.
pub struct RenderJob {
    /// The document's render generation it started from; results for another are dropped.
    generation: u64,
    results: Receiver<(usize, usize, Vec<Cell>)>,
    pub total: usize,
    pub done: usize,
}

/// The system file dialog, open on its own thread.
pub struct PendingDialog {
    pub action: FileAction,
    pub suggested: String,
    started: Instant,
    picked: Receiver<Option<PathBuf>>,
}

/// A document file being read and decoded.
pub struct OpeningFile {
    /// The path as given: for the tab, the recent files and messages.
    pub path: String,
    document: Receiver<Result<Document, OpenError>>,
}

enum OpenError {
    Unreadable(String),
    Damaged(String),
}

/// A picture being read for Import Image.
pub struct LoadingPicture {
    name: String,
    image: Receiver<Result<RgbaImage, String>>,
}

/// What a file job writes.
#[derive(Clone, Copy, PartialEq)]
pub enum Writing {
    Png { transparent: bool },
    Gif,
    /// The document itself, as a `.vc` file.
    Document,
}

/// A file being written from a copy of the document: a PNG, a GIF, or the document itself.
pub struct FileJob {
    writing: Writing,
    path: PathBuf,
    /// Runs once the document has been saved.
    then: Option<PendingAction>,
    /// The document's render generation it started from: 3D frames it renders are kept if it still matches.
    generation: u64,
    progress: Receiver<FileProgress>,
    /// Set by Cancel; an export stops after the frame in hand.
    stop: Arc<AtomicBool>,
    /// 3D frames to render first, and how many are done.
    renders: (usize, usize),
    /// Frames to write, and how many are in the file.
    frames: (usize, usize),
    started: Instant,
}

enum FileProgress {
    /// A 3D frame that was out of date (the editor takes it too).
    Rendered(usize, usize, Vec<Cell>),
    /// Frames in the file so far.
    Written(usize),
    /// Done; for a document, whether it was written compressed.
    Finished(Result<bool, String>),
}

/// The waiting window shows up only for work that takes longer than this, so a quick save
/// doesn't flash a window.
const SHOW_WAITING_AFTER: Duration = Duration::from_millis(250);

/// A system dialog that returns faster than this was never shown (nobody cancels that quickly).
const FAILED_DIALOG: Duration = Duration::from_millis(300);
/// Larger pictures are scaled down once on load, so previews stay quick.
const MAX_SOURCE_SIDE: u32 = 1024;

impl AsciiArtEditor {
    /// Takes in rendered 3D frames and starts rendering the ones still out of date.
    pub(crate) fn run_render_job(&mut self, ctx: &egui::Context) {
        if let Some(job) = &mut self.render_job {
            let current = job.generation == self.doc.render_gen;
            let mut finished = false;
            loop {
                match job.results.try_recv() {
                    Ok((layer, frame, cells)) => {
                        if current {
                            self.doc.finish_3d(layer, frame, cells);
                        }
                        job.done += 1;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            if finished || !current {
                self.render_job = None;
            }
        }
        // A file job renders the 3D frames it needs itself.
        if self.render_job.is_none() && self.file_job.is_none() {
            let work = self.doc.stale_3d();
            if !work.is_empty() {
                let (w, h) = (self.doc.width, self.doc.height);
                let scenes: Vec<_> = work.iter().map(|&(layer, frame)| (layer, frame, self.doc.layers[layer].scene.clone())).collect();
                let (send, results) = channel();
                std::thread::spawn(move || {
                    for (layer, frame, scene) in scenes {
                        let Some(scene) = scene else { continue };
                        // Nobody listening any more: the work is out of date.
                        if send.send((layer, frame, scene.render(w, h, frame))).is_err() {
                            return;
                        }
                    }
                });
                self.render_job = Some(RenderJob { generation: self.doc.render_gen, results, total: work.len(), done: 0 });
            }
        }
        if self.render_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    /// Writes a PNG, a GIF or the document on another thread, from a copy of the document; 3D
    /// frames still out of date are rendered there first. Input waits meanwhile; the waiting
    /// window shows how far it is. `then` runs once a document has been saved.
    pub(super) fn start_writing(&mut self, writing: Writing, path: PathBuf, then: Option<PendingAction>) {
        // The job renders what it needs; the editor's own 3D work waits until it is done.
        self.render_job = None;
        let mut doc = self.doc.clone();
        let frames: Vec<usize> = match writing {
            Writing::Png { .. } => vec![doc.frame],
            Writing::Gif | Writing::Document => (0..doc.frame_count()).collect(),
        };
        let stale: Vec<(usize, usize)> = doc.stale_3d().into_iter().filter(|(_, frame)| frames.contains(frame)).collect();
        let stop = Arc::new(AtomicBool::new(false));
        let (send, progress) = channel();
        self.file_job = Some(FileJob {
            writing,
            path: path.clone(),
            then,
            generation: doc.render_gen,
            progress,
            stop: stop.clone(),
            renders: (stale.len(), 0),
            frames: (frames.len(), 0),
            started: Instant::now(),
        });
        std::thread::spawn(move || {
            let (w, h) = (doc.width, doc.height);
            for (layer, frame) in stale {
                if stop.load(Ordering::Relaxed) { break; }
                let Some(scene) = doc.layers[layer].scene.clone() else { continue };
                let cells = scene.render(w, h, frame);
                doc.finish_3d(layer, frame, cells.clone());
                if send.send(FileProgress::Rendered(layer, frame, cells)).is_err() { return; }
            }
            let stopped = || stop.load(Ordering::Relaxed);
            let result = if stopped() {
                Err("stopped".into())
            } else {
                match writing {
                    Writing::Gif => export::write_gif(&doc, &path, |written| !stopped() && send.send(FileProgress::Written(written)).is_ok()).map(|()| true),
                    Writing::Png { transparent } => export::render_png(&doc, transparent).and_then(|image| {
                        if stopped() { return Err("stopped".into()); }
                        image.save(&path).map(|()| true).map_err(|e| e.to_string())
                    }),
                    Writing::Document => super::file_io::document_bytes(&doc).and_then(|(bytes, compressed)| {
                        super::storage::write_atomically(&path, bytes).map(|()| compressed).map_err(|e| e.to_string())
                    }),
                }
            };
            let _ = send.send(FileProgress::Finished(result));
        });
    }

    pub(super) fn poll_file_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.file_job else { return };
        let finished = loop {
            match job.progress.try_recv() {
                Ok(FileProgress::Rendered(layer, frame, cells)) => {
                    job.renders.1 += 1;
                    if job.generation == self.doc.render_gen {
                        self.doc.finish_3d(layer, frame, cells);
                    }
                }
                Ok(FileProgress::Written(written)) => job.frames.1 = written,
                Ok(FileProgress::Finished(result)) => break Some(result),
                Err(TryRecvError::Empty) => break None,
                Err(TryRecvError::Disconnected) => break Some(Err("the writer stopped unexpectedly".into())),
            }
        };
        let Some(result) = finished else {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        };
        let job = self.file_job.take().unwrap();
        let kind = match job.writing {
            Writing::Png { .. } => "PNG",
            Writing::Gif => "GIF",
            Writing::Document => {
                return match result {
                    Ok(compressed) => self.saved(job.path.to_string_lossy().to_string(), compressed, job.then, ctx),
                    Err(e) => self.show_status(format!("Save failed: {e}")),
                };
            }
        };
        match result {
            Ok(_) => self.show_status(format!("Exported {kind}: {}", job.path.display())),
            Err(_) if job.stop.load(Ordering::Relaxed) => self.show_status(format!("{kind} export cancelled")),
            Err(e) => self.show_status(format!("{kind} export failed: {e}")),
        }
    }

    /// Is a file being written?
    #[cfg(test)]
    pub(crate) fn writing_file(&self) -> bool {
        self.file_job.is_some()
    }

    /// Renders whatever 3D work is left before something reads the cells (save, export, merge…).
    pub(super) fn finish_3d_now(&mut self) {
        self.render_job = None;
        self.doc.render_3d_now();
    }

    /// Opens the system file dialog on its own thread; the answer arrives in `poll_file_dialog`.
    pub(super) fn open_system_dialog(&mut self, action: FileAction, suggested: String) {
        let (kind, extensions) = action.filter();
        let (kind, extensions): (String, Vec<String>) = (kind.to_owned(), extensions.iter().map(|e| e.to_string()).collect());
        let (saves, name) = (action.saves(), suggested.clone());
        let (send, picked) = channel();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().add_filter(&kind, &extensions);
            let path = if saves { dialog.set_file_name(&name).save_file() } else { dialog.pick_file() };
            let _ = send.send(path);
        });
        self.pending_dialog = Some(PendingDialog { action, suggested, started: Instant::now(), picked });
    }

    pub(super) fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending_dialog else { return };
        let answer = match pending.picked.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(TryRecvError::Disconnected) => None,
        };
        let pending = self.pending_dialog.take().unwrap();
        match answer {
            Some(path) => self.finish_file_action(pending.action, path),
            None if pending.started.elapsed() >= FAILED_DIALOG => {} // cancelled
            None => {
                self.system_dialog_failed = true;
                self.show_status("The system file dialog didn't open, using the built-in one");
                self.open_file_browser(pending.action, pending.suggested);
            }
        }
    }

    /// Reads and decodes a `.vc` file on another thread; it opens in a tab when it's ready, and
    /// the editor can be used meanwhile.
    pub(super) fn start_opening(&mut self, path: &std::path::Path) {
        let shown = path.to_string_lossy().to_string();
        if self.opening.iter().any(|o| o.path == shown) { return; }
        let path = path.to_path_buf();
        let (send, document) = channel();
        std::thread::spawn(move || {
            let read = std::fs::read(&path)
                .map_err(|e| OpenError::Unreadable(e.to_string()))
                .and_then(|data| super::file_io::parse_document(&data).map_err(OpenError::Damaged));
            let _ = send.send(read);
        });
        self.opening.push(OpeningFile { path: shown, document });
    }

    pub(super) fn poll_opening(&mut self, ctx: &egui::Context) {
        // In the order they were asked for, so the tabs line up that way; not in the middle of a
        // stroke, which a new tab in front would cut off, nor while the document in front is
        // being saved.
        while let Some(first) = self.opening.first()
            && !self.drawing
            && self.file_job.is_none()
        {
            let read = match first.document.try_recv() {
                Ok(read) => read,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => Err(OpenError::Damaged("the reader stopped".into())),
            };
            let opening = self.opening.remove(0);
            match read {
                Ok(mut doc) => {
                    doc.file_path = Some(opening.path.clone());
                    self.open_in_tab(doc);
                    self.settings.add_recent(&opening.path);
                    self.show_status(format!("Opened: {}", self.doc_name()));
                }
                Err(OpenError::Unreadable(e)) => {
                    self.settings.forget_recent(&opening.path);
                    self.show_status(format!("Failed to read file: {e}"));
                }
                Err(OpenError::Damaged(e)) => self.show_status(format!("Can't open file: {e}")),
            }
        }
        if !self.opening.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Are documents still being read?
    #[cfg(test)]
    pub(crate) fn opening_files(&self) -> bool {
        !self.opening.is_empty()
    }

    /// Reads the picture on another thread; the Import Image dialog opens when it's ready.
    pub fn start_image_import(&mut self, path: &std::path::Path) {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let path = path.to_path_buf();
        let (send, image) = channel();
        std::thread::spawn(move || {
            let read = image::open(&path).map(|image| {
                let image = image.to_rgba8();
                let (w, h) = image.dimensions();
                if w.max(h) > MAX_SOURCE_SIDE {
                    let scale = MAX_SOURCE_SIDE as f32 / w.max(h) as f32;
                    image::imageops::thumbnail(&image, (w as f32 * scale) as u32, (h as f32 * scale) as u32)
                } else {
                    image
                }
            });
            let _ = send.send(read.map_err(|e| e.to_string()));
        });
        self.loading_picture = Some(LoadingPicture { name, image });
    }

    pub(super) fn poll_loading_picture(&mut self, ctx: &egui::Context) {
        let Some(loading) = &self.loading_picture else { return };
        let read = match loading.image.try_recv() {
            Ok(read) => read,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("the reader stopped".into()),
        };
        let loading = self.loading_picture.take().unwrap();
        match read {
            Ok(source) => self.open_image_import(loading.name, source),
            Err(e) => self.show_status(format!("Can't read image: {e}")),
        }
    }

    /// Something the user waits for: a small window in front, so nothing else starts meanwhile.
    pub(super) fn waiting_window(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.file_job {
            if job.started.elapsed() < SHOW_WAITING_AFTER {
                ctx.request_repaint_after(SHOW_WAITING_AFTER);
                return;
            }
            let stopping = job.stop.load(Ordering::Relaxed);
            let ((renders, rendered), (frames, written)) = (job.renders, job.frames);
            let name = job.path.file_name().unwrap_or_default().to_string_lossy();
            let (text, done, total) = if stopping {
                ("Stopping…".to_owned(), rendered + written, renders + frames)
            } else if rendered < renders {
                (format!("Rendering 3D frames  {rendered} / {renders}"), rendered, renders + frames)
            } else {
                match job.writing {
                    Writing::Document => (format!("Saving {name}…"), renders, renders),
                    Writing::Gif if frames > 1 => (format!("Writing GIF  frame {} / {frames}", (written + 1).min(frames)), renders + written, renders + frames),
                    _ => (format!("Writing {name}…"), renders, renders + frames),
                }
            };
            let can_cancel = job.writing != Writing::Document;
            let mut cancel = false;
            Modal::new(Id::new("waiting")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.horizontal(|ui| {
                    ui.add(Spinner::new().size(18.0).color(theme::ACCENT));
                    ui.label(text);
                });
                if total > 0 {
                    ui.add_space(6.0);
                    ui.add(ProgressBar::new(done as f32 / total as f32).desired_height(10.0).fill(theme::ACCENT_STRONG));
                }
                if can_cancel {
                    ui.add_space(6.0);
                    cancel = ui.add_enabled(!stopping, Button::new("Cancel")).clicked();
                }
            });
            if cancel {
                job.stop.store(true, Ordering::Relaxed);
            }
            return;
        }
        let (text, can_skip) = if let Some(loading) = &self.loading_picture {
            (format!("Reading {}…", loading.name), false)
        } else if self.pending_dialog.is_some() {
            ("Waiting for the system file dialog…".to_owned(), true)
        } else {
            return;
        };
        let mut skip = false;
        Modal::new(Id::new("waiting")).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(Spinner::new().size(18.0).color(theme::ACCENT));
                ui.label(text);
            });
            if can_skip {
                ui.add_space(6.0);
                skip = ui.button("Use the built-in chooser").on_hover_text("If the system dialog doesn't show up").clicked();
            }
        });
        if skip
            && let Some(pending) = self.pending_dialog.take()
        {
            self.system_dialog_failed = true;
            self.open_file_browser(pending.action, pending.suggested);
        }
    }

    /// Is something running that the user waits for (the waiting window is up)?
    pub(super) fn waiting(&self) -> bool {
        self.loading_picture.is_some() || self.pending_dialog.is_some() || self.file_job.is_some()
    }
}
