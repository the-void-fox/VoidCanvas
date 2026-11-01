//! Pixel art with half blocks: every cell holds two square pixels, top and bottom,
//! stored as `▀` / `▄` / `█` with the foreground and background colours.

use crate::document::{Cell, Color, Document};
use super::{Changes, DrawContext, bresenham, brush_positions, merge_changes};

/// The top and bottom pixel colours of a cell (`None` = transparent).
pub fn halves(cell: &Cell) -> (Option<Color>, Option<Color>) {
    let opaque = |c: &Color| (!c.is_transparent()).then(|| c.clone());
    let (fg, bg) = (opaque(&cell.fg), opaque(&cell.bg));
    match cell.ch {
        '▀' => (fg, bg),
        '▄' => (bg, fg),
        '█' => (fg.clone(), fg),
        // Any other glyph is painted over; its background stays.
        _ => (bg.clone(), bg),
    }
}

/// The cell showing these two pixels.
pub fn from_halves(top: Option<Color>, bottom: Option<Color>) -> Cell {
    match (top, bottom) {
        (None, None) => Cell::empty(),
        (Some(t), Some(b)) if t == b => Cell::new('█', t, Color::TRANSPARENT),
        (Some(t), None) => Cell::new('▀', t, Color::TRANSPARENT),
        (None, Some(b)) => Cell::new('▄', b, Color::TRANSPARENT),
        (Some(t), Some(b)) => Cell::new('▀', t, b),
    }
}

#[derive(Default, Debug)]
pub struct PixelTool {
    last: Option<(usize, usize)>,
    accumulated: Changes,
}

impl PixelTool {
    /// Pixel coordinates are cells across and half cells down.
    pub fn on_press(&mut self, doc: &mut Document, px: usize, py: usize, ctx: &DrawContext) {
        self.accumulated.clear();
        self.last = Some((px, py));
        self.stamp(doc, px, py, ctx);
    }

    pub fn on_drag(&mut self, doc: &mut Document, px: usize, py: usize, ctx: &DrawContext) {
        let Some((lx, ly)) = self.last else { return };
        for (x, y) in bresenham(lx as isize, ly as isize, px as isize, py as isize) {
            self.stamp(doc, x, y, ctx);
        }
        self.last = Some((px, py));
    }

    pub fn on_release(&mut self) -> Changes {
        self.last = None;
        std::mem::take(&mut self.accumulated)
    }

    /// The cells a press at pixel (px, py) would produce.
    pub fn preview(&self, doc: &Document, px: usize, py: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let layer = doc.active_layer();
        let mut cells: Vec<(usize, usize, Cell)> = Vec::new();
        for (x, y) in pixels(px, py, ctx, doc.width, doc.height) {
            let (cx, cy) = (x, y / 2);
            let index = match cells.iter().position(|c| (c.0, c.1) == (cx, cy)) {
                Some(i) => i,
                None => {
                    let Some(cell) = layer.get(cx, cy) else { continue };
                    cells.push((cx, cy, cell.clone()));
                    cells.len() - 1
                }
            };
            cells[index].2 = with_pixel(&cells[index].2, y % 2 == 1, ctx);
        }
        cells
    }

    fn stamp(&mut self, doc: &mut Document, px: usize, py: usize, ctx: &DrawContext) {
        let (cols, rows) = (doc.width, doc.height);
        let layer = doc.active_layer_mut();
        for (x, y) in pixels(px, py, ctx, cols, rows) {
            let Some(old) = layer.get(x, y / 2).cloned() else { continue };
            let new = with_pixel(&old, y % 2 == 1, ctx);
            layer.set(x, y / 2, new.clone());
            merge_changes(&mut self.accumulated, Changes::from([((x, y / 2), (old, new))]));
        }
    }
}

/// Pixels covered by the brush at (px, py), mirrored if symmetry is on.
fn pixels(px: usize, py: usize, ctx: &DrawContext, cols: usize, rows: usize) -> Vec<(usize, usize)> {
    let (w, h) = (cols, rows * 2);
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (x, y) in brush_positions(px, py, ctx.brush_size) {
        if x >= w || y >= h { continue; }
        for (mx, my) in ctx.mirror.points(x, y, w, h) {
            if !out.contains(&(mx, my)) {
                out.push((mx, my));
            }
        }
    }
    out
}

/// `cell` with its top (or bottom) pixel set to the current slot's foreground.
fn with_pixel(cell: &Cell, bottom: bool, ctx: &DrawContext) -> Cell {
    let color = ctx.current_fg();
    let paint = (!color.is_transparent()).then_some(color);
    let (top, low) = halves(cell);
    if bottom { from_halves(top, paint) } else { from_halves(paint, low) }
}
