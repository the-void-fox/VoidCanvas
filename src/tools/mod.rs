pub mod brush;
pub mod eraser;
pub mod fill;
pub mod gradient;
pub mod shapes;
pub mod border;
pub mod pixel;
pub mod selection;
pub mod shade;
pub mod text;

use crate::document::{Cell, Color, Document};
use crate::glyphs::{Transform, transform_glyph};
use crate::icons;
pub use crate::document::{Changes, merge_changes};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolType {
    Brush,
    Shade,
    Pixel,
    Text,
    Eyedropper,
    Eraser,
    Fill,
    Gradient,
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
            &[ToolType::Brush, ToolType::Pixel, ToolType::Shade, ToolType::Eraser, ToolType::Fill, ToolType::Gradient, ToolType::Eyedropper],
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
            ToolType::Shade     => icons::SHADE,
            ToolType::Pixel     => icons::PIXEL,
            ToolType::Text      => icons::TEXT,
            ToolType::Eyedropper => icons::EYEDROPPER,
            ToolType::Eraser    => icons::ERASER,
            ToolType::Fill      => icons::FILL,
            ToolType::Gradient  => icons::GRADIENT,
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
            ToolType::Shade     => 'G',
            ToolType::Pixel     => 'P',
            ToolType::Text      => 'T',
            ToolType::Eyedropper => 'I',
            ToolType::Eraser    => 'E',
            ToolType::Fill      => 'F',
            ToolType::Gradient  => 'U',
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
            ToolType::Shade     => "Shade",
            ToolType::Pixel     => "Pixel",
            ToolType::Text      => "Text",
            ToolType::Eyedropper => "Eyedropper",
            ToolType::Eraser    => "Eraser",
            ToolType::Fill      => "Fill",
            ToolType::Gradient  => "Gradient",
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

/// Symmetry axes through the middle of the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mirror {
    /// Mirror left ↔ right.
    pub x: bool,
    /// Mirror top ↔ bottom.
    pub y: bool,
}

impl Mirror {
    /// (x, y) and its mirror images in a `w` × `h` grid.
    pub fn points(&self, x: usize, y: usize, w: usize, h: usize) -> Vec<(usize, usize)> {
        let (mx, my) = (w - 1 - x.min(w - 1), h - 1 - y.min(h - 1));
        let mut points = vec![(x, y)];
        if self.x { points.push((mx, y)); }
        if self.y { points.push((x, my)); }
        if self.x && self.y { points.push((mx, my)); }
        points
    }

    /// The stamps and their mirror images, with glyphs turned to match (`/` ↔ `\\`, `┌` ↔ `┐`, …).
    pub fn stamps(&self, stamps: Vec<(usize, usize, char)>, w: usize, h: usize) -> Vec<(usize, usize, char)> {
        if !self.x && !self.y { return stamps; }
        let mut out = Vec::with_capacity(stamps.len() * 4);
        for (x, y, ch) in stamps {
            if x >= w || y >= h { continue; }
            let flipped_x = transform_glyph(ch, Transform::FlipHorizontal);
            out.push((x, y, ch));
            if self.x { out.push((w - 1 - x, y, flipped_x)); }
            if self.y { out.push((x, h - 1 - y, transform_glyph(ch, Transform::FlipVertical))); }
            if self.x && self.y { out.push((w - 1 - x, h - 1 - y, transform_glyph(flipped_x, Transform::FlipVertical))); }
        }
        out
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
    pub mirror: Mirror,
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