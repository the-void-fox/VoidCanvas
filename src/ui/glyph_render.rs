//! Draws box-drawing lines, block elements and braille as shapes instead of font glyphs,
//! so they fill their cell exactly and join their neighbours seamlessly at any zoom.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

use crate::glyphs::BOX_ARMS;

/// Rounds a rect to physical pixels, so neighbouring cells share exact edges.
pub fn snap_rect(rect: Rect, pixels_per_point: f32) -> Rect {
    let snap = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(pos2(snap(rect.min.x), snap(rect.min.y)), pos2(snap(rect.max.x), snap(rect.max.y)))
}

/// Line widths shared by every cell on screen.
///
/// Snapped cells differ by a pixel in size (24 vs 25 px at 241 %); deriving widths per cell
/// would make some columns' lines a pixel thicker, so they come from the nominal cell size.
#[derive(Clone, Copy)]
pub struct Strokes {
    /// Light line width, in physical pixels.
    light: f32,
    /// Space between the two strokes of a double line, in physical pixels.
    gap: f32,
    /// Braille dot radius, in physical pixels.
    dot: f32,
}

impl Strokes {
    /// For cells `cell_width` points wide (before snapping).
    pub fn new(cell_width: f32, pixels_per_point: f32) -> Self {
        let width = cell_width * pixels_per_point;
        let light = (width / 10.0).round().max(1.0);
        Self { light, gap: (width / 6.0).round().max(light), dot: (width * 0.14).max(0.6) }
    }
}

/// Draws `ch` into the (snapped) cell `rect`. Returns `false` if the font should draw it.
pub fn draw(painter: &Painter, ch: char, rect: Rect, color: Color32, strokes: Strokes) -> bool {
    let px = Px::new(painter, rect, color, strokes);
    match ch as u32 {
        0x2500..=0x257F => px.box_drawing(ch as usize - 0x2500),
        0x2580..=0x2590 | 0x2594..=0x259F => px.block(ch as u32),
        0x2800..=0x28FF => px.braille(ch as u32 - 0x2800),
        _ => return false,
    }
    true
}

const UP: usize = 0;
const RIGHT: usize = 1;
const DOWN: usize = 2;
const LEFT: usize = 3;

/// A cell in physical pixels.
struct Px<'a> {
    painter: &'a Painter,
    color: Color32,
    ppp: f32,
    x: (f32, f32),
    y: (f32, f32),
    light: f32,
    gap: f32,
    dot: f32,
}

impl<'a> Px<'a> {
    fn new(painter: &'a Painter, rect: Rect, color: Color32, strokes: Strokes) -> Self {
        let ppp = painter.pixels_per_point();
        let x = (rect.min.x * ppp, rect.max.x * ppp);
        let y = (rect.min.y * ppp, rect.max.y * ppp);
        let Strokes { light, gap, dot } = strokes;
        Self { painter, color, ppp, x, y, light, gap, dot }
    }

    fn fill(&self, x: (f32, f32), y: (f32, f32)) {
        let rect = Rect::from_min_max(
            pos2(x.0 / self.ppp, y.0 / self.ppp),
            pos2(x.1 / self.ppp, y.1 / self.ppp),
        );
        self.painter.rect_filled(rect, 0.0, self.color);
    }

    fn point(&self, x: f32, y: f32) -> Pos2 {
        pos2(x / self.ppp, y / self.ppp)
    }

    /// Width across a line of the given weight (a double line: both strokes and the gap).
    fn thickness(&self, weight: u8) -> f32 {
        match weight {
            2 => self.light * 2.0,
            3 => self.light * 2.0 + self.gap,
            _ => self.light,
        }
    }

    // ── Box drawing ──────────────────────────────────────────────────────────

    fn box_drawing(&self, index: usize) {
        let arms = BOX_ARMS[index];
        if arms == [0; 4] {
            self.box_special(0x2500 + index as u32);
            return;
        }
        for dir in [UP, RIGHT, DOWN, LEFT] {
            if arms[dir] != 0 {
                self.arm(&arms, dir);
            }
        }
    }

    /// One arm, from the cell edge in direction `dir` to the junction in the middle.
    fn arm(&self, arms: &[u8; 4], dir: usize) {
        let horizontal = dir == LEFT || dir == RIGHT;
        let (along, across) = if horizontal { (self.x, self.y) } else { (self.y, self.x) };
        let (side_lo, side_hi) = if horizontal { (UP, DOWN) } else { (LEFT, RIGHT) };
        let toward_hi = dir == RIGHT || dir == DOWN;
        let weight = arms[dir];

        // Lines crossing ours occupy `perp` on our axis (our own width if there are none).
        let perp_weight = arms[side_lo].max(arms[side_hi]);
        let perp = band(along, self.thickness(if perp_weight == 0 { weight } else { perp_weight }));
        // Reach across the crossing lines, or stop at the double line's stroke nearest to us.
        let far = if toward_hi { perp.0 } else { perp.1 };
        let near_stroke = if toward_hi { perp.1 - self.light } else { perp.0 + self.light };
        let span = |reach: f32| if toward_hi { (reach, along.1) } else { (along.0, reach) };

        let draw = |reach: f32, cross: (f32, f32)| {
            let along = span(reach);
            if horizontal { self.fill(along, cross) } else { self.fill(cross, along) }
        };

        if weight != 3 {
            // A T onto a double line meets its near stroke; corners and crossings go all the way.
            let t_onto_double = perp_weight == 3
                && arms[(dir + 2) % 4] == 0
                && arms[side_lo] == 3
                && arms[side_hi] == 3;
            draw(if t_onto_double { near_stroke } else { far }, band(across, self.thickness(weight)));
        } else {
            let (lo, hi) = band(across, self.thickness(3));
            for (stroke, side) in [((lo, lo + self.light), side_lo), ((hi - self.light, hi), side_hi)] {
                // A stroke facing another double line turns into it; otherwise it runs past the junction.
                let reach = if arms[side] == 3 { near_stroke } else { far };
                draw(reach, stroke);
            }
        }
    }

    /// Dashed lines, rounded corners and diagonals.
    fn box_special(&self, cp: u32) {
        let (dashes, heavy, vertical) = match cp {
            0x2504 => (3, false, false), 0x2505 => (3, true, false),
            0x2506 => (3, false, true),  0x2507 => (3, true, true),
            0x2508 => (4, false, false), 0x2509 => (4, true, false),
            0x250A => (4, false, true),  0x250B => (4, true, true),
            0x254C => (2, false, false), 0x254D => (2, true, false),
            0x254E => (2, false, true),  0x254F => (2, true, true),
            _ => return self.box_curve(cp),
        };
        let thick = self.thickness(if heavy { 2 } else { 1 });
        let (along, across) = if vertical { (self.y, self.x) } else { (self.x, self.y) };
        let cross = band(across, thick);
        let step = (along.1 - along.0) / dashes as f32;
        for i in 0..dashes {
            let start = (along.0 + step * (i as f32 + 0.2)).round();
            let end = (along.0 + step * (i as f32 + 0.8)).round().max(start + 1.0);
            if vertical { self.fill(cross, (start, end)) } else { self.fill((start, end), cross) }
        }
    }

    fn box_curve(&self, cp: u32) {
        let stroke = Stroke::new(self.light / self.ppp, self.color);
        let (x0, x1, y0, y1) = (self.x.0, self.x.1, self.y.0, self.y.1);
        match cp {
            0x2571 => { self.painter.line_segment([self.point(x1, y0), self.point(x0, y1)], stroke); }
            0x2572 => { self.painter.line_segment([self.point(x0, y0), self.point(x1, y1)], stroke); }
            0x2573 => {
                self.painter.line_segment([self.point(x1, y0), self.point(x0, y1)], stroke);
                self.painter.line_segment([self.point(x0, y0), self.point(x1, y1)], stroke);
            }
            0x256D..=0x2570 => {
                // Centre of a light line, so arcs meet straight lines exactly.
                let cx = band(self.x, self.light);
                let cy = band(self.y, self.light);
                let (cx, cy) = ((cx.0 + cx.1) / 2.0, (cy.0 + cy.1) / 2.0);
                let r = (x1 - x0).min(y1 - y0) / 2.0;
                // Arc toward (dx, dy): ╭ right+down, ╮ left+down, ╯ left+up, ╰ right+up.
                let (dx, dy) = match cp { 0x256D => (1.0, 1.0), 0x256E => (-1.0, 1.0), 0x256F => (-1.0, -1.0), _ => (1.0, -1.0) };
                let edge_x = if dx > 0.0 { x1 } else { x0 };
                let edge_y = if dy > 0.0 { y1 } else { y0 };
                let mut points = vec![self.point(edge_x, cy)];
                const STEPS: usize = 12;
                for i in 0..=STEPS {
                    let a = std::f32::consts::FRAC_PI_2 * i as f32 / STEPS as f32;
                    points.push(self.point(cx + dx * r * (1.0 - a.sin()), cy + dy * r * (1.0 - a.cos())));
                }
                points.push(self.point(cx, edge_y));
                self.painter.add(Shape::line(points, stroke));
            }
            _ => {}
        }
    }

    // ── Blocks ───────────────────────────────────────────────────────────────

    /// Fills the part of the cell between fractions (fx0, fy0) and (fx1, fy1).
    fn part(&self, fx0: f32, fy0: f32, fx1: f32, fy1: f32) {
        let at = |(lo, hi): (f32, f32), f: f32| (lo + (hi - lo) * f).round();
        self.fill((at(self.x, fx0), at(self.x, fx1)), (at(self.y, fy0), at(self.y, fy1)));
    }

    fn block(&self, cp: u32) {
        match cp {
            0x2580 => self.part(0.0, 0.0, 1.0, 0.5),
            // ▁ … █: lower eighths
            0x2581..=0x2588 => self.part(0.0, 1.0 - (cp - 0x2580) as f32 / 8.0, 1.0, 1.0),
            // ▉ … ▏: left eighths
            0x2589..=0x258F => self.part(0.0, 0.0, (0x2590 - cp) as f32 / 8.0, 1.0),
            0x2590 => self.part(0.5, 0.0, 1.0, 1.0),
            0x2594 => self.part(0.0, 0.0, 1.0, 0.125),
            0x2595 => self.part(0.875, 0.0, 1.0, 1.0),
            _ => {
                // Quadrants ▖▗▘▙▚▛▜▝▞▟ as [upper left, upper right, lower left, lower right].
                let q: [bool; 4] = match cp {
                    0x2596 => [false, false, true, false],
                    0x2597 => [false, false, false, true],
                    0x2598 => [true, false, false, false],
                    0x2599 => [true, false, true, true],
                    0x259A => [true, false, false, true],
                    0x259B => [true, true, true, false],
                    0x259C => [true, true, false, true],
                    0x259D => [false, true, false, false],
                    0x259E => [false, true, true, false],
                    _      => [false, true, true, true],
                };
                for (i, on) in q.into_iter().enumerate() {
                    if on {
                        let (fx, fy) = ((i % 2) as f32 * 0.5, (i / 2) as f32 * 0.5);
                        self.part(fx, fy, fx + 0.5, fy + 0.5);
                    }
                }
            }
        }
    }

    // ── Braille ──────────────────────────────────────────────────────────────

    fn braille(&self, bits: u32) {
        // Bit → (column, row) of the 2×4 dot grid.
        const DOTS: [(f32, f32); 8] = [(0.0, 0.0), (0.0, 1.0), (0.0, 2.0), (1.0, 0.0), (1.0, 1.0), (1.0, 2.0), (0.0, 3.0), (1.0, 3.0)];
        let (w, h) = (self.x.1 - self.x.0, self.y.1 - self.y.0);
        let radius = self.dot / self.ppp;
        for (bit, (col, row)) in DOTS.iter().enumerate() {
            if bits & (1 << bit) != 0 {
                let center = self.point(self.x.0 + w * (col + 0.5) / 2.0, self.y.0 + h * (row + 0.5) / 4.0);
                self.painter.circle_filled(center, radius, self.color);
            }
        }
    }
}

/// A line `thickness` wide centred in `range`, aligned to whole pixels.
fn band(range: (f32, f32), thickness: f32) -> (f32, f32) {
    let start = ((range.0 + range.1 - thickness) / 2.0).floor();
    (start, start + thickness)
}
