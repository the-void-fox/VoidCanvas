use crate::document::{Cell, Document};
use super::{Changes, DrawContext, bresenham};

#[derive(Default, Debug)]
pub struct RectangleTool {
    pub start: Option<(usize, usize)>,
    pub filled: bool,
}

impl RectangleTool {
    pub fn on_press(&mut self, x: usize, y: usize) {
        self.start = Some((x, y));
    }

    pub fn on_release(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((sx, sy)) = self.start.take() else { return Changes::new(); };
        let points = rect_points(sx, sy, x, y, self.filled);
        apply_points(doc, &points, ctx)
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let Some((sx, sy)) = self.start else { return vec![]; };
        let cell = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());
        rect_points(sx, sy, x, y, self.filled)
            .into_iter()
            .filter(|&(px, py)| px < doc.width && py < doc.height)
            .map(|(px, py)| (px, py, cell.clone()))
            .collect()
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

#[derive(Default, Debug)]
pub struct EllipseTool {
    pub start: Option<(usize, usize)>,
    pub filled: bool,
}

impl EllipseTool {
    pub fn on_press(&mut self, x: usize, y: usize) { self.start = Some((x, y)); }

    pub fn on_release(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((sx, sy)) = self.start.take() else { return Changes::new(); };
        let points = ellipse_points(sx, sy, x, y, self.filled);
        apply_points(doc, &points, ctx)
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let Some((sx, sy)) = self.start else { return vec![]; };
        let cell = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());
        ellipse_points(sx, sy, x, y, self.filled)
            .into_iter()
            .filter(|&(px, py)| px < doc.width && py < doc.height)
            .map(|(px, py)| (px, py, cell.clone()))
            .collect()
    }
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

#[derive(Default, Debug)]
pub struct LineTool {
    pub start: Option<(usize, usize)>,
}

impl LineTool {
    pub fn on_press(&mut self, x: usize, y: usize) { self.start = Some((x, y)); }

    pub fn on_release(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((sx, sy)) = self.start.take() else { return Changes::new(); };
        let points = bresenham(sx as isize, sy as isize, x as isize, y as isize);
        let valid: Vec<_> = points.into_iter().filter(|&(px, py)| px < doc.width && py < doc.height).collect();
        apply_points(doc, &valid, ctx)
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let Some((sx, sy)) = self.start else { return vec![]; };
        let cell = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());
        bresenham(sx as isize, sy as isize, x as isize, y as isize)
            .into_iter()
            .filter(|&(px, py)| px < doc.width && py < doc.height)
            .map(|(px, py)| (px, py, cell.clone()))
            .collect()
    }
}

fn apply_points(doc: &mut Document, points: &[(usize, usize)], ctx: &DrawContext) -> Changes {
    let mut changes = Changes::new();
    let new_cell = Cell::new(ctx.current_char(), ctx.current_fg(), ctx.current_bg());
    let layer = doc.active_layer_mut();

    for &(px, py) in points {
        if px >= layer.width || py >= layer.height { continue; }
        let old = layer.get(px, py).cloned().unwrap_or_default();
        layer.set(px, py, new_cell.clone());
        changes.insert((px, py), (old, new_cell.clone()));
    }
    changes
}