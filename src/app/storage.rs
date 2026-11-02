//! Things kept between runs: settings (recent files) and the crash-recovery autosave.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::library::{Palette, Ramp, builtin_palettes, builtin_ramps};

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
pub fn write_atomically(path: &Path, data: impl AsRef<[u8]>) -> std::io::Result<()> {
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
    /// The user's own glyph sets.
    #[serde(default)]
    pub glyph_sets: Vec<GlyphSet>,
    /// Brushes the user saved.
    #[serde(default)]
    pub brushes: Vec<crate::tools::brush::BrushPreset>,
    /// The user's own density ramps.
    #[serde(default)]
    pub ramps: Vec<Ramp>,
    /// The user's own colour palettes.
    #[serde(default)]
    pub palettes: Vec<Palette>,
}

/// A named set of glyphs made by the user, shown in the Glyphs panel after the built-in ones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlyphSet {
    pub name: String,
    pub glyphs: String,
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

    /// Changes the user's glyph sets and keeps them.
    pub fn edit_glyph_sets(&mut self, change: impl FnOnce(&mut Vec<GlyphSet>)) {
        change(&mut self.glyph_sets);
        self.save();
    }

    /// Every density ramp: the built-in ones, then the user's.
    pub fn all_ramps(&self) -> Vec<Ramp> {
        builtin_ramps().into_iter().chain(self.ramps.iter().cloned()).collect()
    }

    /// Every colour palette: the built-in ones, then the user's.
    pub fn all_palettes(&self) -> Vec<Palette> {
        builtin_palettes().into_iter().chain(self.palettes.iter().cloned()).collect()
    }

    /// Every brush: the built-in ones, then the user's.
    pub fn all_brushes(&self) -> Vec<crate::tools::brush::BrushPreset> {
        crate::tools::brush::builtin_presets().into_iter().chain(self.brushes.iter().cloned()).collect()
    }

    pub fn edit_ramps(&mut self, change: impl FnOnce(&mut Vec<Ramp>)) {
        change(&mut self.ramps);
        self.save();
    }

    pub fn edit_palettes(&mut self, change: impl FnOnce(&mut Vec<Palette>)) {
        change(&mut self.palettes);
        self.save();
    }

    /// Changes the user's brushes and keeps them.
    pub fn edit_brushes(&mut self, change: impl FnOnce(&mut Vec<crate::tools::brush::BrushPreset>)) {
        change(&mut self.brushes);
        self.save();
    }

    pub fn clear_recent(&mut self) {
        self.recent_files.clear();
        self.save();
    }
}

/// Writes the recovery file on a thread of its own, so a big animation doesn't stop the editor
/// every time. Versions are written in turn; one still waiting when a newer one comes is skipped.
#[derive(Default)]
pub struct AutosaveWriter {
    worker: Option<(Sender<Vec<SavedDocument>>, JoinHandle<()>)>,
}

impl AutosaveWriter {
    /// Keeps `docs` (copies of the unsaved documents), or removes the recovery file if there are none.
    pub fn keep(&mut self, docs: Vec<SavedDocument>) {
        let (versions, _) = self.worker.get_or_insert_with(|| {
            let (send, versions) = channel::<Vec<SavedDocument>>();
            let thread = std::thread::spawn(move || {
                while let Ok(mut docs) = versions.recv() {
                    while let Ok(newer) = versions.try_recv() {
                        docs = newer;
                    }
                    Autosave::write(&docs);
                }
            });
            (send, thread)
        });
        let _ = versions.send(docs);
    }

    /// A clean exit: waits for the writes under way, then removes the recovery file.
    pub fn finish_and_remove(&mut self) {
        if let Some((versions, thread)) = self.worker.take() {
            drop(versions);
            let _ = thread.join();
        }
        Autosave::remove();
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

/// Recovery files start with this; older ones were JSON (and still load).
const AUTOSAVE_MAGIC: &[u8; 8] = b"VCAUTOSV";

impl Autosave {
    fn path() -> Option<PathBuf> {
        config_dir().map(|dir| dir.join("autosave.vc"))
    }

    /// Keeps `docs`, or removes the recovery file when there is nothing unsaved.
    fn write(docs: &[SavedDocument]) {
        if docs.is_empty() {
            return Self::remove();
        }
        if let Some(path) = Self::path() {
            let _ = write_atomically(&path, Self::to_bytes(docs));
        }
    }

    /// `VCAUTOSV`, the number of documents, then each one's path (empty if it has none) and
    /// its bytes as a `.vc` file, both after their length (4 bytes, little-endian).
    pub(crate) fn to_bytes(docs: &[SavedDocument]) -> Vec<u8> {
        let mut out = AUTOSAVE_MAGIC.to_vec();
        let part = |out: &mut Vec<u8>, bytes: &[u8]| {
            out.extend((bytes.len() as u32).to_le_bytes());
            out.extend(bytes);
        };
        out.extend((docs.len() as u32).to_le_bytes());
        for saved in docs {
            part(&mut out, saved.file_path.as_deref().unwrap_or("").as_bytes());
            part(&mut out, &super::file_io::document_bytes(&saved.document).map(|(bytes, _)| bytes).unwrap_or_default());
        }
        out
    }

    /// The documents in a recovery file; a damaged one is left out, the others are kept.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Vec<SavedDocument>> {
        if !bytes.starts_with(AUTOSAVE_MAGIC) {
            // Before 2.1 the file was JSON; before tabs, it held a single document.
            let text = std::str::from_utf8(bytes).ok()?;
            let mut documents = serde_json::from_str::<Autosave>(text)
                .map(|autosave| autosave.documents)
                .ok()
                .or_else(|| Some(vec![serde_json::from_str::<SavedDocument>(text).ok()?]))?;
            documents.retain_mut(|saved| saved.document.validate().is_ok());
            return Some(documents);
        }
        fn take<'a>(rest: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
            let (part, after) = rest.split_at_checked(n)?;
            *rest = after;
            Some(part)
        }
        fn part<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
            let len = u32::from_le_bytes(take(rest, 4)?.try_into().ok()?);
            take(rest, len as usize)
        }
        let mut rest = bytes.strip_prefix(AUTOSAVE_MAGIC.as_slice())?;
        let count = u32::from_le_bytes(take(&mut rest, 4)?.try_into().ok()?);
        let mut docs = Vec::new();
        for _ in 0..count {
            let path = String::from_utf8(part(&mut rest)?.to_vec()).ok()?;
            if let Ok(document) = super::file_io::parse_document(part(&mut rest)?) {
                docs.push(SavedDocument { file_path: (!path.is_empty()).then_some(path), document });
            }
        }
        Some(docs)
    }

    /// The autosave left by a run that didn't exit cleanly.
    pub fn load() -> Option<Self> {
        let path = Self::path()?;
        let data = std::fs::read(&path).ok()?;
        let documents = Self::from_bytes(&data)?;
        if documents.is_empty() {
            return None;
        }
        let age = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok());
        Some(Autosave { documents, age })
    }

    pub fn remove() {
        if let Some(path) = Self::path() {
            let _ = std::fs::remove_file(path);
        }
    }
}
