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

fn ellipse_points(x1: usize, y1: usize, x2: usize, y2: usize, filled: bool) -> Vec<(usize, usize)> {
    use std::collections::HashSet;
    let mut pts: HashSet<(usize, usize)> = HashSet::new();

    let cx = ((x1 + x2) / 2) as isize;
    let cy = ((y1 + y2) / 2) as isize;
    let rx = ((x2.max(x1) - x2.min(x1)) / 2) as isize;
    let ry = ((y2.max(y1) - y2.min(y1)) / 2) as isize;

    if rx == 0 || ry == 0 {
        return vec![];
    }

    let add = |pts: &mut HashSet<(usize, usize)>, x: isize, y: isize, filled: bool, cx: isize, cy: isize, _rx: isize| {
        let add_pt = |pts: &mut HashSet<(usize, usize)>, px: isize, py: isize| {
            if px >= 0 && py >= 0 { pts.insert((px as usize, py as usize)); }
        };
        if filled {
            for fx in (cx - x)..=(cx + x) {
                add_pt(pts, fx, cy + y);
                add_pt(pts, fx, cy - y);
            }
        } else {
            add_pt(pts, cx + x, cy + y);
            add_pt(pts, cx - x, cy + y);
            add_pt(pts, cx + x, cy - y);
            add_pt(pts, cx - x, cy - y);
        }
    };

    let rx2 = rx * rx;
    let ry2 = ry * ry;

    let mut x = 0isize;
    let mut y = ry;
    let mut d1 = ry2 - rx2 * ry + rx2 / 4;
    let mut dx = 2 * ry2 * x;
    let mut dy = 2 * rx2 * y;

    while dx < dy {
        add(&mut pts, x, y, filled, cx, cy, rx);
        if d1 < 0 {
            x += 1; dx += 2 * ry2; d1 += dx + ry2;
        } else {
            x += 1; y -= 1; dx += 2 * ry2; dy -= 2 * rx2; d1 += dx - dy + ry2;
        }
    }

    let mut d2 = ry2 * (x * x) + rx2 * (y - 1) * (y - 1) - rx2 * ry2;
    while y >= 0 {
        add(&mut pts, x, y, filled, cx, cy, rx);
        if d2 > 0 {
            y -= 1; dy -= 2 * rx2; d2 += rx2 - dy;
        } else {
            y -= 1; x += 1; dx += 2 * ry2; dy -= 2 * rx2; d2 += dx - dy + rx2;
        }
    }

    pts.into_iter().collect()
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