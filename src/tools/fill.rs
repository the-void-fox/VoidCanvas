use crate::document::{Cell, Document};
use super::{Changes, DrawContext};

#[derive(Default, Debug)]
pub struct FillTool;

impl FillTool {
    pub fn flood_fill(&self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let layer = &doc.layers[doc.active_layer];
        if layer.locked { return Changes::new(); }

        let target_ch = match layer.get(x, y) {
            Some(c) => c.ch,
            None => return Changes::new(),
        };

        let new_ch = ctx.current_char();
        if new_ch == target_ch { return Changes::new(); }

        let width = layer.width;
        let height = layer.height;

        let mut changes = Changes::new();
        let mut stack = vec![(x, y)];
        let mut visited = vec![false; width * height];

        while let Some((cx, cy)) = stack.pop() {
            if cx >= width || cy >= height { continue; }
            let idx = cy * width + cx;
            if visited[idx] { continue; }

            let current_ch = doc.layers[doc.active_layer]
                .get(cx, cy)
                .map(|c| c.ch)
                .unwrap_or(' ');

            if current_ch != target_ch { continue; }

            visited[idx] = true;
            let old = doc.layers[doc.active_layer].get(cx, cy).cloned().unwrap_or_default();
            let new = Cell::new(new_ch, ctx.current_fg(), ctx.current_bg());
            doc.active_layer_mut().set(cx, cy, new.clone());
            changes.insert((cx, cy), (old, new));

            if cx + 1 < width  { stack.push((cx + 1, cy)); }
            if cx > 0          { stack.push((cx - 1, cy)); }
            if cy + 1 < height { stack.push((cx, cy + 1)); }
            if cy > 0          { stack.push((cx, cy - 1)); }
        }

        changes
    }
}