use serde::{Deserialize, Serialize};
use super::{Blend, Stack, cell::Cell, layer::Layer};

/// How long a new animation frame shows, in milliseconds.
pub const DEFAULT_FRAME_MS: u32 = 100;

/// One animation frame: its cells in every layer (bottom first) and how long it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameData {
    pub duration: u32,
    pub cells: Vec<Vec<Cell>>,
}

fn is_still(durations: &[u32]) -> bool {
    durations.len() <= 1
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Layers are stored bottom → top: `layers[0]` is the background,
/// the last layer is drawn on top of everything else.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<Layer>,
    pub active_layer: usize,
    /// How long each animation frame shows, in milliseconds; a still picture has one frame.
    #[serde(default, skip_serializing_if = "is_still")]
    pub durations: Vec<u32>,
    /// The frame being shown and edited.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub frame: usize,
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
            durations: vec![DEFAULT_FRAME_MS],
            frame: 0,
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
        if self.durations.is_empty() {
            self.durations.push(DEFAULT_FRAME_MS);
        }
        let count = self.frame_count();
        if self.layers.iter().any(|l| l.frame_count() != count) {
            return Err("the animation frames don't match".into());
        }
        self.frame = self.frame.min(count - 1);
        for layer in &mut self.layers {
            layer.sync_frames(count, self.frame);
        }
        Ok(())
    }

    pub fn frame_count(&self) -> usize {
        self.durations.len()
    }

    /// Shows (and edits) frame `frame` from now on.
    pub fn show_frame(&mut self, frame: usize) {
        if frame >= self.frame_count() { return; }
        for layer in &mut self.layers {
            layer.show_frame(frame);
        }
        self.frame = frame;
    }

    /// A copy of frame `frame`.
    pub fn frame_data(&self, frame: usize) -> FrameData {
        FrameData {
            duration: self.durations[frame],
            cells: self.layers.iter().map(|l| l.cells_in(frame).to_vec()).collect(),
        }
    }

    /// An empty frame as long as the one shown.
    pub fn blank_frame(&self) -> FrameData {
        let blank = vec![Cell::default(); self.width * self.height];
        FrameData { duration: self.durations[self.frame], cells: vec![blank; self.layers.len()] }
    }

    /// Adds a frame at `index` and shows it.
    pub fn insert_frame(&mut self, index: usize, data: FrameData) {
        let count = self.frame_count();
        let index = index.min(count);
        for (layer, cells) in self.layers.iter_mut().zip(data.cells) {
            layer.insert_frame(index, Some(cells), count, self.frame);
        }
        if index <= self.frame {
            self.frame += 1;
        }
        self.durations.insert(index, data.duration);
        self.show_frame(index);
        self.modified = true;
    }

    /// Takes frame `index` out (another one must be left); a neighbour is shown if it was.
    pub fn take_frame(&mut self, index: usize) -> FrameData {
        if self.frame == index {
            self.show_frame(if index + 1 < self.frame_count() { index + 1 } else { index - 1 });
        }
        let cells = self.layers.iter_mut().map(|l| l.remove_frame(index)).collect();
        let duration = self.durations.remove(index);
        if self.frame > index {
            self.frame -= 1;
        }
        self.modified = true;
        FrameData { duration, cells }
    }

    pub fn move_frame(&mut self, from: usize, to: usize) {
        let count = self.frame_count();
        if from == to || from >= count || to >= count { return; }
        for layer in &mut self.layers {
            layer.move_frame(from, to);
        }
        let duration = self.durations.remove(from);
        self.durations.insert(to, duration);
        self.frame = if self.frame == from {
            to
        } else if from < self.frame && to >= self.frame {
            self.frame - 1
        } else if from > self.frame && to <= self.frame {
            self.frame + 1
        } else {
            self.frame
        };
        self.modified = true;
    }

    /// Puts `layer` (taken from this document earlier, maybe while another frame was shown) at `index`.
    pub fn replace_layer(&mut self, index: usize, mut layer: Layer) {
        layer.sync_frames(self.frame_count(), self.frame);
        self.layers[index] = layer;
        self.modified = true;
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

    pub fn insert_layer(&mut self, index: usize, mut layer: Layer) {
        layer.sync_frames(self.frame_count(), self.frame);
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

    /// The layer below `upper` with `upper` baked into it, in every frame, by its blend mode and
    /// opacity. (Overlaid glyphs can't share one cell: the top one is kept.)
    pub fn merged_down(&self, upper: usize) -> Layer {
        let top = &self.layers[upper];
        let mut merged = self.layers[upper - 1].clone();
        for frame in 0..self.frame_count() {
            for (below, t) in merged.cells_in_mut(frame).iter_mut().zip(top.cells_in(frame)) {
                let mut stack = Stack::from_cell(below);
                stack.push(t, top.blend, top.opacity);
                *below = stack.top();
            }
        }
        merged
    }

    /// What is visible at (x, y) as one cell: the top glyph over the blended backgrounds.
    pub fn composite_cell(&self, x: usize, y: usize) -> Cell {
        self.composite_in(self.frame, x, y)
    }

    /// Like [`Self::composite_cell`], in any frame.
    pub fn composite_in(&self, frame: usize, x: usize, y: usize) -> Cell {
        self.stack_in(frame, x, y).top()
    }

    /// Everything visible at (x, y) in `frame`, overlaid glyphs included.
    pub fn stack_in(&self, frame: usize, x: usize, y: usize) -> Stack {
        let mut stack = Stack::new();
        if x >= self.width || y >= self.height { return stack; }
        let i = y * self.width + x;
        for layer in self.layers.iter().filter(|l| l.visible) {
            stack.push(&layer.cells_in(frame)[i], layer.blend, layer.opacity);
        }
        stack
    }

    /// What would be visible at (x, y) if the active layer's cell there were `cell` (previews).
    pub fn stack_with(&self, x: usize, y: usize, cell: &Cell) -> Stack {
        let mut stack = Stack::new();
        for (i, layer) in self.layers.iter().enumerate().filter(|(_, l)| l.visible) {
            let own = if i == self.active_layer { Some(cell) } else { layer.get(x, y) };
            if let Some(own) = own {
                stack.push(own, layer.blend, layer.opacity);
            }
        }
        stack
    }

    /// Does any cell of `frame` show glyphs over glyphs, which text can't hold?
    pub fn has_overlaid_glyphs(&self, frame: usize) -> bool {
        self.layers.iter().any(|l| l.visible && l.blend == Blend::Overlay)
            && (0..self.height).any(|y| (0..self.width).any(|x| self.stack_in(frame, x, y).glyphs.len() > 1))
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
