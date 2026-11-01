use serde::{Deserialize, Serialize};
use super::{Changes, cell::Cell, layer::Layer};

/// Layers are stored bottom → top: `layers[0]` is the background,
/// the last layer is drawn on top of everything else.
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

    /// Sanity checks for a document that was just loaded from disk.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.layers.is_empty() {
            return Err("the file has no layers".into());
        }
        let damaged = self.layers.iter().find(|l| {
            !l.is_valid() || l.width != self.width || l.height != self.height
        });
        if let Some(layer) = damaged {
            return Err(format!("layer \"{}\" is damaged", layer.name));
        }
        self.active_layer = self.active_layer.min(self.layers.len() - 1);
        Ok(())
    }

    pub fn active_layer(&self) -> &Layer {
        &self.layers[self.active_layer]
    }

    pub fn active_layer_mut(&mut self) -> &mut Layer {
        &mut self.layers[self.active_layer]
    }

    /// A unique "Layer N" name.
    pub fn next_layer_name(&self) -> String {
        (self.layers.len() + 1..)
            .map(|n| format!("Layer {n}"))
            .find(|name| self.layers.iter().all(|l| &l.name != name))
            .unwrap()
    }

    pub fn insert_layer(&mut self, index: usize, layer: Layer) {
        let index = index.min(self.layers.len());
        self.layers.insert(index, layer);
        self.active_layer = index;
        self.modified = true;
    }

    pub fn take_layer(&mut self, index: usize) -> Layer {
        let layer = self.layers.remove(index);
        if self.active_layer >= self.layers.len() {
            self.active_layer = self.layers.len().saturating_sub(1);
        }
        self.modified = true;
        layer
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

    /// Changes to the layer below `upper` that bake `upper` into it.
    pub fn merge_down_changes(&self, upper: usize) -> Changes {
        let mut changes = Changes::new();
        if upper == 0 || upper >= self.layers.len() { return changes; }
        let (top, bottom) = (&self.layers[upper], &self.layers[upper - 1]);
        for y in 0..self.height {
            for x in 0..self.width {
                let (Some(t), Some(b)) = (top.get(x, y), bottom.get(x, y)) else { continue };
                let merged = t.over(b);
                if merged != *b {
                    changes.insert((x, y), (b.clone(), merged));
                }
            }
        }
        changes
    }

    /// What is visible at (x, y) after stacking all visible layers.
    pub fn composite_cell(&self, x: usize, y: usize) -> Cell {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .filter_map(|l| l.get(x, y))
            .fold(Cell::empty(), |below, top| top.over(&below))
    }

    /// What would be visible at (x, y) if the active layer's cell there were `cell` (previews).
    pub fn composite_with(&self, x: usize, y: usize, cell: &Cell) -> Cell {
        self.layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.visible)
            .filter_map(|(i, l)| if i == self.active_layer { Some(cell) } else { l.get(x, y) })
            .fold(Cell::empty(), |below, top| top.over(&below))
    }

    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        for layer in &mut self.layers {
            layer.resize(new_w, new_h);
        }
        self.width = new_w;
        self.height = new_h;
        self.modified = true;
    }
}
