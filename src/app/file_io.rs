//! Saving, opening and exporting files.

use eframe::egui;

use super::{AsciiArtEditor, PendingAction, file_dialog::FileAction, storage};
use crate::document::{Color, Document, codec};
use crate::tools::selection::text_to_content;

impl AsciiArtEditor {
    /// Saves to the current file, or asks where to save it the first time.
    pub(super) fn save(&mut self, ctx: &egui::Context) {
        self.save_then(None, ctx);
    }

    /// Like [`Self::save`], then runs `then` once the document is safely saved.
    pub(super) fn save_then(&mut self, then: Option<PendingAction>, ctx: &egui::Context) {
        match self.doc.file_path.clone() {
            Some(path) => {
                if self.write_file(path)
                    && let Some(then) = then
                {
                    self.perform(then, ctx);
                }
            }
            None => self.choose_file(FileAction::SaveAs { then }, ctx),
        }
    }

    /// Writes next to the target first and then renames, so a failed write can't wipe the old file.
    pub(super) fn write_file(&mut self, path: String) -> bool {
        let (data, compressed) = match document_bytes(&self.doc) {
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
                self.refresh_autosave();
                self.autosaved_version = self.history.version();
                let how = if compressed { "" } else { " (uncompressed)" };
                self.show_status(format!("Saved{how}: {}", self.doc_name()));
                true
            }
            Err(e) => {
                self.show_status(format!("Save failed: {e}"));
                false
            }
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
        if let Some(tab) = self.tab_with_file(&shown) {
            self.switch_tab(tab);
            self.show_status(format!("{} is already open", self.doc_name()));
            return;
        }
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(e) => {
                self.settings.forget_recent(&shown);
                self.show_status(format!("Failed to read file: {e}"));
                return;
            }
        };
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) {
            self.import_text(path, &String::from_utf8_lossy(&data));
            return;
        }
        let mut doc = match parse_document(&data) {
            Ok(doc) => doc,
            Err(e) => {
                self.show_status(format!("Can't open file: {e}"));
                return;
            }
        };
        doc.file_path = Some(shown.clone());
        self.open_in_tab(doc);
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
        self.open_in_tab(doc);
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let clipped = if block.len() > MAX || block.first().is_some_and(|r| r.len() > MAX) { " (clipped)" } else { "" };
        self.show_status(format!("Imported {name}: {width}×{height}{clipped}"));
    }
}

/// A document from the bytes of a `.vc` file: compressed, or JSON as older versions wrote.
pub fn parse_document(bytes: &[u8]) -> Result<Document, String> {
    let mut doc = if codec::is_binary(bytes) {
        codec::decode(bytes)?
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| "not a Void Canvas file".to_owned())?;
        serde_json::from_str::<Document>(text).map_err(|e| e.to_string())?
    };
    doc.validate()?;
    Ok(doc)
}

/// The bytes of `doc` as a `.vc` file, and whether they are compressed. The compressed form
/// is decoded again and compared before it is trusted; were that ever to fail, the document
/// is written as JSON instead, so a bug in the codec can't lose work.
pub fn document_bytes(doc: &Document) -> Result<(Vec<u8>, bool), String> {
    let bytes = codec::encode(doc);
    if codec::decode(&bytes).is_ok_and(|back| codec::same_content(doc, &back)) {
        return Ok((bytes, true));
    }
    serde_json::to_vec_pretty(doc).map(|json| (json, false)).map_err(|e| e.to_string())
}
