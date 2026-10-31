pub mod brush;
pub mod eraser;
pub mod fill;
pub mod shapes;
pub mod border;
pub mod selection;
pub mod text;

use crate::document::{Cell, Color, Document};
use crate::icons;
pub use crate::document::{Changes, merge_changes};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolType {
    Brush,
    Text,
    Eyedropper,
    Eraser,
    Fill,
    Rectangle,
    Ellipse,
    Line,
    Border,
    Select,
}

impl ToolType {
    /// Tools in toolbar order, split into groups.
    pub fn groups() -> &'static [&'static [ToolType]] {
        &[
            &[ToolType::Select],
            &[ToolType::Brush, ToolType::Eraser, ToolType::Fill, ToolType::Eyedropper],
            &[ToolType::Text],
            &[ToolType::Line, ToolType::Rectangle, ToolType::Ellipse, ToolType::Border],
        ]
    }

    /// Tools that change cells (and take Alt+click as the eyedropper).
    pub fn draws(&self) -> bool {
        !matches!(self, ToolType::Select | ToolType::Eyedropper)
    }

    /// Box shapes and lines, drawn by dragging from corner to corner.
    pub fn is_shape(&self) -> bool {
        matches!(self, ToolType::Line | ToolType::Rectangle | ToolType::Ellipse | ToolType::Border)
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ToolType::Brush     => icons::BRUSH,
            ToolType::Text      => icons::TEXT,
            ToolType::Eyedropper => icons::EYEDROPPER,
            ToolType::Eraser    => icons::ERASER,
            ToolType::Fill      => icons::FILL,
            ToolType::Rectangle => icons::RECTANGLE,
            ToolType::Ellipse   => icons::ELLIPSE,
            ToolType::Line      => icons::LINE,
            ToolType::Border    => icons::BORDER,
            ToolType::Select    => icons::SELECT,
        }
    }

    /// The single-key shortcut that selects the tool.
    pub fn key(&self) -> char {
        match self {
            ToolType::Brush     => 'B',
            ToolType::Text      => 'T',
            ToolType::Eyedropper => 'I',
            ToolType::Eraser    => 'E',
            ToolType::Fill      => 'F',
            ToolType::Rectangle => 'R',
            ToolType::Ellipse   => 'O',
            ToolType::Line      => 'L',
            ToolType::Border    => 'D',
            ToolType::Select    => 'S',
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ToolType::Brush     => "Brush",
            ToolType::Text      => "Text",
            ToolType::Eyedropper => "Eyedropper",
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

/// Which parts of a cell the drawing tools change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channels {
    pub glyph: bool,
    pub fg: bool,
    pub bg: bool,
}

impl Default for Channels {
    fn default() -> Self {
        Self { glyph: true, fg: true, bg: true }
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
    pub channels: Channels,
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

    /// `old` with the enabled channels replaced: glyph `ch` and the slot's colours.
    pub fn paint(&self, old: &Cell, ch: char) -> Cell {
        Cell {
            ch: if self.channels.glyph { ch } else { old.ch },
            fg: if self.channels.fg { self.current_fg() } else { old.fg.clone() },
            bg: if self.channels.bg { self.current_bg() } else { old.bg.clone() },
        }
    }

    /// `old` with the enabled channels cleared.
    pub fn erase(&self, old: &Cell) -> Cell {
        let blank = Cell::empty();
        Cell {
            ch: if self.channels.glyph { blank.ch } else { old.ch },
            fg: if self.channels.fg { blank.fg } else { old.fg.clone() },
            bg: if self.channels.bg { blank.bg } else { old.bg.clone() },
        }
    }
}

/// Paints glyphs at cells of the active layer (through [`DrawContext::paint`]), recording changes.
pub fn apply_stamps(doc: &mut Document, stamps: &[(usize, usize, char)], ctx: &DrawContext) -> Changes {
    let mut changes = Changes::new();
    let layer = doc.active_layer_mut();
    for &(x, y, ch) in stamps {
        let Some(old) = layer.get(x, y).cloned() else { continue };
        let new = ctx.paint(&old, ch);
        layer.set(x, y, new.clone());
        merge_changes(&mut changes, Changes::from([((x, y), (old, new))]));
    }
    changes
}

/// What [`apply_stamps`] would produce, for previews.
pub fn preview_stamps(doc: &Document, stamps: &[(usize, usize, char)], ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
    let layer = doc.active_layer();
    stamps
        .iter()
        .filter_map(|&(x, y, ch)| layer.get(x, y).map(|old| (x, y, ctx.paint(old, ch))))
        .collect()
}

/// A `size`×`size` square around (cx, cy); even sizes reach one cell further right / down.
pub fn brush_positions(cx: usize, cy: usize, size: usize) -> Vec<(usize, usize)> {
    let mut positions = Vec::new();
    let size = size.max(1) as isize;
    let (lo, hi) = (-(size - 1) / 2, size / 2);
    let cx = cx as isize;
    let cy = cy as isize;
    for dy in lo..=hi {
        for dx in lo..=hi {
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