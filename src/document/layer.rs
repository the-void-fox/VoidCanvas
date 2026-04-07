use serde::{Deserialize, Serialize};
use super::cell::Cell;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        self.idx(x, y).map(|i| &mut self.cells[i])
    }

    pub fn set(&mut self, x: usize, y: usize, cell: Cell) -> bool {
        if self.locked {
            return false;
        }
        if let Some(i) = self.idx(x, y) {
            self.cells[i] = cell;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        if !self.locked {
            self.cells.iter_mut().for_each(|c| *c = Cell::default());
        }
    }

    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        let mut new_cells = vec![Cell::default(); new_w * new_h];
        for y in 0..self.height.min(new_h) {
            for x in 0..self.width.min(new_w) {
                new_cells[y * new_w + x] = self.cells[y * self.width + x].clone();
            }
        }
        self.cells = new_cells;
        self.width = new_w;
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

    pub fn paste_region(&mut self, x: usize, y: usize, region: &[Vec<Cell>]) {
        if self.locked { return; }
        for (dy, row) in region.iter().enumerate() {
            for (dx, cell) in row.iter().enumerate() {
                self.set(x + dx, y + dy, cell.clone());
            }
        }
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
}