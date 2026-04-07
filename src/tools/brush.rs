use crate::document::{Cell, Document};
use super::{Changes, DrawContext, brush_positions, bresenham};

#[derive(Default, Debug)]
pub struct BrushTool {
    pub last_pos: Option<(usize, usize)>,
    pub accumulated: Changes,
}

impl BrushTool {
    pub fn on_press(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        self.accumulated.clear();
        self.last_pos = Some((x, y));
        self.draw_at(doc, x, y, ctx)
    }

    pub fn on_drag(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((lx, ly)) = self.last_pos else { return Changes::new(); };
        let points = bresenham(lx as isize, ly as isize, x as isize, y as isize);
        let mut all = Changes::new();
        for (px, py) in points {
            all.extend(self.draw_at(doc, px, py, ctx));
        }
        self.last_pos = Some((x, y));
        all
    }

    pub fn on_release(&mut self) -> Changes {
        self.last_pos = None;
        let result = self.accumulated.clone();
        self.accumulated.clear();
        result
    }

    fn draw_at(&mut self, doc: &mut Document, cx: usize, cy: usize, ctx: &DrawContext) -> Changes {
        let mut changes = Changes::new();
        let layer = doc.active_layer_mut();
        let new_cell = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());

        for (px, py) in brush_positions(cx, cy, ctx.brush_size) {
            if px >= layer.width || py >= layer.height { continue; }
            if !self.accumulated.contains_key(&(px, py)) {
                if let Some(old) = layer.get(px, py) {
                    self.accumulated.insert((px, py), (old.clone(), new_cell.clone()));
                }
            }
            layer.set(px, py, new_cell.clone());
            if let Some(entry) = self.accumulated.get_mut(&(px, py)) {
                entry.1 = new_cell.clone();
            }
            changes.insert((px, py), self.accumulated[&(px, py)].clone());
        }
        changes
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let cell = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());
        brush_positions(x, y, ctx.brush_size)
            .into_iter()
            .filter(|&(px, py)| px < doc.width && py < doc.height)
            .map(|(px, py)| (px, py, cell.clone()))
            .collect()
    }
}