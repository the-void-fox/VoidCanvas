//! Shade brush: every stroke over a cell steps its glyph one level denser
//! (or lighter with the secondary button) along a density ramp.

use std::collections::HashSet;

use crate::document::{Cell, Document};
use super::{Changes, DrawContext, bresenham, brush_positions, merge_changes};

#[derive(Default, Debug)]
pub struct ShadeTool {
    last: Option<(usize, usize)>,
    accumulated: Changes,
    /// Cells already shaded in this stroke (each steps once per stroke).
    visited: HashSet<(usize, usize)>,
}

impl ShadeTool {
    pub fn on_press(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext, ramp: &str) {
        self.accumulated.clear();
        self.visited.clear();
        self.last = Some((x, y));
        self.shade_at(doc, x, y, ctx, ramp);
    }

    pub fn on_drag(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext, ramp: &str) {
        let Some((lx, ly)) = self.last else { return };
        for (px, py) in bresenham(lx as isize, ly as isize, x as isize, y as isize) {
            self.shade_at(doc, px, py, ctx, ramp);
        }
        self.last = Some((x, y));
    }

    pub fn on_release(&mut self) -> Changes {
        self.last = None;
        self.visited.clear();
        std::mem::take(&mut self.accumulated)
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext, ramp: &str) -> Vec<(usize, usize, Cell)> {
        let layer = doc.active_layer();
        footprint(doc, x, y, ctx)
            .into_iter()
            .filter_map(|(px, py)| layer.get(px, py).map(|old| (px, py, shaded(old, ctx, ramp))))
            .collect()
    }

    fn shade_at(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext, ramp: &str) {
        let cells = footprint(doc, x, y, ctx);
        let layer = doc.active_layer_mut();
        for pos in cells {
            if !self.visited.insert(pos) { continue; }
            let Some(old) = layer.get(pos.0, pos.1).cloned() else { continue };
            let new = shaded(&old, ctx, ramp);
            layer.set(pos.0, pos.1, new.clone());
            merge_changes(&mut self.accumulated, Changes::from([(pos, (old, new))]));
        }
    }
}

/// Cells under the brush at (x, y), with their mirror images.
fn footprint(doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize)> {
    let (w, h) = (doc.width, doc.height);
    let mut cells = Vec::new();
    for (px, py) in brush_positions(x, y, ctx.brush_size) {
        if px >= w || py >= h { continue; }
        for p in ctx.mirror.points(px, py, w, h) {
            if !cells.contains(&p) {
                cells.push(p);
            }
        }
    }
    cells
}

/// `cell` one step denser (primary) or lighter (secondary); colours follow the paint channels.
fn shaded(cell: &Cell, ctx: &DrawContext, ramp: &str) -> Cell {
    let levels: Vec<char> = ramp.chars().collect();
    let level = levels.iter().position(|&c| c == cell.ch).unwrap_or(0);
    let next = if ctx.is_primary { (level + 1).min(levels.len() - 1) } else { level.saturating_sub(1) };
    let mut glyph_always = ctx.clone();
    glyph_always.channels.glyph = true;
    glyph_always.paint(cell, levels[next])
}
