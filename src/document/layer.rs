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
    cells: Vec<Cell>,
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
        }
    }

    /// The cell buffer matches the declared size (checked after loading a file).
    pub fn is_valid(&self) -> bool {
        self.cells.len() == self.width * self.height
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
        let mut new_cells = vec![Cell::default(); new_w * new_h];
        for y in 0..self.height.min(new_h) {
            for x in 0..self.width.min(new_w) {
                new_cells[y * new_w + x] = self.cells[y * self.width + x].clone();
            }
        }
        self.cells = new_cells;
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