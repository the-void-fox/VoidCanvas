//! Pixel art inside cells. Half blocks give two square pixels per cell, top and bottom, each
//! with its own colour (`▀` / `▄` / `█` with the foreground and background). The finer grids —
//! quarters (2×2), sixths (2×3) and braille dots (2×4) — draw in one colour per cell.

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

/// How finely the Pixel tool splits a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelGrid {
    /// 1×2 half blocks, two colours per cell.
    #[default]
    Halves,
    /// 2×2 quarter blocks.
    Quarters,
    /// 2×3 sextants.
    Sixths,
    /// 2×4 braille dots.
    Braille,
}

/// Quarter blocks by their quarters (bit 1 up-left, 2 up-right, 4 down-left, 8 down-right), from 1.
const QUARTERS: [char; 15] = ['▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
/// Braille dot bit of (column, row).
const BRAILLE: [[u32; 4]; 2] = [[0, 1, 2, 6], [3, 4, 5, 7]];

impl PixelGrid {
    pub const ALL: [PixelGrid; 4] = [PixelGrid::Halves, PixelGrid::Quarters, PixelGrid::Sixths, PixelGrid::Braille];

    pub fn name(self) -> &'static str {
        match self {
            PixelGrid::Halves => "Halves 1×2",
            PixelGrid::Quarters => "Quarters 2×2",
            PixelGrid::Sixths => "Sixths 2×3",
            PixelGrid::Braille => "Braille 2×4",
        }
    }

    /// Pixels across and down one cell.
    pub fn size(self) -> (usize, usize) {
        match self {
            PixelGrid::Halves => (1, 2),
            PixelGrid::Quarters => (2, 2),
            PixelGrid::Sixths => (2, 3),
            PixelGrid::Braille => (2, 4),
        }
    }

    /// The pixel under a point `sub` (0..1 across and down) inside cell (x, y).
    pub fn pixel_at(self, x: usize, y: usize, sub: (f32, f32)) -> (usize, usize) {
        let (w, h) = self.size();
        let part = |f: f32, n: usize| ((f * n as f32) as usize).min(n - 1);
        (x * w + part(sub.0, w), y * h + part(sub.1, h))
    }

    fn bit(self, col: usize, row: usize) -> u32 {
        match self {
            PixelGrid::Braille => 1 << BRAILLE[col][row],
            _ => 1 << (row * 2 + col),
        }
    }

    /// The pixels set in `ch`, if it's a glyph of this grid (a space has none).
    fn mask(self, ch: char) -> Option<u32> {
        if ch == ' ' { return Some(0); }
        match self {
            PixelGrid::Halves => None,
            PixelGrid::Quarters => QUARTERS.iter().position(|&c| c == ch).map(|i| i as u32 + 1),
            PixelGrid::Sixths => match ch {
                '▌' => Some(21),
                '▐' => Some(42),
                '█' => Some(63),
                _ => {
                    let index = (ch as u32).checked_sub(0x1FB00).filter(|&i| i < 60)?;
                    let mut bits = index + 1;
                    if bits >= 21 { bits += 1; }
                    if bits >= 42 { bits += 1; }
                    Some(bits)
                }
            },
            PixelGrid::Braille => (ch as u32).checked_sub(0x2800).filter(|&b| b < 0x100),
        }
    }

    /// The glyph with exactly these pixels set.
    fn glyph(self, mask: u32) -> char {
        if mask == 0 { return ' '; }
        match self {
            PixelGrid::Halves => ' ',
            PixelGrid::Quarters => QUARTERS[mask as usize - 1],
            PixelGrid::Sixths => match mask {
                21 => '▌',
                42 => '▐',
                63 => '█',
                m => char::from_u32(0x1FB00 + m - 1 - (m > 21) as u32 - (m > 42) as u32).unwrap_or(' '),
            },
            PixelGrid::Braille => char::from_u32(0x2800 + mask).unwrap_or(' '),
        }
    }
}

#[derive(Default, Debug)]
pub struct PixelTool {
    last: Option<(usize, usize)>,
    accumulated: Changes,
}

impl PixelTool {
    /// Pixel coordinates count `grid` pixels across and down the whole canvas.
    pub fn on_press(&mut self, doc: &mut Document, (px, py): (usize, usize), grid: PixelGrid, ctx: &DrawContext) {
        self.accumulated.clear();
        self.last = Some((px, py));
        self.stamp(doc, px, py, grid, ctx);
    }

    pub fn on_drag(&mut self, doc: &mut Document, (px, py): (usize, usize), grid: PixelGrid, ctx: &DrawContext) {
        let Some((lx, ly)) = self.last else { return };
        for (x, y) in bresenham(lx as isize, ly as isize, px as isize, py as isize) {
            self.stamp(doc, x, y, grid, ctx);
        }
        self.last = Some((px, py));
    }

    pub fn on_release(&mut self) -> Changes {
        self.last = None;
        std::mem::take(&mut self.accumulated)
    }

    /// The cells a press at pixel (px, py) would produce.
    pub fn preview(&self, doc: &Document, (px, py): (usize, usize), grid: PixelGrid, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let layer = doc.active_layer();
        let (gw, gh) = grid.size();
        let mut cells: Vec<(usize, usize, Cell)> = Vec::new();
        for (x, y) in pixels(px, py, grid, ctx, doc.width, doc.height) {
            let (cx, cy) = (x / gw, y / gh);
            let index = match cells.iter().position(|c| (c.0, c.1) == (cx, cy)) {
                Some(i) => i,
                None => {
                    let Some(cell) = layer.get(cx, cy) else { continue };
                    cells.push((cx, cy, cell.clone()));
                    cells.len() - 1
                }
            };
            cells[index].2 = with_pixel(&cells[index].2, grid, (x % gw, y % gh), ctx);
        }
        cells
    }

    fn stamp(&mut self, doc: &mut Document, px: usize, py: usize, grid: PixelGrid, ctx: &DrawContext) {
        let (cols, rows) = (doc.width, doc.height);
        let (gw, gh) = grid.size();
        let layer = doc.active_layer_mut();
        for (x, y) in pixels(px, py, grid, ctx, cols, rows) {
            let (cx, cy) = (x / gw, y / gh);
            let Some(old) = layer.get(cx, cy).cloned() else { continue };
            let new = with_pixel(&old, grid, (x % gw, y % gh), ctx);
            layer.set(cx, cy, new.clone());
            merge_changes(&mut self.accumulated, Changes::from([((cx, cy), (old, new))]));
        }
    }
}

/// Pixels covered by the brush at (px, py), mirrored if symmetry is on.
fn pixels(px: usize, py: usize, grid: PixelGrid, ctx: &DrawContext, cols: usize, rows: usize) -> Vec<(usize, usize)> {
    let (gw, gh) = grid.size();
    let (w, h) = (cols * gw, rows * gh);
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

/// `cell` with the pixel at (col, row) inside it painted. Half blocks take the current slot's
/// foreground; on the finer grids the left button sets a pixel in the primary foreground
/// (which colours the whole cell) and the right button clears it. Any other glyph is replaced.
fn with_pixel(cell: &Cell, grid: PixelGrid, (col, row): (usize, usize), ctx: &DrawContext) -> Cell {
    if grid == PixelGrid::Halves {
        let color = ctx.current_fg();
        let paint = (!color.is_transparent()).then_some(color);
        let (top, low) = halves(cell);
        return if row == 1 { from_halves(top, paint) } else { from_halves(paint, low) };
    }
    let mask = grid.mask(cell.ch).unwrap_or(0);
    let bit = grid.bit(col, row);
    let (mask, fg) = if ctx.is_primary { (mask | bit, ctx.primary_fg.clone()) } else { (mask & !bit, cell.fg.clone()) };
    Cell { ch: grid.glyph(mask), fg, bg: cell.bg.clone() }
}

/// Is the pixel at (col, row) inside `cell` set? (For picking on the finer grids.)
pub fn pixel_set(cell: &Cell, grid: PixelGrid, (col, row): (usize, usize)) -> bool {
    grid.mask(cell.ch).is_some_and(|mask| mask & grid.bit(col, row) != 0)
}
