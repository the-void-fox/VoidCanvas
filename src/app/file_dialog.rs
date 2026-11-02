//! Choosing files: the system dialog, or a built-in browser when the system one can't open
//! (e.g. on Linux without a working xdg-desktop-portal).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::iter::Peekable;
use std::str::Chars;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, *};

use super::{AsciiArtEditor, PendingAction, background::Writing, import::IMAGE_EXTENSIONS};
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
    /// A 3D model for a 3D layer.
    LoadModel,
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
            FileAction::LoadModel => "Load 3D Model",
        }
    }

    /// Filter name and the extensions shown; the first one is added to new file names.
    pub(super) fn filter(&self) -> (&'static str, &'static [&'static str]) {
        match self {
            FileAction::Open => ("Void Canvas or text", &["vc", "txt"]),
            FileAction::ImportImage => ("Images", IMAGE_EXTENSIONS),
            FileAction::SaveAs { .. } => ("Void Canvas", &["vc"]),
            FileAction::ExportText => ("Text", &["txt"]),
            FileAction::ExportAnsi => ("ANSI", &["ans"]),
            FileAction::ExportHtml => ("HTML", &["html"]),
            FileAction::ExportPng { .. } => ("PNG Image", &["png"]),
            FileAction::ExportGif => ("GIF Animation", &["gif"]),
            FileAction::LoadModel => ("OBJ model", &["obj"]),
        }
    }

    pub(super) fn saves(&self) -> bool {
        !matches!(self, FileAction::Open | FileAction::ImportImage | FileAction::LoadModel)
    }
}

/// A system dialog that returns faster than this was never shown (nobody cancels that quickly).
const FAILED_DIALOG: Duration = Duration::from_millis(300);

impl AsciiArtEditor {
    /// Asks for a file for `action`, then carries it out. The system dialog runs on its own
    /// thread (the window keeps answering); when it can't open, the built-in chooser does.
    pub(super) fn choose_file(&mut self, action: FileAction) {
        if self.pending_dialog.is_some() { return; }
        let suggested = self.suggested_file_name(&action);
        if !self.system_dialog_failed && !cfg!(target_os = "macos") {
            self.open_system_dialog(action, suggested);
            return;
        }
        // macOS wants its dialogs on the main thread.
        if !self.system_dialog_failed {
            let started = Instant::now();
            let (kind, extensions) = action.filter();
            let dialog = rfd::FileDialog::new().add_filter(kind, extensions);
            let picked = if action.saves() { dialog.set_file_name(&suggested).save_file() } else { dialog.pick_file() };
            match picked {
                Some(path) => return self.finish_file_action(action, path),
                None if started.elapsed() >= FAILED_DIALOG => return, // cancelled
                None => {
                    self.system_dialog_failed = true;
                    self.show_status("The system file dialog didn't open, using the built-in one");
                }
            }
        }
        self.open_file_browser(action, suggested);
    }

    pub(super) fn open_file_browser(&mut self, action: FileAction, suggested: String) {
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

    pub(super) fn finish_file_action(&mut self, action: FileAction, mut path: PathBuf) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if action.saves() {
            let ext = action.filter().1[0];
            if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case(ext)) {
                path.set_extension(ext);
            }
        }
        // Images and documents take a while: they are written in the background.
        match action {
            FileAction::ExportPng { transparent } => return self.start_writing(Writing::Png { transparent }, path, None),
            FileAction::ExportGif => return self.start_writing(Writing::Gif, path, None),
            FileAction::SaveAs { then } => return self.start_writing(Writing::Document, path, then),
            _ => {}
        }
        // Text exports read the cells: 3D layers catch up first.
        self.finish_3d_now();
        let text_export = |app: &Self| match action {
            FileAction::ExportText => Some(("Text", export::to_text(&app.doc, export::whole(&app.doc)))),
            FileAction::ExportAnsi => Some(("ANSI", export::to_ansi(&app.doc, export::whole(&app.doc)))),
            FileAction::ExportHtml => Some(("HTML", export::to_html(&app.doc, &app.doc_name()))),
            _ => None,
        };
        if let Some((kind, contents)) = text_export(self) {
            match std::fs::write(&path, contents) {
                Ok(()) => {
                    self.show_status(format!("Exported {kind}: {}", path.display()));
                    self.warn_if_overlaid();
                }
                Err(e) => self.show_status(format!("Export failed: {e}")),
            }
            return;
        }
        match action {
            FileAction::Open => self.open_path(&path),
            FileAction::ImportImage => self.start_image_import(&path),
            FileAction::LoadModel => self.load_model(&path),
            _ => {}
        }
    }

    pub(super) fn file_browser_dialog(&mut self, ctx: &egui::Context) {
        let Some(browser) = &mut self.file_browser else { return };
        let modal = Modal::new(Id::new("file_browser")).show(ctx, |ui| browser.ui(ui));
        match modal.inner {
            Some(Outcome::Chosen(path)) => {
                let browser = self.file_browser.take().unwrap();
                self.finish_file_action(browser.action, path);
            }
            Some(Outcome::Cancelled) => self.file_browser = None,
            None if modal.should_close() => self.file_browser = None,
            None => {}
        }
    }
}

/// Windows writes canonical paths as `\\?\C:\…`; the plain `C:\…` means the same and reads better.
fn readable(path: PathBuf) -> PathBuf {
    if cfg!(windows)
        && let Some(plain) = path.to_str().and_then(|p| p.strip_prefix(r"\\?\"))
        && plain.as_bytes().get(1) == Some(&b':')
    {
        return PathBuf::from(plain);
    }
    path
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

enum Outcome {
    Chosen(PathBuf),
    Cancelled,
}

/// The built-in file chooser: the usual places and a folder tree on the left, the folder's
/// sub-folders and matching files on the right.
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
    /// Home, Desktop, Documents… where they exist: (icon, name, folder).
    places: Vec<(&'static str, &'static str, PathBuf)>,
    /// Where the tree starts: `/`, or the drives on Windows.
    roots: Vec<PathBuf>,
    /// Folders open in the tree.
    open: HashSet<PathBuf>,
    /// Sub-folders of the folders read so far.
    subfolders: HashMap<PathBuf, Vec<PathBuf>>,
    /// The folder changed: the list starts at the top.
    moved: bool,
    /// The folder changed: the tree is to scroll to it.
    reveal: bool,
    /// The tree's scroll offset and height in the last frame.
    tree_view: (f32, f32),
}

/// The chooser's size, and the width of its places-and-tree column.
const BROWSER_SIZE: Vec2 = vec2(760.0, 340.0);
const SIDEBAR_WIDTH: f32 = 220.0;
/// How far each tree level is indented.
const TREE_INDENT: f32 = 12.0;

impl FileBrowser {
    fn new(action: FileAction, dir: PathBuf, name: String) -> Self {
        let focus_name = action.saves();
        let mut browser = Self {
            action,
            dir: PathBuf::new(),
            name,
            entries: Vec::new(),
            error: None,
            path_input: String::new(),
            focus_name,
            places: places(),
            roots: roots(),
            open: HashSet::new(),
            subfolders: HashMap::new(),
            moved: false,
            reveal: false,
            tree_view: (0.0, 0.0),
        };
        browser.enter(dir);
        browser
    }

    /// Re-reads the folder: visible sub-folders and files with a wanted extension.
    pub fn refresh(&mut self) {
        let extensions = self.action.filter().1;
        self.path_input = self.dir.to_string_lossy().to_string();
        self.error = None;
        self.entries = match read_folder(&self.dir) {
            Ok(entries) => {
                let wanted = |name: &str| {
                    Path::new(name).extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
                };
                entries.into_iter().filter(|(name, is_dir)| *is_dir || wanted(name)).collect()
            }
            Err(e) => {
                self.error = Some(format!("Can't read this folder: {e}"));
                Vec::new()
            }
        };
        let subfolders = self.entries.iter().filter(|(_, is_dir)| *is_dir).map(|(name, _)| self.dir.join(name)).collect();
        self.subfolders.insert(self.dir.clone(), subfolders);
    }

    fn enter(&mut self, dir: PathBuf) {
        // Absolute, so the path field is meaningful and "up" always has somewhere to go.
        self.dir = std::fs::canonicalize(&dir).map(readable).unwrap_or(dir);
        self.moved = true;
        self.reveal = true;
        // The tree opens down to the folder.
        let ancestors: Vec<PathBuf> = self.dir.ancestors().skip(1).map(Path::to_path_buf).collect();
        for folder in ancestors {
            self.read_subfolders(&folder);
            self.open.insert(folder);
        }
        if let Some(root) = self.dir.ancestors().last()
            && !self.roots.iter().any(|r| r == root)
        {
            self.roots.push(root.to_path_buf());
        }
        self.refresh();
    }

    /// Reads the sub-folders of `folder` for the tree, unless they are known already.
    fn read_subfolders(&mut self, folder: &Path) {
        if self.subfolders.contains_key(folder) { return; }
        let subfolders = read_folder(folder)
            .map(|entries| entries.into_iter().filter(|(_, is_dir)| *is_dir).map(|(name, _)| folder.join(name)).collect())
            .unwrap_or_default();
        self.subfolders.insert(folder.to_path_buf(), subfolders);
    }

    /// Opens or closes `folder` in the tree.
    fn toggle(&mut self, folder: PathBuf) {
        if !self.open.remove(&folder) {
            self.read_subfolders(&folder);
            self.open.insert(folder);
        }
    }

    /// The tree as shown, top to bottom: (depth, folder), open folders followed by their sub-folders.
    fn tree_rows(&self) -> Vec<(usize, &Path)> {
        let mut rows = Vec::new();
        let mut stack: Vec<(usize, &Path)> = self.roots.iter().rev().map(|r| (0, r.as_path())).collect();
        while let Some((depth, folder)) = stack.pop() {
            rows.push((depth, folder));
            if self.open.contains(folder)
                && let Some(subfolders) = self.subfolders.get(folder)
            {
                stack.extend(subfolders.iter().rev().map(|s| (depth + 1, s.as_path())));
            }
        }
        rows
    }

    /// The file the current name points at.
    fn target(&self) -> PathBuf {
        let name = Path::new(self.name.trim());
        if name.is_absolute() { name.to_path_buf() } else { self.dir.join(name) }
    }

    fn ui(&mut self, ui: &mut Ui) -> Option<Outcome> {
        let mut outcome = None;
        let moved = std::mem::take(&mut self.moved);
        ui.set_width(BROWSER_SIZE.x);
        ui.heading(self.action.title());
        ui.add_space(6.0);

        let mut enter = None;
        ui.horizontal(|ui| {
            let up = self.dir.parent().map(Path::to_path_buf);
            let up_key = ui.input(|i| i.modifiers.alt && i.key_pressed(Key::ArrowUp))
                || (!ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Backspace)));
            if ui.add_enabled(up.is_some(), Button::new(RichText::new(icons::ARROW_UP).font(theme::mono(14.0))))
                .on_hover_text("Parent folder (Backspace)")
                .clicked()
                || up_key
            {
                enter = up;
            }
            let path_edit = ui.add(TextEdit::singleline(&mut self.path_input).desired_width(f32::INFINITY));
            if path_edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let dir = PathBuf::from(self.path_input.trim());
                if dir.is_dir() { enter = Some(dir) } else { self.error = Some("No such folder".into()) }
            }
        });

        // Every row in the tree and the list is this high, so only the rows in view are drawn
        // (a folder may hold thousands of entries).
        let row_height = (ui.text_style_height(&TextStyle::Button) + 2.0 * ui.spacing().button_padding.y).max(ui.spacing().interact_size.y);
        let row_step = row_height + ui.spacing().item_spacing.y;
        let mut toggle = None;
        ui.allocate_ui_with_layout(BROWSER_SIZE, Layout::left_to_right(Align::Min), |ui| {
            let pane = |ui: &mut Ui, width: f32, contents: &mut dyn FnMut(&mut Ui)| {
                Frame::new().fill(theme::BG_DEEP).corner_radius(6.0).inner_margin(4.0).show(ui, |ui| {
                    ui.set_width(width - 8.0);
                    ui.set_height(BROWSER_SIZE.y - 8.0);
                    ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                    ui.vertical(contents);
                });
            };

            pane(ui, SIDEBAR_WIDTH, &mut |ui| {
                for (icon, place, folder) in &self.places {
                    let button = Button::selectable(*folder == self.dir, format!("{icon}  {place}"))
                        .frame_when_inactive(false)
                        .min_size(vec2(ui.available_width(), row_height));
                    if ui.add(button).on_hover_text(folder.to_string_lossy()).clicked() {
                        enter = Some(folder.clone());
                    }
                }
                if !self.places.is_empty() {
                    ui.separator();
                }

                let rows = self.tree_rows();
                let mut scroll = ScrollArea::vertical().id_salt("folder_tree").auto_shrink([false, false]);
                // Bring the current folder into view, once the tree's height is known.
                let (top, height) = self.tree_view;
                let revealed = self.reveal && height > 0.0;
                if revealed && let Some(row) = rows.iter().position(|(_, folder)| *folder == self.dir) {
                    let y = row as f32 * row_step;
                    if y < top || y + row_step > top + height {
                        scroll = scroll.vertical_scroll_offset((y - height / 2.0).max(0.0));
                    }
                }
                let shown = scroll.show_rows(ui, row_height, rows.len(), |ui, range| {
                    for &(depth, folder) in &rows[range] {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            ui.add_space(depth as f32 * TREE_INDENT);
                            let open = self.open.contains(folder);
                            // Folders not read yet may have sub-folders.
                            let has_subfolders = self.subfolders.get(folder).is_none_or(|s| !s.is_empty());
                            let arrow = Button::new(RichText::new(if open { icons::UNFOLDED } else { icons::FOLDED }).color(theme::TEXT_DIM))
                                .frame(false)
                                .min_size(vec2(14.0, row_height));
                            if ui.add_visible(has_subfolders, arrow).clicked() {
                                toggle = Some(folder.to_path_buf());
                            }
                            let name = folder.file_name().map_or_else(|| folder.to_string_lossy(), |n| n.to_string_lossy());
                            let icon = if depth == 0 { icons::DRIVE } else { icons::FOLDER };
                            let button = Button::selectable(folder == self.dir, format!("{icon}  {name}"))
                                .frame_when_inactive(false)
                                .min_size(vec2(ui.available_width(), row_height));
                            // Names start on the left, as in the list.
                            let resp = ui.with_layout(Layout::top_down(Align::Min), |ui| ui.add(button)).inner;
                            if resp.clicked() {
                                enter = Some(folder.to_path_buf());
                            }
                            if resp.double_clicked() {
                                toggle = Some(folder.to_path_buf());
                            }
                        });
                    }
                });
                self.tree_view = (shown.state.offset.y, shown.inner_rect.height());
                self.reveal &= !revealed;
            });

            pane(ui, ui.available_width(), &mut |ui| {
                let mut list = ScrollArea::vertical().id_salt("file_list").auto_shrink([false, false]);
                if moved {
                    list = list.vertical_scroll_offset(0.0);
                }
                list.show_rows(ui, row_height, self.entries.len().max(1), |ui, range| {
                    if self.entries.is_empty() {
                        ui.label(RichText::new("No matching files here").color(theme::TEXT_FAINT));
                    }
                    for (name, is_dir) in self.entries.get(range).unwrap_or_default() {
                        let selected = !is_dir && self.name == *name;
                        let label = format!("{}  {name}", if *is_dir { icons::FOLDER } else { icons::FILE });
                        let button = Button::selectable(selected, label)
                            .frame_when_inactive(false)
                            .min_size(vec2(ui.available_width(), row_height));
                        let resp = ui.add(button);
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
        });
        if let Some(folder) = toggle {
            self.toggle(folder);
        }
        if let Some(dir) = enter {
            self.enter(dir);
        }

        ui.add_space(6.0);
        let mut confirm = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            let name_edit = ui.add(TextEdit::singleline(&mut self.name).desired_width(BROWSER_SIZE.x - 200.0));
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

/// The visible entries of a folder, (name, is a folder): folders first, each part by name.
fn read_folder(dir: &Path) -> std::io::Result<Vec<(String, bool)>> {
    let mut entries: Vec<(String, bool)> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { return None; }
            // Only links need a look at what they point to.
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir() || (t.is_symlink() && entry.path().is_dir()));
            Some((name, is_dir))
        })
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| natural_order(&a.0, &b.0)));
    Ok(entries)
}

/// Names in the order people read them: case aside, and "art2" before "art10".
pub(crate) fn natural_order(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    let number = |chars: &mut Peekable<Chars>| {
        let mut digits = String::new();
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits.push(c);
        }
        digits
    };
    loop {
        let order = match (a.peek(), b.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (x, y) = (number(&mut a), number(&mut b));
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                x.len().cmp(&y.len()).then_with(|| x.cmp(y))
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                a.next();
                b.next();
                order
            }
        };
        if order.is_ne() {
            return order;
        }
    }
}

/// Home and the usual folders in it, where they exist.
fn places() -> Vec<(&'static str, &'static str, PathBuf)> {
    let Some(home) = home_dir().and_then(|h| std::fs::canonicalize(h).ok()).map(readable) else { return Vec::new() };
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    let named = std::fs::read_to_string(config.join("user-dirs.dirs")).map(|text| user_dirs(&text, &home)).unwrap_or_default();
    let mut places = vec![(icons::HOME, "Home", home.clone())];
    for (icon, place, key) in [
        (icons::DESKTOP, "Desktop", "DESKTOP"),
        (icons::DOCUMENTS, "Documents", "DOCUMENTS"),
        (icons::DOWNLOADS, "Downloads", "DOWNLOAD"),
        (icons::PICTURES, "Pictures", "PICTURES"),
    ] {
        let folder = named.get(key).cloned().unwrap_or_else(|| home.join(place));
        if folder != home
            && let Ok(folder) = std::fs::canonicalize(folder)
            && folder.is_dir()
        {
            places.push((icon, place, readable(folder)));
        }
    }
    places
}

/// Linux names the usual folders in the user's language (`~/Загрузки`); `user-dirs.dirs` says
/// which is which: `XDG_DOWNLOAD_DIR="$HOME/Загрузки"`.
pub(crate) fn user_dirs(text: &str, home: &Path) -> HashMap<String, PathBuf> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            let key = key.strip_prefix("XDG_")?.strip_suffix("_DIR")?;
            let value = value.trim().trim_matches('"');
            let folder = match value.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None if value.starts_with('/') => PathBuf::from(value),
                None => return None,
            };
            Some((key.to_owned(), folder))
        })
        .collect()
}

/// Where the folder tree starts: `/`, or every drive on Windows.
fn roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        (b'C'..=b'Z').map(|drive| PathBuf::from(format!("{}:\\", drive as char))).filter(|d| d.is_dir()).collect()
    } else {
        vec![PathBuf::from("/")]
    }
}
