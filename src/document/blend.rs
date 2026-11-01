//! How a layer's glyphs meet the glyphs below them, and what a stack of layers looks like.

use serde::{Deserialize, Serialize};

use super::{Cell, Color};
use crate::glyphs::{box_arms, from_arms};

/// A layer's blend mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Blend {
    /// A glyph replaces the one below.
    #[default]
    Normal,
    /// Glyphs that add up to another real character do (`│` + `─` = `┼`, braille dots, block
    /// quarters and sixths, `/` + `\` = `X`); others replace the one below. Text keeps it all.
    Combine,
    /// Both glyphs are drawn, one over the other. Pictures show it; text can only keep the top one.
    Overlay,
}

impl Blend {
    pub const ALL: [Blend; 3] = [Blend::Normal, Blend::Combine, Blend::Overlay];

    pub fn name(self) -> &'static str {
        match self {
            Blend::Normal => "Normal",
            Blend::Combine => "Combine",
            Blend::Overlay => "Overlay",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Blend::Normal => "Glyphs replace the ones below",
            Blend::Combine => "Glyphs join the ones below into one character where Unicode has it (│+─ = ┼, braille, blocks); text export keeps them",
            Blend::Overlay => "Glyphs are drawn over the ones below; text export keeps only the top glyph",
        }
    }
}

/// A cell with every visible layer stacked: its background and the glyphs drawn on it, bottom
/// first. There is more than one glyph only where an Overlay layer draws over another.
#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub bg: Color,
    pub glyphs: Vec<(char, Color)>,
}

impl Stack {
    pub fn new() -> Self {
        Self { bg: Color::TRANSPARENT, glyphs: Vec::new() }
    }

    /// A single cell, as the bottom of a stack.
    pub fn from_cell(cell: &Cell) -> Self {
        let glyphs = if cell.ch == ' ' { Vec::new() } else { vec![(cell.ch, cell.fg.clone())] };
        Self { bg: cell.bg.clone(), glyphs }
    }

    /// Puts a layer's cell on top. Backgrounds blend by their alpha; glyphs meet by `blend`.
    pub fn push(&mut self, cell: &Cell, blend: Blend, opacity: f32) {
        if opacity <= 0.0 { return; }
        self.bg = cell.bg.faded(opacity).over(&self.bg);
        if cell.ch == ' ' { return; }
        let top = (cell.ch, cell.fg.faded(opacity));
        match blend {
            Blend::Normal => self.glyphs = vec![top],
            Blend::Combine => match self.glyphs.last().and_then(|&(below, _)| combine(below, cell.ch)) {
                Some(ch) => {
                    self.glyphs.pop();
                    self.glyphs.push((ch, top.1));
                }
                None => self.glyphs = vec![top],
            },
            Blend::Overlay => self.glyphs.push(top),
        }
    }

    /// The cell as text can hold it: the top glyph on the background.
    pub fn top(&self) -> Cell {
        match self.glyphs.last() {
            Some((ch, fg)) => Cell::new(*ch, fg.clone(), self.bg.clone()),
            None => Cell { bg: self.bg.clone(), ..Cell::empty() },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty() && self.bg.is_transparent()
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

/// The one character that shows both `below` and `top`, if there is one.
pub fn combine(below: char, top: char) -> Option<char> {
    if below == top {
        return Some(top);
    }
    if let (Some(b), Some(t)) = (box_arms(below), box_arms(top)) {
        // The top glyph's style wins where both have an arm.
        return from_arms(std::array::from_fn(|i| if t[i] != 0 { t[i] } else { b[i] }));
    }
    let braille = |c: char| (c as u32).checked_sub(0x2800).filter(|&b| b < 0x100);
    if let (Some(b), Some(t)) = (braille(below), braille(top)) {
        return char::from_u32(0x2800 + (b | t));
    }
    if let (Some(b), Some(t)) = (block_mask(below), block_mask(top)) {
        return from_block_mask(b | t);
    }
    match (below.min(top), below.max(top)) {
        ('-', '|') => Some('+'),
        ('/', '\\') => Some('X'),
        ('+', '-' | '|') | ('-' | '|', '+') => Some('+'),
        _ => None,
    }
}

/// Block glyphs as a 2-column × 6-row grid (bit = row × 2 + column): fine enough for halves,
/// quarters (3 rows each) and sixths (2 rows each) alike.
fn block_mask(ch: char) -> Option<u16> {
    // Quarters: up-left, up-right, down-left, down-right.
    let quarters = |q: u16| {
        let mut mask = 0;
        for (bit, col, rows) in [(1, 0, 0..3), (2, 1, 0..3), (4, 0, 3..6), (8, 1, 3..6)] {
            if q & bit != 0 {
                for row in rows {
                    mask |= 1 << (row * 2 + col);
                }
            }
        }
        mask
    };
    let sixths = |s: u32| {
        let mut mask = 0;
        for i in 0..6 {
            if s & (1 << i) != 0 {
                let (col, row) = (i % 2, i / 2);
                mask |= (1 << (row * 4 + col)) | (1 << (row * 4 + 2 + col));
            }
        }
        mask
    };
    let q = QUARTERS.iter().position(|&c| c == ch);
    if let Some(q) = q {
        return Some(quarters(q as u16 + 1));
    }
    let index = (ch as u32).checked_sub(0x1FB00).filter(|&i| i < 60)?;
    // Sextants skip the patterns that exist elsewhere (left half, right half).
    let mut bits = index + 1;
    if bits >= 21 { bits += 1; }
    if bits >= 42 { bits += 1; }
    Some(sixths(bits))
}

fn from_block_mask(mask: u16) -> Option<char> {
    QUARTERS
        .iter()
        .copied()
        .chain((0x1FB00..0x1FB3C).filter_map(char::from_u32))
        .find(|&c| block_mask(c) == Some(mask))
}

/// The quarter-block glyphs by their quarters (bit 1 up-left, 2 up-right, 4 down-left, 8 down-right), from 1.
const QUARTERS: [char; 15] = ['▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
