//! The Brush: a square of the glyph, a stamp, a scatter of glyphs or a line that picks its glyph
//! by the way the stroke goes. Brushes can be kept as named presets.

use serde::{Deserialize, Serialize};

use crate::document::{Cell, Document};
use super::{Changes, DrawContext, brush_positions, bresenham, merge_changes};

/// What the Brush paints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrushKind {
    /// The glyph in a size × size square.
    Square,
    /// A block of cells (empty ones left out), centred on the pointer. With `recolor` it takes
    /// the slot's colours, otherwise its own.
    Stamp { cells: Vec<Vec<Cell>>, recolor: bool },
    /// Glyphs picked at random from `glyphs`, on `density` (0..1) of the cells of the size × size
    /// square. The same cell gets the same glyph all stroke long.
    Scatter { glyphs: String, density: f32 },
    /// One glyph per cell for the way the stroke goes: horizontal, vertical, rising, falling.
    Direction { glyphs: String },
}

/// A brush with a name, built in or saved by the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrushPreset {
    pub name: String,
    pub kind: BrushKind,
}

/// The brushes that come with the editor.
pub fn builtin_presets() -> Vec<BrushPreset> {
    let preset = |name: &str, kind| BrushPreset { name: name.to_owned(), kind };
    vec![
        preset("Square", BrushKind::Square),
        preset("Stars", BrushKind::Scatter { glyphs: ".·*+°".into(), density: 0.25 }),
        preset("Rain", BrushKind::Scatter { glyphs: "|:'".into(), density: 0.35 }),
        preset("Dots", BrushKind::Scatter { glyphs: "⠁⠂⠄⠈⠐⠠⡀⢀".into(), density: 0.6 }),
        preset("ASCII Lines", BrushKind::Direction { glyphs: "-|/\\".into() }),
        preset("Box Lines", BrushKind::Direction { glyphs: "─│╱╲".into() }),
    ]
}

#[derive(Debug)]
pub struct BrushTool {
    pub last_pos: Option<(usize, usize)>,
    pub accumulated: Changes,
    /// Makes each stroke's scatter different; changes after a stroke, so the preview shows the next one.
    seed: u64,
}

impl Default for BrushTool {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        Self { last_pos: None, accumulated: Changes::new(), seed }
    }
}

impl BrushTool {
    pub fn on_press(&mut self, doc: &mut Document, x: usize, y: usize, brush: &BrushKind, ctx: &DrawContext) {
        self.accumulated.clear();
        self.last_pos = Some((x, y));
        // A direction brush waits until the stroke shows which way it goes.
        if !matches!(brush, BrushKind::Direction { .. }) {
            self.draw_at(doc, x, y, brush, None, ctx);
        }
    }

    pub fn on_drag(&mut self, doc: &mut Document, x: usize, y: usize, brush: &BrushKind, ctx: &DrawContext) {
        let Some((lx, ly)) = self.last_pos else { return };
        if (lx, ly) == (x, y) { return; }
        let way = (x as isize - lx as isize, y as isize - ly as isize);
        // The first step of a direction stroke also marks where it started.
        let points = bresenham(lx as isize, ly as isize, x as isize, y as isize);
        let skip = if matches!(brush, BrushKind::Direction { .. }) && !self.accumulated.is_empty() { 1 } else { 0 };
        for (px, py) in points.into_iter().skip(skip) {
            self.draw_at(doc, px, py, brush, Some(way), ctx);
        }
        self.last_pos = Some((x, y));
    }

    pub fn on_release(&mut self) -> Changes {
        self.last_pos = None;
        self.seed = self.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
        std::mem::take(&mut self.accumulated)
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, brush: &BrushKind, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        if matches!(brush, BrushKind::Direction { .. }) { return Vec::new(); }
        self.cells_at(doc, x, y, brush, None, ctx)
    }

    fn draw_at(&mut self, doc: &mut Document, cx: usize, cy: usize, brush: &BrushKind, way: Option<(isize, isize)>, ctx: &DrawContext) {
        let cells = self.cells_at(doc, cx, cy, brush, way, ctx);
        let layer = doc.active_layer_mut();
        for (x, y, new) in cells {
            let Some(old) = layer.get(x, y).cloned() else { continue };
            layer.set(x, y, new.clone());
            merge_changes(&mut self.accumulated, Changes::from([((x, y), (old, new))]));
        }
    }

    /// The cells the brush puts down at (cx, cy) (mirrored if symmetry is on), as they will look.
    fn cells_at(&self, doc: &Document, cx: usize, cy: usize, brush: &BrushKind, way: Option<(isize, isize)>, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let (w, h) = (doc.width, doc.height);
        let layer = doc.active_layer();
        let painted = |stamps: Vec<(usize, usize, char)>| -> Vec<(usize, usize, Cell)> {
            ctx.mirror
                .stamps(stamps, w, h)
                .into_iter()
                .filter_map(|(x, y, ch)| layer.get(x, y).map(|old| (x, y, ctx.paint(old, ch))))
                .collect()
        };
        match brush {
            BrushKind::Square => {
                let glyph = ctx.current_char();
                painted(brush_positions(cx, cy, ctx.brush_size).into_iter().map(|(x, y)| (x, y, glyph)).collect())
            }
            BrushKind::Scatter { glyphs, density } => {
                let glyphs: Vec<char> = glyphs.chars().collect();
                if glyphs.is_empty() { return Vec::new(); }
                let stamps = brush_positions(cx, cy, ctx.brush_size)
                    .into_iter()
                    .filter_map(|(x, y)| {
                        let roll = hash(x, y, self.seed);
                        let hit = (roll >> 11) as f64 / (1u64 << 53) as f64 <= *density as f64;
                        hit.then(|| (x, y, glyphs[(hash(y, x, !self.seed) % glyphs.len() as u64) as usize]))
                    })
                    .collect();
                painted(stamps)
            }
            BrushKind::Direction { glyphs } => {
                let glyphs: Vec<char> = glyphs.chars().collect();
                let Some(way) = way else { return Vec::new() };
                let Some(&glyph) = glyphs.get(direction(way)) else { return Vec::new() };
                painted(vec![(cx, cy, glyph)])
            }
            BrushKind::Stamp { cells, recolor } => {
                let (bh, bw) = (cells.len(), cells.first().map_or(0, Vec::len));
                let (ox, oy) = (cx as isize - (bw / 2) as isize, cy as isize - (bh / 2) as isize);
                let mut placed = Vec::new();
                for (dy, row) in cells.iter().enumerate() {
                    for (dx, cell) in row.iter().enumerate() {
                        let (x, y) = (ox + dx as isize, oy + dy as isize);
                        if cell.is_empty() || x < 0 || y < 0 { continue; }
                        placed.push((x as usize, y as usize, cell.clone()));
                    }
                }
                ctx.mirror
                    .cells(placed, w, h)
                    .into_iter()
                    .filter_map(|(x, y, cell)| {
                        let old = layer.get(x, y)?;
                        let new = if *recolor { ctx.paint(old, cell.ch) } else { ctx.channels.take(old, &cell) };
                        Some((x, y, new))
                    })
                    .collect()
            }
        }
    }
}

/// Which of horizontal (0), vertical (1), rising (2) or falling (3) a move of `(dx, dy)` cells is,
/// as it looks on screen: a cell is twice as tall as wide.
pub fn direction((dx, dy): (isize, isize)) -> usize {
    let angle = (-(dy as f32) * 2.0).atan2(dx as f32).to_degrees().rem_euclid(180.0);
    match angle {
        a if !(22.5..157.5).contains(&a) => 0,
        a if (67.5..112.5).contains(&a) => 1,
        a if a < 90.0 => 2,
        _ => 3,
    }
}

fn hash(x: usize, y: usize, seed: u64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed;
    h ^= h >> 31;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^ (h >> 29)
}
