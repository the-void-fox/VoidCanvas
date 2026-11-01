//! Things kept between runs: settings (recent files) and the crash-recovery autosave.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::document::Document;

const MAX_RECENT: usize = 10;

/// `$XDG_CONFIG_HOME/void-canvas` (usually `~/.config/void-canvas`) or `%APPDATA%\void-canvas`.
/// Tests never touch the real one.
fn config_dir() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };
    base.map(|dir| dir.join("void-canvas"))
}

/// Writes next to the target first and then renames, so a failed write can't wipe the old file.
pub fn write_atomically(path: &Path, data: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, path)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    /// Most recent first.
    #[serde(default)]
    pub recent_files: Vec<String>,
    /// Command palette entries, most recently run first.
    #[serde(default)]
    pub recent_commands: Vec<String>,
    /// The script last run in the Script window.
    #[serde(default)]
    pub script: String,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        config_dir().map(|dir| dir.join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        if let (Some(path), Ok(data)) = (Self::path(), serde_json::to_string_pretty(self)) {
            let _ = write_atomically(&path, &data);
        }
    }

    pub fn add_recent(&mut self, path: &str) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_owned());
        self.recent_files.truncate(MAX_RECENT);
        self.save();
    }

    pub fn forget_recent(&mut self, path: &str) {
        self.recent_files.retain(|p| p != path);
        self.save();
    }

    pub fn add_recent_command(&mut self, key: String, max: usize) {
        self.recent_commands.retain(|k| *k != key);
        self.recent_commands.insert(0, key);
        self.recent_commands.truncate(max);
        self.save();
    }

    pub fn set_script(&mut self, code: &str) {
        if self.script != code {
            self.script = code.to_owned();
            self.save();
        }
    }

    pub fn clear_recent(&mut self) {
        self.recent_files.clear();
        self.save();
    }
}

/// One document kept by the autosave.
#[derive(Serialize, Deserialize)]
pub struct SavedDocument {
    /// The file the document belongs to, if it was ever saved.
    pub file_path: Option<String>,
    pub document: Document,
}

/// Unsaved work of every open document, written in the background and offered back after a crash.
#[derive(Serialize, Deserialize)]
pub struct Autosave {
    pub documents: Vec<SavedDocument>,
    /// How long ago it was written (filled in when loading).
    #[serde(skip)]
    pub age: Option<Duration>,
}

impl Autosave {
    fn path() -> Option<PathBuf> {
        config_dir().map(|dir| dir.join("autosave.vc"))
    }

    /// Keeps `docs`, or removes the recovery file when there is nothing unsaved.
    pub fn write(docs: &[&Document]) {
        if docs.is_empty() {
            return Self::remove();
        }
        let documents = docs
            .iter()
            .map(|doc| SavedDocument { file_path: doc.file_path.clone(), document: (*doc).clone() })
            .collect();
        let autosave = Autosave { documents, age: None };
        if let (Some(path), Ok(data)) = (Self::path(), serde_json::to_string(&autosave)) {
            let _ = write_atomically(&path, &data);
        }
    }

    /// The autosave left by a run that didn't exit cleanly.
    pub fn load() -> Option<Self> {
        let path = Self::path()?;
        let data = std::fs::read_to_string(&path).ok()?;
        // Before tabs, the file held a single document.
        let mut autosave = serde_json::from_str::<Autosave>(&data).ok().or_else(|| {
            let single = serde_json::from_str::<SavedDocument>(&data).ok()?;
            Some(Autosave { documents: vec![single], age: None })
        })?;
        autosave.documents.retain_mut(|saved| saved.document.validate().is_ok());
        if autosave.documents.is_empty() {
            return None;
        }
        autosave.age = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok());
        Some(autosave)
    }

    pub fn remove() {
        if let Some(path) = Self::path() {
            let _ = std::fs::remove_file(path);
        }
    }
}
