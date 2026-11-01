//! Choosing files: the system dialog, or a built-in browser when the system one can't open
//! (e.g. on Linux without a working xdg-desktop-portal).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, *};

use super::{AsciiArtEditor, PendingAction, import::IMAGE_EXTENSIONS};
use crate::{export, icons, theme};

/// What a chosen file is for.
#[derive(Clone, PartialEq)]
pub enum FileAction {
    Open,
    ImportImage,
    /// Save the document; `then` runs once it has been saved.
    SaveAs { then: Option<PendingAction> },
    ExportText,
    ExportAnsi,
    ExportHtml,
    ExportPng { transparent: bool },
    ExportGif,
}

impl FileAction {
    fn title(&self) -> &'static str {
        match self {
            FileAction::Open => "Open",
            FileAction::ImportImage => "Import Image",
            FileAction::SaveAs { .. } => "Save As",
            FileAction::ExportText => "Export Text",
            FileAction::ExportAnsi => "Export ANSI",
            FileAction::ExportHtml => "Export HTML",
            FileAction::ExportPng { .. } => "Export PNG",
            FileAction::ExportGif => "Export GIF",
        }
    }

    /// Filter name and the extensions shown; the first one is added to new file names.
    fn filter(&self) -> (&'static str, &'static [&'static str]) {
        match self {
            FileAction::Open => ("Void Canvas or text", &["vc", "txt"]),
            FileAction::ImportImage => ("Images", IMAGE_EXTENSIONS),
            FileAction::SaveAs { .. } => ("Void Canvas", &["vc"]),
            FileAction::ExportText => ("Text", &["txt"]),
            FileAction::ExportAnsi => ("ANSI", &["ans"]),
            FileAction::ExportHtml => ("HTML", &["html"]),
            FileAction::ExportPng { .. } => ("PNG Image", &["png"]),
            FileAction::ExportGif => ("GIF Animation", &["gif"]),
        }
    }

    fn saves(&self) -> bool {
        !matches!(self, FileAction::Open | FileAction::ImportImage)
    }
}

/// A system dialog that returns faster than this was never shown (nobody cancels that quickly).
const FAILED_DIALOG: Duration = Duration::from_millis(300);

impl AsciiArtEditor {
    /// Asks for a file for `action`, then carries it out.
    pub(super) fn choose_file(&mut self, action: FileAction, ctx: &egui::Context) {
        let suggested = self.suggested_file_name(&action);
        if !self.system_dialog_failed {
            let started = Instant::now();
            let (kind, extensions) = action.filter();
            let dialog = rfd::FileDialog::new().add_filter(kind, extensions);
            let picked = if action.saves() { dialog.set_file_name(&suggested).save_file() } else { dialog.pick_file() };
            match picked {
                Some(path) => return self.finish_file_action(action, path, ctx),
                None if started.elapsed() >= FAILED_DIALOG => return, // cancelled
                None => {
                    self.system_dialog_failed = true;
                    self.show_status("The system file dialog didn't open, using the built-in one");
                }
            }
        }
        let doc_dir = self.doc.file_path.as_deref().and_then(|p| Path::new(p).parent()).map(Path::to_path_buf);
        let dir = [self.last_dir.clone(), doc_dir, home_dir()]
            .into_iter()
            .flatten()
            .find(|d| d.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.file_browser = Some(FileBrowser::new(action, dir, suggested));
    }

    fn suggested_file_name(&self, action: &FileAction) -> String {
        if !action.saves() { return String::new(); }
        let stem = Path::new(&self.doc_name()).file_stem().unwrap_or_default().to_string_lossy().to_string();
        format!("{stem}.{}", action.filter().1[0])
    }

    fn finish_file_action(&mut self, action: FileAction, mut path: PathBuf, ctx: &egui::Context) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if action.saves() {
            let ext = action.filter().1[0];
            if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case(ext)) {
                path.set_extension(ext);
            }
        }
        let text_export = |app: &Self| match action {
            FileAction::ExportText => Some(("Text", export::to_text(&app.doc, export::whole(&app.doc)))),
            FileAction::ExportAnsi => Some(("ANSI", export::to_ansi(&app.doc, export::whole(&app.doc)))),
            FileAction::ExportHtml => Some(("HTML", export::to_html(&app.doc, &app.doc_name()))),
            _ => None,
        };
        if let Some((kind, contents)) = text_export(self) {
            match std::fs::write(&path, contents) {
                Ok(()) => self.show_status(format!("Exported {kind}: {}", path.display())),
                Err(e) => self.show_status(format!("Export failed: {e}")),
            }
            return;
        }
        match action {
            FileAction::Open => self.open_path(&path),
            FileAction::ImportImage => self.start_image_import(&path),
            FileAction::SaveAs { then } => {
                if self.write_file(path.to_string_lossy().to_string())
                    && let Some(then) = then
                {
                    self.perform(then, ctx);
                }
            }
            FileAction::ExportPng { transparent } => {
                let result = export::render_png(&self.doc, transparent)
                    .and_then(|img| img.save(&path).map_err(|e| e.to_string()));
                match result {
                    Ok(()) => self.show_status(format!("Exported PNG: {}", path.display())),
                    Err(e) => self.show_status(format!("PNG export failed: {e}")),
                }
            }
            FileAction::ExportGif => match export::write_gif(&self.doc, &path) {
                Ok(()) => self.show_status(format!("Exported GIF: {}", path.display())),
                Err(e) => self.show_status(format!("GIF export failed: {e}")),
            },
            _ => {}
        }
    }

    pub(super) fn file_browser_dialog(&mut self, ctx: &egui::Context) {
        let Some(browser) = &mut self.file_browser else { return };
        let modal = Modal::new(Id::new("file_browser")).show(ctx, |ui| browser.ui(ui));
        match modal.inner {
            Some(Outcome::Chosen(path)) => {
                let browser = self.file_browser.take().unwrap();
                self.finish_file_action(browser.action, path, ctx);
            }
            Some(Outcome::Cancelled) => self.file_browser = None,
            None if modal.should_close() => self.file_browser = None,
            None => {}
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

enum Outcome {
    Chosen(PathBuf),
    Cancelled,
}

/// The built-in file chooser.
pub struct FileBrowser {
    pub action: FileAction,
    pub dir: PathBuf,
    /// File name (or a path, absolute or relative to `dir`).
    pub name: String,
    /// Folders first, then matching files.
    entries: Vec<(String, bool)>,
    error: Option<String>,
    /// The folder path field, while it is being edited.
    path_input: String,
    /// Put the cursor into the name field on the first frame.
    focus_name: bool,
}

impl FileBrowser {
    fn new(action: FileAction, dir: PathBuf, name: String) -> Self {
        let focus_name = action.saves();
        let mut browser = Self { action, dir: PathBuf::new(), name, entries: Vec::new(), error: None, path_input: String::new(), focus_name };
        browser.enter(dir);
        browser
    }

    /// Re-reads the folder: visible sub-folders and files with a wanted extension.
    pub fn refresh(&mut self) {
        let extensions = self.action.filter().1;
        self.path_input = self.dir.to_string_lossy().to_string();
        self.error = None;
        self.entries = match std::fs::read_dir(&self.dir) {
            Ok(read) => {
                let mut entries: Vec<(String, bool)> = read
                    .filter_map(Result::ok)
                    .filter_map(|entry| {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') { return None; }
                        let is_dir = entry.path().is_dir();
                        let wanted = is_dir
                            || Path::new(&name)
                                .extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)));
                        wanted.then_some((name, is_dir))
                    })
                    .collect();
                entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
                entries
            }
            Err(e) => {
                self.error = Some(format!("Can't read this folder: {e}"));
                Vec::new()
            }
        };
    }

    fn enter(&mut self, dir: PathBuf) {
        // Absolute, so the path field is meaningful and "up" always has somewhere to go.
        self.dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        self.refresh();
    }

    /// The file the current name points at.
    fn target(&self) -> PathBuf {
        let name = Path::new(self.name.trim());
        if name.is_absolute() { name.to_path_buf() } else { self.dir.join(name) }
    }

    fn ui(&mut self, ui: &mut Ui) -> Option<Outcome> {
        let mut outcome = None;
        ui.set_width(520.0);
        ui.heading(self.action.title());
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let up = self.dir.parent().map(Path::to_path_buf);
            if ui.add_enabled(up.is_some(), Button::new(RichText::new(icons::ARROW_UP).font(theme::mono(14.0))))
                .on_hover_text("Parent folder")
                .clicked()
            {
                self.enter(up.unwrap());
            }
            let path_edit = ui.add(TextEdit::singleline(&mut self.path_input).desired_width(f32::INFINITY));
            if path_edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let dir = PathBuf::from(self.path_input.trim());
                if dir.is_dir() { self.enter(dir) } else { self.error = Some("No such folder".into()) }
            }
        });

        let mut enter = None;
        Frame::new().fill(theme::BG_DEEP).corner_radius(6.0).inner_margin(4.0).show(ui, |ui| {
            // A scroll position per folder, so a new folder starts at the top.
            let scroll = ScrollArea::vertical().id_salt(&self.dir).auto_shrink([false, false]);
            scroll.max_height(300.0).min_scrolled_height(300.0).show(ui, |ui| {
                if self.entries.is_empty() {
                    ui.label(RichText::new("No matching files here").color(theme::TEXT_FAINT));
                }
                for (name, is_dir) in &self.entries {
                    let selected = !is_dir && self.name == *name;
                    let label = format!("{}  {name}", if *is_dir { icons::FOLDER } else { icons::FILE });
                    let resp = ui.add(Button::selectable(selected, label).frame_when_inactive(false));
                    if *is_dir {
                        if resp.clicked() {
                            enter = Some(self.dir.join(name));
                        }
                    } else {
                        if resp.clicked() {
                            self.name = name.clone();
                        }
                        if resp.double_clicked() {
                            outcome = Some(Outcome::Chosen(self.dir.join(name)));
                        }
                    }
                }
            });
        });
        if let Some(dir) = enter {
            self.enter(dir);
        }

        ui.add_space(6.0);
        let mut confirm = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            let name_edit = ui.add(TextEdit::singleline(&mut self.name).desired_width(300.0));
            if std::mem::take(&mut self.focus_name) {
                name_edit.request_focus();
            }
            confirm |= name_edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let verb = if self.action.saves() { "Save" } else { "Open" };
            confirm |= ui.button(verb).clicked();
            if ui.button("Cancel").clicked() {
                outcome = Some(Outcome::Cancelled);
            }
        });

        let target = self.target();
        if self.action.saves() && target.is_file() {
            ui.label(RichText::new(format!("{} will be replaced", target.file_name().unwrap_or_default().to_string_lossy())).color(theme::WARNING));
        }
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).color(theme::WARNING));
        }

        if confirm {
            if self.name.trim().is_empty() {
                self.error = Some("Type a file name".into());
            } else if target.is_dir() {
                self.enter(target);
                self.name.clear();
            } else if !self.action.saves() && !target.is_file() {
                self.error = Some("No such file".into());
            } else {
                outcome = Some(Outcome::Chosen(target));
            }
        }
        outcome
    }
}
