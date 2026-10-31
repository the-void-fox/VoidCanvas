//! Saving, opening and exporting files.

use super::AsciiArtEditor;
use crate::document::Document;

impl AsciiArtEditor {
    pub(super) fn export_text(&mut self) {
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
        let tmp = format!("{path}.tmp");
        let result = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &path));
        match result {
            Ok(()) => {
                self.doc.file_path = Some(path);
                self.doc.modified = false;
                self.show_status(format!("Saved: {}", self.doc_name()));
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.show_status(format!("Save failed: {e}"));
                false
            }
        }
    }

    pub(super) fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Void Canvas", &["vc"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    /// Loads a `.vc` file, replacing the current document (reports problems in the status bar).
    pub fn open_path(&mut self, path: &std::path::Path) {
        let data = match std::fs::read_to_string(path) {
            Ok(data) => data,
            Err(e) => {
                self.show_status(format!("Failed to read file: {e}"));
                return;
            }
        };
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
        doc.file_path = Some(path.to_string_lossy().to_string());
        self.load_document(doc);
        self.show_status(format!("Opened: {}", self.doc_name()));
    }
}
