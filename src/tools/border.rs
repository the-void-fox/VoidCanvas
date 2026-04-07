use crate::document::{Cell, Document};
use super::{Changes, DrawContext};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderStyle {
    Single,
    Double,
    Heavy,
}

impl Default for BorderStyle {
    fn default() -> Self { Self::Single }
}

impl BorderStyle {
    pub fn chars(&self) -> BorderChars {
        match self {
            BorderStyle::Single => BorderChars {
                h: '─', v: '│',
                tl: '┌', tr: '┐', bl: '└', br: '┘',
            },
            BorderStyle::Double => BorderChars {
                h: '═', v: '║',
                tl: '╔', tr: '╗', bl: '╚', br: '╝',
            },
            BorderStyle::Heavy => BorderChars {
                h: '━', v: '┃',
                tl: '┏', tr: '┓', bl: '┗', br: '┛',
            },
        }
    }
}

pub struct BorderChars {
    pub h: char, pub v: char,
    pub tl: char, pub tr: char,
    pub bl: char, pub br: char,
}

#[derive(Default, Debug)]
pub struct BorderTool {
    pub start: Option<(usize, usize)>,
    pub style: BorderStyle,
}

impl BorderTool {
    pub fn on_press(&mut self, x: usize, y: usize) { self.start = Some((x, y)); }

    pub fn on_release(&mut self, doc: &mut Document, x: usize, y: usize, ctx: &DrawContext) -> Changes {
        let Some((sx, sy)) = self.start.take() else { return Changes::new(); };
        self.draw_border(doc, sx, sy, x, y, ctx)
    }

    pub fn preview(&self, doc: &Document, x: usize, y: usize, ctx: &DrawContext) -> Vec<(usize, usize, Cell)> {
        let Some((sx, sy)) = self.start else { return vec![]; };
        let chars = self.style.chars();
        let fg = ctx.current_fg();
        let bg = ctx.current_bg();
        let (min_x, max_x) = (sx.min(x), sx.max(x));
        let (min_y, max_y) = (sy.min(y), sy.max(y));
        let mut preview = Vec::new();

        let mut add = |px: usize, py: usize, ch: char| {
            if px < doc.width && py < doc.height {
                preview.push((px, py, Cell::new(ch, fg.clone(), bg.clone())));
            }
        };

        add(min_x, min_y, chars.tl); add(max_x, min_y, chars.tr);
        add(min_x, max_y, chars.bl); add(max_x, max_y, chars.br);
        for px in (min_x + 1)..max_x { add(px, min_y, chars.h); add(px, max_y, chars.h); }
        for py in (min_y + 1)..max_y { add(min_x, py, chars.v); add(max_x, py, chars.v); }

        preview
    }

    fn draw_border(&self, doc: &mut Document, x1: usize, y1: usize, x2: usize, y2: usize, ctx: &DrawContext) -> Changes {
        let chars = self.style.chars();
        let fg = ctx.current_fg();
        let bg = ctx.current_bg();
        let (min_x, max_x) = (x1.min(x2), x1.max(x2));
        let (min_y, max_y) = (y1.min(y2), y1.max(y2));
        let mut changes = Changes::new();

        let layer = doc.active_layer_mut();
        if layer.locked { return changes; }

        let mut set = |x: usize, y: usize, ch: char| {
            if x >= layer.width || y >= layer.height { return; }
            let old = layer.get(x, y).cloned().unwrap_or_default();
            let new = Cell::new(ch, fg.clone(), bg.clone());
            layer.set(x, y, new.clone());
            changes.insert((x, y), (old, new));
        };

        set(min_x, min_y, chars.tl); set(max_x, min_y, chars.tr);
        set(min_x, max_y, chars.bl); set(max_x, max_y, chars.br);
        for x in (min_x + 1)..max_x { set(x, min_y, chars.h); set(x, max_y, chars.h); }
        for y in (min_y + 1)..max_y { set(min_x, y, chars.v); set(max_x, y, chars.v); }

        changes
    }
}