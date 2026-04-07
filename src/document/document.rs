use serde::{Deserialize, Serialize};
use super::{cell::Cell, layer::Layer};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    #[serde(skip)]
    pub modified: bool,
    #[serde(skip)]
    pub file_path: Option<String>,
}

impl Document {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            layers: vec![Layer::new(width, height, "Background")],
            active_layer: 0,
            modified: false,
            file_path: None,
        }
    }

    pub fn active_layer(&self) -> &Layer {
        &self.layers[self.active_layer]
    }

    pub fn active_layer_mut(&mut self) -> &mut Layer {
        &mut self.layers[self.active_layer]
    }

    pub fn add_layer(&mut self) -> usize {
        let name = format!("Layer {}", self.layers.len() + 1);
        let idx = self.active_layer + 1;
        self.layers.insert(idx, Layer::new(self.width, self.height, name));
        self.active_layer = idx;
        self.modified = true;
        idx
    }

    pub fn remove_layer(&mut self) -> bool {
        if self.layers.len() <= 1 { return false; }
        self.layers.remove(self.active_layer);
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len() - 1;
        }
        self.modified = true;
        true
    }

    pub fn duplicate_layer(&mut self) {
        let mut new_layer = self.layers[self.active_layer].clone();
        new_layer.name = format!("{} (copy)", new_layer.name);
        let idx = self.active_layer + 1;
        self.layers.insert(idx, new_layer);
        self.active_layer = idx;
        self.modified = true;
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from == to || from >= self.layers.len() || to >= self.layers.len() {
            return;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        self.active_layer = to;
        self.modified = true;
    }

    pub fn merge_down(&mut self) {
        let idx = self.active_layer;
        if idx + 1 >= self.layers.len() { return; }

        let upper_cells: Vec<(usize, usize, Cell)> = {
            let upper = &self.layers[idx];
            let mut changes = Vec::new();
            for y in 0..upper.height {
                for x in 0..upper.width {
                    if let Some(c) = upper.get(x, y) {
                        if !c.is_empty() {
                            changes.push((x, y, c.clone()));
                        }
                    }
                }
            }
            changes
        };

        let lower = &mut self.layers[idx + 1];
        for (x, y, cell) in upper_cells {
            lower.set(x, y, cell);
        }

        self.layers.remove(idx);
        if self.active_layer > 0 {
            self.active_layer -= 1;
        }
        self.modified = true;
    }

    pub fn composite_cell(&self, x: usize, y: usize) -> Option<&Cell> {
        for layer in self.layers.iter().rev() {
            if !layer.visible { continue; }
            if let Some(cell) = layer.get(x, y) {
                if !cell.is_empty() {
                    return Some(cell);
                }
            }
        }
        None
    }

    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        for layer in &mut self.layers {
            layer.resize(new_w, new_h);
        }
        self.width = new_w;
        self.height = new_h;
        self.modified = true;
    }

    pub fn to_text(&self) -> String {
        let mut lines: Vec<String> = (0..self.height)
            .map(|y| {
                let line: String = (0..self.width)
                    .map(|x| {
                        self.composite_cell(x, y)
                            .map(|c| c.ch)
                            .unwrap_or(' ')
                    })
                    .collect();
                line.trim_end().to_string()
            })
            .collect();

        while lines.last().map(|l: &String| l.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        lines.join("\n")
    }

    pub fn from_text(&mut self, text: &str) {
        self.layers = vec![Layer::new(self.width, self.height, "Imported")];
        self.active_layer = 0;
        for (y, line) in text.lines().enumerate() {
            if y >= self.height { break; }
            for (x, ch) in line.chars().enumerate() {
                if x >= self.width { break; }
                if let Some(cell) = self.layers[0].get_mut(x, y) {
                    cell.ch = ch;
                }
            }
        }
        self.modified = true;
    }
}