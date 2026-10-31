use crate::document::{Cell, Document};
use super::{Changes, DrawContext, apply_stamps, brush_positions, bresenham, merge_changes, preview_stamps};

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
        let stamps = self.stamps(cx, cy, ctx);
        let changes = apply_stamps(doc, &stamps, ctx);
        merge_changes(&mut self.accumulated, changes.clone());
        changes
    }

    fn stamps(&self, cx: usize, cy: usize, ctx: &DrawContext) -> Vec<(usize, usize, char)> {
        let glyph = ctx.current_char();
        brush_positions(cx, cy, ctx.brush_size).into_iter().map(|(x, y)| (x, y, glyph)).collect()
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        preview_stamps(doc, &self.stamps(x, y, ctx), ctx)
    }
}