//! Saving, opening and exporting files.

use super::{AsciiArtEditor, storage};
use crate::export;
use crate::document::{Color, Document};
use crate::tools::selection::text_to_content;

impl AsciiArtEditor {
    /// Asks where to save `contents` as a `kind` file with extension `ext`.
    fn export_string(&mut self, kind: &str, ext: &str, contents: String) {
        let Some(mut path) = rfd::FileDialog::new().add_filter(kind, &[ext]).save_file() else { return };
        if path.extension().is_none_or(|e| e != ext) {
            path.set_extension(ext);
        }
        match std::fs::write(&path, contents) {
            Ok(()) => self.show_status(format!("Exported {kind}: {}", path.display())),
            Err(e) => self.show_status(format!("Export failed: {e}")),
        }
    }

    pub(super) fn export_text(&mut self) {
        let text = export::to_text(&self.doc, export::whole(&self.doc));
        self.export_string("Text", "txt", text);
    }

    pub(super) fn export_ansi(&mut self) {
        let ansi = export::to_ansi(&self.doc, export::whole(&self.doc));
        self.export_string("ANSI", "ans", ansi);
    }

    pub(super) fn export_html(&mut self) {
        let html = export::to_html(&self.doc, &self.doc_name());
        self.export_string("HTML", "html", html);
    }

    pub(super) fn export_png(&mut self, transparent: bool) {
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("PNG Image", &["png"])
            .save_file()
        else {
            return;
        };
        if path.extension().is_none_or(|ext| ext != "png") {
            path.set_extension("png");
        }
        let result = crate::export::render_png(&self.doc, transparent)
            .and_then(|img| img.save(&path).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.show_status(format!("Exported PNG: {}", path.display())),
            Err(e) => self.show_status(format!("PNG export failed: {e}")),
        }
    }

    /// Saves to the current file (asks for a path the first time). Returns `true` on success.
    pub(super) fn save(&mut self) -> bool {
        match self.doc.file_path.clone() {
            Some(path) => self.write_file(path),
            None => self.save_as(),
        }
    }

    pub(super) fn save_as(&mut self) -> bool {
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
    pub(super) fn write_file(&mut self, path: String) -> bool {
        let data = match serde_json::to_string_pretty(&self.doc) {
            Ok(data) => data,
            Err(e) => {
                self.show_status(format!("Save failed: {e}"));
                return false;
            }
        };
        match storage::write_atomically(std::path::Path::new(&path), &data) {
            Ok(()) => {
                self.settings.add_recent(&path);
                self.doc.file_path = Some(path);
                self.doc.modified = false;
                storage::Autosave::remove();
                self.autosaved_version = self.history.version();
                self.show_status(format!("Saved: {}", self.doc_name()));
                true
            }
            Err(e) => {
                self.show_status(format!("Save failed: {e}"));
                false
            }
        }
    }

    pub(super) fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Void Canvas or text", &["vc", "txt"])
            .add_filter("Void Canvas", &["vc"])
            .add_filter("Text", &["txt"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    /// Loads a `.vc` file (or imports a `.txt` one), replacing the current document.
    /// Problems are reported in the status bar.
    pub fn open_path(&mut self, path: &std::path::Path) {
        if super::import::is_image(path) {
            self.start_image_import(path);
            return;
        }
        let shown = path.to_string_lossy().to_string();
        let data = match std::fs::read_to_string(path) {
            Ok(data) => data,
            Err(e) => {
                self.settings.forget_recent(&shown);
                self.show_status(format!("Failed to read file: {e}"));
                return;
            }
        };
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) {
            self.import_text(path, &data);
            return;
        }
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
        doc.file_path = Some(shown.clone());
        self.load_document(doc);
        self.settings.add_recent(&shown);
        self.show_status(format!("Opened: {}", self.doc_name()));
    }

    /// A text file becomes a new document just big enough for it.
    fn import_text(&mut self, path: &std::path::Path, text: &str) {
        const MAX: usize = 500;
        let block = text_to_content(text, &Color::WHITE, &Color::TRANSPARENT);
        let width = block.first().map_or(0, |row| row.len()).clamp(1, MAX);
        let height = block.len().clamp(1, MAX);
        let mut doc = Document::new(width, height);
        doc.layers[0].name = "Imported".into();
        for (y, row) in block.iter().take(height).enumerate() {
            for (x, cell) in row.iter().take(width).enumerate() {
                doc.layers[0].set(x, y, cell.clone());
            }
        }
        doc.modified = true;
        self.load_document(doc);
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let clipped = if block.len() > MAX || block.first().is_some_and(|r| r.len() > MAX) { " (clipped)" } else { "" };
        self.show_status(format!("Imported {name}: {width}×{height}{clipped}"));
    }
}
