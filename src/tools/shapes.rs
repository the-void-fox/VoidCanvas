//! Line, rectangle, ellipse and border box: drag from one corner to the other.

use crate::document::{Cell, Document, Layer};
use crate::glyphs::{box_arms, from_arms};
use super::{Changes, DrawContext, apply_stamps, bresenham, border::BorderStyle, preview_stamps};

/// Modifier keys while dragging a shape.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShapeMods {
    /// Shift: a visually square box, or a horizontal / vertical / diagonal line.
    pub constrain: bool,
    /// Alt: the press point is the centre.
    pub from_center: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Line,
    /// An L-shaped line of box-drawing glyphs that joins existing lines.
    BoxLine(BorderStyle),
    Rect { filled: bool },
    Ellipse { filled: bool },
    Border(BorderStyle),
}

#[derive(Default, Debug)]
pub struct ShapeTool {
    pub start: Option<(usize, usize)>,
}

impl ShapeTool {
    pub fn on_press(&mut self, x: usize, y: usize) {
        self.start = Some((x, y));
    }

    pub fn preview(&self, doc: &Document, shape: Shape, end: (usize, usize), mods: ShapeMods, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let Some(start) = self.start else { return vec![]; };
        let (a, b) = shape_box(shape, start, end, mods, doc);
        let stamps = stamps(shape, a, b, ctx.current_char(), doc.active_layer());
        preview_stamps(doc, &ctx.mirror.stamps(stamps, doc.width, doc.height), ctx)
    }

    pub fn on_release(&mut self, doc: &mut Document, shape: Shape, end: (usize, usize), mods: ShapeMods, ctx: &DrawContext) -> Changes {
        let Some(start) = self.start.take() else { return Changes::new(); };
        let (a, b) = shape_box(shape, start, end, mods, doc);
        let stamps = stamps(shape, a, b, ctx.current_char(), doc.active_layer());
        apply_stamps(doc, &ctx.mirror.stamps(stamps, doc.width, doc.height), ctx)
    }
}

/// The two corners (or line ends) after applying the modifiers, inside the document.
pub fn shape_box(shape: Shape, start: (usize, usize), end: (usize, usize), mods: ShapeMods, doc: &Document) -> ((usize, usize), (usize, usize)) {
    let sign = |v: isize| if v < 0 { -1 } else { 1 };
    let (sx, sy) = (start.0 as isize, start.1 as isize);
    let (mut dx, mut dy) = (end.0 as isize - sx, end.1 as isize - sy);

    if mods.constrain {
        if matches!(shape, Shape::Line | Shape::BoxLine(_)) {
            let (ax, ay) = (dx.abs(), dy.abs());
            if ay * 2 <= ax {
                dy = 0;
            } else if ax * 2 <= ay {
                dx = 0;
            } else {
                let d = (ax + ay) / 2;
                (dx, dy) = (sign(dx) * d, sign(dy) * d);
            }
        } else if mods.from_center {
            // Cells are twice as tall as wide: a square is twice as many columns as rows.
            let half_h = dy.abs().max((dx.abs() + 1) / 2);
            (dx, dy) = (sign(dx) * 2 * half_h, sign(dy) * half_h);
        } else {
            let h = (dy.abs() + 1).max((dx.abs() + 2) / 2);
            (dx, dy) = (sign(dx) * (2 * h - 1), sign(dy) * (h - 1));
        }
    }

    let (ax, ay) = if mods.from_center { (sx - dx, sy - dy) } else { (sx, sy) };
    let clamp = |x: isize, y: isize| {
        (
            x.clamp(0, doc.width.saturating_sub(1) as isize) as usize,
            y.clamp(0, doc.height.saturating_sub(1) as isize) as usize,
        )
    };
    (clamp(ax, ay), clamp(sx + dx, sy + dy))
}

/// Cells of the shape between corners `a` and `b`, with the glyph for each.
/// Box-drawing shapes join the lines already on `layer`.
fn stamps(shape: Shape, a: (usize, usize), b: (usize, usize), glyph: char, layer: &Layer) -> Vec<(usize, usize, char)> {
    let with_glyph = |points: Vec<(usize, usize)>| points.into_iter().map(|(x, y)| (x, y, glyph)).collect();
    match shape {
        Shape::Line => with_glyph(bresenham(a.0 as isize, a.1 as isize, b.0 as isize, b.1 as isize)),
        Shape::Rect { filled } => with_glyph(rect_points(a.0, a.1, b.0, b.1, filled)),
        Shape::Ellipse { filled } => with_glyph(ellipse_points(a.0, a.1, b.0, b.1, filled)),
        Shape::Border(style) => join_lines(frame_arms(a, b, style.weight()), layer, false),
        Shape::BoxLine(style) => join_lines(path_arms(a, b, style.weight()), layer, true),
    }
}

const UP: usize = 0;
const RIGHT: usize = 1;
const DOWN: usize = 2;
const LEFT: usize = 3;

/// Arms of every cell on the outline of the box between `a` and `b`.
fn frame_arms(a: (usize, usize), b: (usize, usize), weight: u8) -> Vec<((usize, usize), [u8; 4])> {
    let (min_x, max_x) = (a.0.min(b.0), a.0.max(b.0));
    let (min_y, max_y) = (a.1.min(b.1), a.1.max(b.1));
    let mut cells = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let on_side = x == min_x || x == max_x;
            let on_end = y == min_y || y == max_y;
            if !on_side && !on_end { continue; }
            let mut arms = [0; 4];
            if on_side && y > min_y { arms[UP] = weight; }
            if on_side && y < max_y { arms[DOWN] = weight; }
            if on_end && x > min_x { arms[LEFT] = weight; }
            if on_end && x < max_x { arms[RIGHT] = weight; }
            if arms != [0; 4] {
                cells.push(((x, y), arms));
            }
        }
    }
    cells
}

/// Arms along an L-shaped path: across to `b`'s column, then up or down to `b`.
fn path_arms(a: (usize, usize), b: (usize, usize), weight: u8) -> Vec<((usize, usize), [u8; 4])> {
    let mut path = vec![a];
    let (mut x, mut y) = a;
    while x != b.0 {
        x = if b.0 > x { x + 1 } else { x - 1 };
        path.push((x, y));
    }
    while y != b.1 {
        y = if b.1 > y { y + 1 } else { y - 1 };
        path.push((x, y));
    }

    let toward = |from: (usize, usize), to: (usize, usize)| {
        if to.0 > from.0 { RIGHT } else if to.0 < from.0 { LEFT } else if to.1 > from.1 { DOWN } else { UP }
    };
    if path.len() == 1 {
        let mut arms = [0; 4];
        (arms[LEFT], arms[RIGHT]) = (weight, weight);
        return vec![(a, arms)];
    }
    (0..path.len())
        .map(|i| {
            let mut arms = [0; 4];
            if i > 0 { arms[toward(path[i], path[i - 1])] = weight; }
            if i + 1 < path.len() { arms[toward(path[i], path[i + 1])] = weight; }
            (path[i], arms)
        })
        .collect()
}

/// Turns arms into glyphs, merged with the box lines already in each cell, so lines that
/// meet form ├ ┬ ┼. A line's free end with nothing to join runs to the edge of its cell.
fn join_lines(cells: Vec<((usize, usize), [u8; 4])>, layer: &Layer, extend_ends: bool) -> Vec<(usize, usize, char)> {
    cells
        .into_iter()
        .filter_map(|((x, y), mut arms)| {
            let existing = layer.get(x, y).and_then(|c| box_arms(c.ch)).unwrap_or([0; 4]);
            let is_end = arms.iter().filter(|&&w| w > 0).count() == 1;
            if extend_ends && is_end && existing == [0; 4] {
                let dir = arms.iter().position(|&w| w > 0).unwrap();
                arms[(dir + 2) % 4] = arms[dir];
            }
            let merged: [u8; 4] = std::array::from_fn(|d| if arms[d] > 0 { arms[d] } else { existing[d] });
            let ch = from_arms(merged).or_else(|| from_arms(arms))?;
            Some((x, y, ch))
        })
        .collect()
}

fn rect_points(x1: usize, y1: usize, x2: usize, y2: usize, filled: bool) -> Vec<(usize, usize)> {
    let (min_x, max_x) = (x1.min(x2), x1.max(x2));
    let (min_y, max_y) = (y1.min(y2), y1.max(y2));
    let mut pts = Vec::new();

    if filled {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                pts.push((x, y));
            }
        }
    } else {
        for x in min_x..=max_x {
            pts.push((x, min_y));
            pts.push((x, max_y));
        }
        for y in (min_y + 1)..max_y {
            pts.push((min_x, y));
            pts.push((max_x, y));
        }
    }
    pts
}
/// Cells of the ellipse inscribed in the box (x1, y1)–(x2, y2); it touches all four box edges.
pub(crate) fn ellipse_points(x1: usize, y1: usize, x2: usize, y2: usize, filled: bool) -> Vec<(usize, usize)> {
    let (min_x, max_x) = (x1.min(x2), x1.max(x2));
    let (min_y, max_y) = (y1.min(y2), y1.max(y2));
    let (w, h) = (max_x - min_x + 1, max_y - min_y + 1);
    // Cell centres relative to the ellipse centre are `i - c`.
    let (cx, cy) = ((w - 1) as f32 / 2.0, (h - 1) as f32 / 2.0);
    // Slightly inside the box, so small boxes get round corners.
    let (rx, ry) = (w as f32 / 2.0 - 0.1, h as f32 / 2.0 - 0.1);

    // A cell is in if its centre is inside, or it lies on one of the axes
    // (that keeps very thin ellipses touching the box ends).
    let is_in = |i: isize, j: isize| {
        if i < 0 || j < 0 || i >= w as isize || j >= h as isize { return false; }
        let dx = i as f32 - cx;
        let dy = j as f32 - cy;
        dx.abs() <= 0.5 || dy.abs() <= 0.5 || (dx / rx).powi(2) + (dy / ry).powi(2) <= 1.0
    };

    let mut pts = Vec::new();
    for j in 0..h as isize {
        for i in 0..w as isize {
            if !is_in(i, j) { continue; }
            let on_edge = !is_in(i - 1, j) || !is_in(i + 1, j) || !is_in(i, j - 1) || !is_in(i, j + 1);
            if filled || on_edge {
                pts.push((min_x + i as usize, min_y + j as usize));
            }
        }
    }
    pts
}
