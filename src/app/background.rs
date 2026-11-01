//! Work done off the UI thread, so the window keeps answering: rendering 3D layers, the system
//! file dialog, and reading pictures. A small window shows what is going on meanwhile.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, *};
use image::RgbaImage;

use super::AsciiArtEditor;
use super::file_dialog::FileAction;
use crate::document::Cell;
use crate::theme;

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

/// A picture being read for Import Image.
pub struct LoadingPicture {
    name: String,
    image: Receiver<Result<RgbaImage, String>>,
}

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
        if self.render_job.is_none() {
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
            Some(path) => self.finish_file_action(pending.action, path, ctx),
            None if pending.started.elapsed() >= FAILED_DIALOG => {} // cancelled
            None => {
                self.system_dialog_failed = true;
                self.show_status("The system file dialog didn't open, using the built-in one");
                self.open_file_browser(pending.action, pending.suggested);
            }
        }
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
        self.loading_picture.is_some() || self.pending_dialog.is_some()
    }
}
