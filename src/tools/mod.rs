pub mod brush;
pub mod eraser;
pub mod fill;
pub mod shapes;
pub mod border;
pub mod selection;

use crate::document::Color;
pub use crate::document::Changes;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolType {
    Brush,
    Eraser,
    Fill,
    Rectangle,
    Ellipse,
    Line,
    Border,
    Select,
}

impl ToolType {
    pub fn all() -> &'static [ToolType] {
        &[
            ToolType::Brush,
            ToolType::Eraser,
            ToolType::Fill,
            ToolType::Rectangle,
            ToolType::Ellipse,
            ToolType::Line,
            ToolType::Border,
            ToolType::Select,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ToolType::Brush     => "󰃣",
            ToolType::Eraser    => "",
            ToolType::Fill      => "",
            ToolType::Rectangle => "󰹞",
            ToolType::Ellipse   => "󰺠",
            ToolType::Line      => "󰕞",
            ToolType::Border    => "󰢡",
            ToolType::Select    => "󰒅",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ToolType::Brush     => "Brush",
            ToolType::Eraser    => "Eraser",
            ToolType::Fill      => "Fill",
            ToolType::Rectangle => "Rectangle",
            ToolType::Ellipse   => "Ellipse",
            ToolType::Line      => "Line",
            ToolType::Border    => "Border",
            ToolType::Select    => "Select",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DrawContext {
    pub primary_char: char,
    pub secondary_char: char,
    pub primary_fg: Color,
    pub primary_bg: Color,
    pub secondary_fg: Color,
    pub secondary_bg: Color,
    pub brush_size: usize,
    pub is_primary: bool,
}

impl DrawContext {
    pub fn current_char(&self) -> char {
        if self.is_primary { self.primary_char } else { self.secondary_char }
    }
    pub fn current_fg(&self) -> Color {
        if self.is_primary { self.primary_fg.clone() } else { self.secondary_fg.clone() }
    }
    pub fn current_bg(&self) -> Color {
        if self.is_primary { self.primary_bg.clone() } else { self.secondary_bg.clone() }
    }
}

/// Folds `more` into `into`, keeping the earliest "before" and the latest "after" of each cell.
pub fn merge_changes(into: &mut Changes, more: Changes) {
    for (pos, (old, new)) in more {
        into.entry(pos)
            .and_modify(|entry| entry.1 = new.clone())
            .or_insert((old, new));
    }
}

pub fn brush_positions(cx: usize, cy: usize, size: usize) -> Vec<(usize, usize)> {
    let mut positions = Vec::new();
    let half = (size / 2) as isize;
    let cx = cx as isize;
    let cy = cy as isize;
    for dy in -half..=half {
        for dx in -half..=half {
            let x = cx + dx;
            let y = cy + dy;
            if x >= 0 && y >= 0 {
                positions.push((x as usize, y as usize));
            }
        }
    }
    positions
}

pub fn bresenham(x0: isize, y0: isize, x1: isize, y1: isize) -> Vec<(usize, usize)> {
    let mut points = Vec::new();
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    loop {
        if x >= 0 && y >= 0 {
            points.push((x as usize, y as usize));
        }
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 > -dy { err -= dy; x += sx; }
        if e2 < dx  { err += dx; y += sy; }
    }
    points
}