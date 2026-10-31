//! Draws box-drawing lines, block elements and braille as shapes instead of font glyphs,
//! so they fill their cell exactly and join their neighbours seamlessly at any zoom.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

/// Rounds a rect to physical pixels, so neighbouring cells share exact edges.
pub fn snap_rect(rect: Rect, pixels_per_point: f32) -> Rect {
    let snap = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    Rect::from_min_max(pos2(snap(rect.min.x), snap(rect.min.y)), pos2(snap(rect.max.x), snap(rect.max.y)))
}

/// Draws `ch` into the (snapped) cell `rect`. Returns `false` if the font should draw it.
pub fn draw(painter: &Painter, ch: char, rect: Rect, color: Color32) -> bool {
    let px = Px::new(painter, rect, color);
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
    /// Thickness of a light line.
    light: f32,
    /// Space between the two strokes of a double line.
    gap: f32,
}

impl<'a> Px<'a> {
    fn new(painter: &'a Painter, rect: Rect, color: Color32) -> Self {
        let ppp = painter.pixels_per_point();
        let x = (rect.min.x * ppp, rect.max.x * ppp);
        let y = (rect.min.y * ppp, rect.max.y * ppp);
        let width = x.1 - x.0;
        let light = (width / 10.0).round().max(1.0);
        let gap = (width / 6.0).round().max(light);
        Self { painter, color, ppp, x, y, light, gap }
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
        let radius = (w.min(h / 2.0) * 0.14).max(0.6) / self.ppp;
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

/// Arm weights of U+2500–U+257F as [up, right, down, left]:
/// 0 none, 1 light, 2 heavy, 3 double. All-zero rows are dashes, arcs and diagonals.
/// Generated from the Unicode character names.
#[rustfmt::skip]
const BOX_ARMS: [[u8; 4]; 128] = [
    [0, 1, 0, 1], // 2500 ─ light horizontal
    [0, 2, 0, 2], // 2501 ━ heavy horizontal
    [1, 0, 1, 0], // 2502 │ light vertical
    [2, 0, 2, 0], // 2503 ┃ heavy vertical
    [0, 0, 0, 0], // 2504 ┄ light triple dash horizontal
    [0, 0, 0, 0], // 2505 ┅ heavy triple dash horizontal
    [0, 0, 0, 0], // 2506 ┆ light triple dash vertical
    [0, 0, 0, 0], // 2507 ┇ heavy triple dash vertical
    [0, 0, 0, 0], // 2508 ┈ light quadruple dash horizontal
    [0, 0, 0, 0], // 2509 ┉ heavy quadruple dash horizontal
    [0, 0, 0, 0], // 250A ┊ light quadruple dash vertical
    [0, 0, 0, 0], // 250B ┋ heavy quadruple dash vertical
    [0, 1, 1, 0], // 250C ┌ light down and right
    [0, 2, 1, 0], // 250D ┍ down light and right heavy
    [0, 1, 2, 0], // 250E ┎ down heavy and right light
    [0, 2, 2, 0], // 250F ┏ heavy down and right
    [0, 0, 1, 1], // 2510 ┐ light down and left
    [0, 0, 1, 2], // 2511 ┑ down light and left heavy
    [0, 0, 2, 1], // 2512 ┒ down heavy and left light
    [0, 0, 2, 2], // 2513 ┓ heavy down and left
    [1, 1, 0, 0], // 2514 └ light up and right
    [1, 2, 0, 0], // 2515 ┕ up light and right heavy
    [2, 1, 0, 0], // 2516 ┖ up heavy and right light
    [2, 2, 0, 0], // 2517 ┗ heavy up and right
    [1, 0, 0, 1], // 2518 ┘ light up and left
    [1, 0, 0, 2], // 2519 ┙ up light and left heavy
    [2, 0, 0, 1], // 251A ┚ up heavy and left light
    [2, 0, 0, 2], // 251B ┛ heavy up and left
    [1, 1, 1, 0], // 251C ├ light vertical and right
    [1, 2, 1, 0], // 251D ┝ vertical light and right heavy
    [2, 1, 1, 0], // 251E ┞ up heavy and right down light
    [1, 1, 2, 0], // 251F ┟ down heavy and right up light
    [2, 1, 2, 0], // 2520 ┠ vertical heavy and right light
    [2, 2, 1, 0], // 2521 ┡ down light and right up heavy
    [1, 2, 2, 0], // 2522 ┢ up light and right down heavy
    [2, 2, 2, 0], // 2523 ┣ heavy vertical and right
    [1, 0, 1, 1], // 2524 ┤ light vertical and left
    [1, 0, 1, 2], // 2525 ┥ vertical light and left heavy
    [2, 0, 1, 1], // 2526 ┦ up heavy and left down light
    [1, 0, 2, 1], // 2527 ┧ down heavy and left up light
    [2, 0, 2, 1], // 2528 ┨ vertical heavy and left light
    [2, 0, 1, 2], // 2529 ┩ down light and left up heavy
    [1, 0, 2, 2], // 252A ┪ up light and left down heavy
    [2, 0, 2, 2], // 252B ┫ heavy vertical and left
    [0, 1, 1, 1], // 252C ┬ light down and horizontal
    [0, 1, 1, 2], // 252D ┭ left heavy and right down light
    [0, 2, 1, 1], // 252E ┮ right heavy and left down light
    [0, 2, 1, 2], // 252F ┯ down light and horizontal heavy
    [0, 1, 2, 1], // 2530 ┰ down heavy and horizontal light
    [0, 1, 2, 2], // 2531 ┱ right light and left down heavy
    [0, 2, 2, 1], // 2532 ┲ left light and right down heavy
    [0, 2, 2, 2], // 2533 ┳ heavy down and horizontal
    [1, 1, 0, 1], // 2534 ┴ light up and horizontal
    [1, 1, 0, 2], // 2535 ┵ left heavy and right up light
    [1, 2, 0, 1], // 2536 ┶ right heavy and left up light
    [1, 2, 0, 2], // 2537 ┷ up light and horizontal heavy
    [2, 1, 0, 1], // 2538 ┸ up heavy and horizontal light
    [2, 1, 0, 2], // 2539 ┹ right light and left up heavy
    [2, 2, 0, 1], // 253A ┺ left light and right up heavy
    [2, 2, 0, 2], // 253B ┻ heavy up and horizontal
    [1, 1, 1, 1], // 253C ┼ light vertical and horizontal
    [1, 1, 1, 2], // 253D ┽ left heavy and right vertical light
    [1, 2, 1, 1], // 253E ┾ right heavy and left vertical light
    [1, 2, 1, 2], // 253F ┿ vertical light and horizontal heavy
    [2, 1, 1, 1], // 2540 ╀ up heavy and down horizontal light
    [1, 1, 2, 1], // 2541 ╁ down heavy and up horizontal light
    [2, 1, 2, 1], // 2542 ╂ vertical heavy and horizontal light
    [2, 1, 1, 2], // 2543 ╃ left up heavy and right down light
    [2, 2, 1, 1], // 2544 ╄ right up heavy and left down light
    [1, 1, 2, 2], // 2545 ╅ left down heavy and right up light
    [1, 2, 2, 1], // 2546 ╆ right down heavy and left up light
    [2, 2, 1, 2], // 2547 ╇ down light and up horizontal heavy
    [1, 2, 2, 2], // 2548 ╈ up light and down horizontal heavy
    [2, 1, 2, 2], // 2549 ╉ right light and left vertical heavy
    [2, 2, 2, 1], // 254A ╊ left light and right vertical heavy
    [2, 2, 2, 2], // 254B ╋ heavy vertical and horizontal
    [0, 0, 0, 0], // 254C ╌ light double dash horizontal
    [0, 0, 0, 0], // 254D ╍ heavy double dash horizontal
    [0, 0, 0, 0], // 254E ╎ light double dash vertical
    [0, 0, 0, 0], // 254F ╏ heavy double dash vertical
    [0, 3, 0, 3], // 2550 ═ double horizontal
    [3, 0, 3, 0], // 2551 ║ double vertical
    [0, 3, 1, 0], // 2552 ╒ down single and right double
    [0, 1, 3, 0], // 2553 ╓ down double and right single
    [0, 3, 3, 0], // 2554 ╔ double down and right
    [0, 0, 1, 3], // 2555 ╕ down single and left double
    [0, 0, 3, 1], // 2556 ╖ down double and left single
    [0, 0, 3, 3], // 2557 ╗ double down and left
    [1, 3, 0, 0], // 2558 ╘ up single and right double
    [3, 1, 0, 0], // 2559 ╙ up double and right single
    [3, 3, 0, 0], // 255A ╚ double up and right
    [1, 0, 0, 3], // 255B ╛ up single and left double
    [3, 0, 0, 1], // 255C ╜ up double and left single
    [3, 0, 0, 3], // 255D ╝ double up and left
    [1, 3, 1, 0], // 255E ╞ vertical single and right double
    [3, 1, 3, 0], // 255F ╟ vertical double and right single
    [3, 3, 3, 0], // 2560 ╠ double vertical and right
    [1, 0, 1, 3], // 2561 ╡ vertical single and left double
    [3, 0, 3, 1], // 2562 ╢ vertical double and left single
    [3, 0, 3, 3], // 2563 ╣ double vertical and left
    [0, 3, 1, 3], // 2564 ╤ down single and horizontal double
    [0, 1, 3, 1], // 2565 ╥ down double and horizontal single
    [0, 3, 3, 3], // 2566 ╦ double down and horizontal
    [1, 3, 0, 3], // 2567 ╧ up single and horizontal double
    [3, 1, 0, 1], // 2568 ╨ up double and horizontal single
    [3, 3, 0, 3], // 2569 ╩ double up and horizontal
    [1, 3, 1, 3], // 256A ╪ vertical single and horizontal double
    [3, 1, 3, 1], // 256B ╫ vertical double and horizontal single
    [3, 3, 3, 3], // 256C ╬ double vertical and horizontal
    [0, 0, 0, 0], // 256D ╭ light arc down and right
    [0, 0, 0, 0], // 256E ╮ light arc down and left
    [0, 0, 0, 0], // 256F ╯ light arc up and left
    [0, 0, 0, 0], // 2570 ╰ light arc up and right
    [0, 0, 0, 0], // 2571 ╱ light diagonal upper right to lower left
    [0, 0, 0, 0], // 2572 ╲ light diagonal upper left to lower right
    [0, 0, 0, 0], // 2573 ╳ light diagonal cross
    [0, 0, 0, 1], // 2574 ╴ light left
    [1, 0, 0, 0], // 2575 ╵ light up
    [0, 1, 0, 0], // 2576 ╶ light right
    [0, 0, 1, 0], // 2577 ╷ light down
    [0, 0, 0, 2], // 2578 ╸ heavy left
    [2, 0, 0, 0], // 2579 ╹ heavy up
    [0, 2, 0, 0], // 257A ╺ heavy right
    [0, 0, 2, 0], // 257B ╻ heavy down
    [0, 2, 0, 1], // 257C ╼ light left and heavy right
    [1, 0, 2, 0], // 257D ╽ light up and heavy down
    [0, 1, 0, 2], // 257E ╾ heavy left and light right
    [2, 0, 1, 0], // 257F ╿ heavy up and light down
];
