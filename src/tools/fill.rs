use crate::document::{Cell, Document};
use super::{Changes, DrawContext};

#[derive(Default, Debug)]
pub struct FillTool;

impl FillTool {
    /// Fills the 4-connected region of cells that look like the one at (x, y).
    pub fn flood_fill(&self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let layer = doc.active_layer_mut();
        if layer.locked { return Changes::new(); }

        let target = match layer.get(x, y) {
            Some(c) => c.clone(),
            None => return Changes::new(),
        };

        let new = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());
        if new.same_look(&target) { return Changes::new(); }

        let width = layer.width;
        let height = layer.height;

        let mut changes = Changes::new();
        let mut stack = vec![(x, y)];
        let mut visited = vec![false; width * height];

        while let Some((cx, cy)) = stack.pop() {
            if cx >= width || cy >= height { continue; }
            let idx = cy * width + cx;
            if visited[idx] { continue; }
            visited[idx] = true;

            let Some(old) = layer.get(cx, cy).cloned() else { continue };
            if !old.same_look(&target) { continue; }

            layer.set(cx, cy, new.clone());
            changes.insert((cx, cy), (old, new.clone()));

            if cx + 1 < width  { stack.push((cx + 1, cy)); }
            if cx > 0          { stack.push((cx - 1, cy)); }
            if cy + 1 < height { stack.push((cx, cy + 1)); }
            if cy > 0          { stack.push((cx, cy - 1)); }
        }

        changes
    }
}
