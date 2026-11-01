use serde::{Deserialize, Serialize};
use super::{Changes, cell::Cell};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    /// The cells of the frame being shown.
    cells: Vec<Cell>,
    /// In an animation, the cells of every frame by frame number; the shown frame's entry is
    /// empty, because its cells are `cells`. Empty when there is a single frame.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    frames: Vec<Vec<Cell>>,
}

impl Layer {
    pub fn new(width: usize, height: usize, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            visible: true,
            locked: false,
            opacity: 1.0,
            cells: vec![Cell::default(); width * height],
            frames: Vec::new(),
        }
    }

    /// A layer holding `frames` (the cells of every animation frame), with `shown` in front.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_frames(
        name: String,
        width: usize,
        height: usize,
        visible: bool,
        locked: bool,
        opacity: f32,
        mut frames: Vec<Vec<Cell>>,
        shown: usize,
    ) -> Self {
        let cells = std::mem::take(&mut frames[shown]);
        if frames.len() == 1 {
            frames.clear();
        }
        Self { name, width, height, visible, locked, opacity, cells, frames }
    }

    fn blank(&self) -> Vec<Cell> {
        vec![Cell::default(); self.width * self.height]
    }

    /// The frame whose cells are `cells`.
    fn shown_frame(&self) -> usize {
        self.frames.iter().position(Vec::is_empty).unwrap_or(0)
    }

    /// Makes the layer hold `count` frames with `current` shown, keeping what it has.
    /// A single-frame layer joining an animation shows its cells in `current`, the other frames start blank.
    pub(crate) fn sync_frames(&mut self, count: usize, current: usize) {
        if self.frames.is_empty() {
            if count > 1 {
                self.frames = (0..count).map(|f| if f == current { Vec::new() } else { self.blank() }).collect();
            }
            return;
        }
        self.show_frame(current);
        if count <= 1 {
            self.frames.clear();
        }
    }

    /// Puts the cells of frame `frame` into `cells`.
    pub(crate) fn show_frame(&mut self, frame: usize) {
        let shown = self.shown_frame();
        if frame == shown || frame >= self.frames.len() { return; }
        let incoming = std::mem::take(&mut self.frames[frame]);
        self.frames[shown] = std::mem::replace(&mut self.cells, incoming);
    }

    /// The cells of any frame.
    pub fn cells_in(&self, frame: usize) -> &[Cell] {
        match self.frames.get(frame) {
            Some(cells) if !cells.is_empty() => cells,
            _ => &self.cells,
        }
    }

    pub(crate) fn cells_in_mut(&mut self, frame: usize) -> &mut Vec<Cell> {
        match self.frames.get(frame) {
            Some(cells) if !cells.is_empty() => &mut self.frames[frame],
            _ => &mut self.cells,
        }
    }

    /// A new frame at `index` with `cells` (blank when `None`); `current` is shown before and after.
    pub(crate) fn insert_frame(&mut self, index: usize, cells: Option<Vec<Cell>>, count: usize, current: usize) {
        self.sync_frames(count, current);
        if self.frames.is_empty() {
            self.frames.push(Vec::new());
        }
        let cells = cells.unwrap_or_else(|| self.blank());
        self.frames.insert(index, cells);
    }

    /// Takes frame `index` out; it must not be the one shown.
    pub(crate) fn remove_frame(&mut self, index: usize) -> Vec<Cell> {
        let cells = self.frames.remove(index);
        if self.frames.len() == 1 {
            self.frames.clear();
        }
        cells
    }

    pub(crate) fn move_frame(&mut self, from: usize, to: usize) {
        if from < self.frames.len() && to < self.frames.len() {
            let cells = self.frames.remove(from);
            self.frames.insert(to, cells);
        }
    }

    /// The cell buffer matches the declared size (checked after loading a file).
    pub fn is_valid(&self) -> bool {
        let size = self.width * self.height;
        self.cells.len() == size
            && (self.frames.is_empty()
                || (self.frames.len() > 1
                    && self.frames.iter().filter(|f| f.is_empty()).count() == 1
                    && self.frames.iter().all(|f| f.is_empty() || f.len() == size)))
    }

    /// Frames held (1 for a still picture).
    pub fn frame_count(&self) -> usize {
        self.frames.len().max(1)
    }

    fn idx(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        self.idx(x, y).map(|i| &self.cells[i])
    }

    pub fn set(&mut self, x: usize, y: usize, cell: Cell) -> bool {
        if self.locked {
            return false;
        }
        self.put(x, y, cell)
    }

    /// Writes a cell even if the layer is locked (used by undo / redo).
    pub fn put(&mut self, x: usize, y: usize, cell: Cell) -> bool {
        if let Some(i) = self.idx(x, y) {
            self.cells[i] = cell;
            true
        } else {
            false
        }
    }

    /// Changes that would erase every non-empty cell.
    pub fn clear_changes(&self) -> Changes {
        let mut changes = Changes::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = &self.cells[y * self.width + x];
                if !cell.is_empty() {
                    changes.insert((x, y), (cell.clone(), Cell::empty()));
                }
            }
        }
        changes
    }

    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        let (w, h) = (self.width, self.height);
        let resized = |cells: &[Cell]| {
            let mut new_cells = vec![Cell::default(); new_w * new_h];
            for y in 0..h.min(new_h) {
                for x in 0..w.min(new_w) {
                    new_cells[y * new_w + x] = cells[y * w + x].clone();
                }
            }
            new_cells
        };
        self.cells = resized(&self.cells);
        for frame in self.frames.iter_mut().filter(|f| !f.is_empty()) {
            *frame = resized(frame);
        }
        self.width  = new_w;
        self.height = new_h;
    }

    pub fn get_region(&self, x1: usize, y1: usize, x2: usize, y2: usize) -> Vec<Vec<Cell>> {
        let (x1, x2) = (x1.min(x2), x1.max(x2));
        let (y1, y2) = (y1.min(y2), y1.max(y2));
        (y1..=y2)
            .map(|y| {
                (x1..=x2)
                    .map(|x| self.get(x, y).cloned().unwrap_or_default())
                    .collect()
            })
            .collect()
    }
}