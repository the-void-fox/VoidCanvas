use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_egui(&self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.r, self.g, self.b, self.a)
    }

    pub fn from_egui(c: egui::Color32) -> Self {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        Self { r, g, b, a }
    }

    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }

    /// Like `==`, but every fully transparent colour counts as the same colour.
    pub fn same_as(&self, other: &Color) -> bool {
        (self.is_transparent() && other.is_transparent()) || self == other
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
}

impl Cell {
    pub fn empty() -> Self {
        Self {
            ch: ' ',
            fg: Color::WHITE,
            bg: Color::TRANSPARENT,
        }
    }

    pub fn new(ch: char, fg: Color, bg: Color) -> Self {
        Self { ch, fg, bg }
    }

    /// Nothing to draw: no glyph and no background.
    pub fn is_empty(&self) -> bool {
        self.ch == ' ' && self.bg.is_transparent()
    }

    /// Cells that look identical on screen (fg is ignored when there is no glyph).
    pub fn same_look(&self, other: &Cell) -> bool {
        self.ch == other.ch
            && self.bg.same_as(&other.bg)
            && (self.ch == ' ' || self.fg.same_as(&other.fg))
    }

    /// Composites `self` on top of `below`.
    /// A glyph hides the glyph underneath; an opaque background hides everything underneath.
    pub fn over(&self, below: &Cell) -> Cell {
        let bg_opaque = !self.bg.is_transparent();
        let (ch, fg) = if self.ch != ' ' || bg_opaque {
            (self.ch, self.fg.clone())
        } else {
            (below.ch, below.fg.clone())
        };
        let bg = if bg_opaque { self.bg.clone() } else { below.bg.clone() };
        Cell { ch, fg, bg }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty()
    }
}
