//! Line, rectangle, ellipse and border box: drag from one corner to the other.

use crate::document::{Cell, Document};
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
        let stamps = ctx.mirror.stamps(stamps(shape, a, b, ctx.current_char()), doc.width, doc.height);
        preview_stamps(doc, &stamps, ctx)
    }

    pub fn on_release(&mut self, doc: &mut Document, shape: Shape, end: (usize, usize), mods: ShapeMods, ctx: &DrawContext) -> Changes {
        let Some(start) = self.start.take() else { return Changes::new(); };
        let (a, b) = shape_box(shape, start, end, mods, doc);
        let stamps = ctx.mirror.stamps(stamps(shape, a, b, ctx.current_char()), doc.width, doc.height);
        apply_stamps(doc, &stamps, ctx)
    }
}

/// The two corners (or line ends) after applying the modifiers, inside the document.
pub fn shape_box(shape: Shape, start: (usize, usize), end: (usize, usize), mods: ShapeMods, doc: &Document) -> ((usize, usize), (usize, usize)) {
    let sign = |v: isize| if v < 0 { -1 } else { 1 };
    let (sx, sy) = (start.0 as isize, start.1 as isize);
    let (mut dx, mut dy) = (end.0 as isize - sx, end.1 as isize - sy);

    if mods.constrain {
        if shape == Shape::Line {
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
fn stamps(shape: Shape, a: (usize, usize), b: (usize, usize), glyph: char) -> Vec<(usize, usize, char)> {
    let with_glyph = |points: Vec<(usize, usize)>| points.into_iter().map(|(x, y)| (x, y, glyph)).collect();
    match shape {
        Shape::Line => with_glyph(bresenham(a.0 as isize, a.1 as isize, b.0 as isize, b.1 as isize)),
        Shape::Rect { filled } => with_glyph(rect_points(a.0, a.1, b.0, b.1, filled)),
        Shape::Ellipse { filled } => with_glyph(ellipse_points(a.0, a.1, b.0, b.1, filled)),
        Shape::Border(style) => style.frame(a, b),
    }
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
