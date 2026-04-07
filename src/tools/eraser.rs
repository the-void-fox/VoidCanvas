use crate::document::{Cell, Document};
use super::{Changes, DrawContext, brush_positions, bresenham};

#[derive(Default, Debug)]
pub struct EraserTool {
    pub last_pos: Option<(usize, usize)>,
    pub accumulated: Changes,
}

impl EraserTool {
    pub fn on_press(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        self.accumulated.clear();
        self.last_pos = Some((x, y));
        self.erase_at(doc, x, y, ctx.brush_size)
    }

    pub fn on_drag(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((lx, ly)) = self.last_pos else { return Changes::new(); };
        let points = bresenham(lx as isize, ly as isize, x as isize, y as isize);
        let mut all = Changes::new();
        for (px, py) in points {
            all.extend(self.erase_at(doc, px, py, ctx.brush_size));
        }
        self.last_pos = Some((x, y));
        all
    }

    pub fn on_release(&mut self) -> Changes {
        self.last_pos = None;
        let r = self.accumulated.clone();
        self.accumulated.clear();
        r
    }

    fn erase_at(&mut self, doc: &mut Document, cx: usize, cy: usize, size: usize) -> Changes {
        let mut changes = Changes::new();
        let layer = doc.active_layer_mut();
        let empty = Cell::empty();

        for (px, py) in brush_positions(cx, cy, size) {
            if px >= layer.width || py >= layer.height { continue; }
            if !self.accumulated.contains_key(&(px, py)) {
                if let Some(old) = layer.get(px, py) {
                    self.accumulated.insert((px, py), (old.clone(), empty.clone()));
                }
            }
            layer.set(px, py, empty.clone());
            if let Some(entry) = self.accumulated.get_mut(&(px, py)) {
                entry.1 = empty.clone();
            }
            changes.insert((px, py), self.accumulated[&(px, py)].clone());
        }
        changes
    }
}