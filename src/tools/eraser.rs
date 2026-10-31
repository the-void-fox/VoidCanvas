use crate::document::Document;
use super::{Changes, DrawContext, brush_positions, bresenham, merge_changes};

#[derive(Default, Debug)]
pub struct EraserTool {
    pub last_pos: Option<(usize, usize)>,
    pub accumulated: Changes,
}

impl EraserTool {
    pub fn on_press(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        self.accumulated.clear();
        self.last_pos = Some((x, y));
        self.erase_at(doc, x, y, ctx)
    }

    pub fn on_drag(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((lx, ly)) = self.last_pos else { return Changes::new(); };
        let points = bresenham(lx as isize, ly as isize, x as isize, y as isize);
        let mut all = Changes::new();
        for (px, py) in points {
            all.extend(self.erase_at(doc, px, py, ctx));
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

    fn erase_at(&mut self, doc: &mut Document, cx: usize, cy: usize, ctx: &DrawContext) -> Changes {
        let mut changes = Changes::new();
        let layer = doc.active_layer_mut();
        for (px, py) in brush_positions(cx, cy, ctx.brush_size) {
            let Some(old) = layer.get(px, py).cloned() else { continue };
            let new = ctx.erase(&old);
            layer.set(px, py, new.clone());
            changes.insert((px, py), (old, new));
        }
        merge_changes(&mut self.accumulated, changes.clone());
        changes
    }
}