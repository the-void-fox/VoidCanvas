//! Typing on the canvas: click to place the cursor, then type.

use crate::document::Document;
use super::{Changes, DrawContext, apply_stamps};

#[derive(Default, Debug)]
pub struct TextTool {
    /// Where the next character goes; `None` when not typing.
    pub cursor: Option<(usize, usize)>,
    /// Column that Enter returns to.
    line_start: usize,
    /// Typing with the secondary slot (right click).
    pub secondary: bool,
    /// Undo group of the current typing session.
    pub group: u64,
}

impl TextTool {
    /// Starts a typing session at (x, y).
    pub fn start(&mut self, x: usize, y: usize, secondary: bool) {
        self.cursor = Some((x, y));
        self.line_start = x;
        self.secondary = secondary;
        self.group += 1;
    }

    pub fn stop(&mut self) {
        self.cursor = None;
    }

    /// Writes `text` at the cursor; `\n` goes to the next line, tabs become spaces.
    pub fn type_text(&mut self, doc: &mut Document, text: &str, ctx: &DrawContext) -> Changes {
        let mut changes = Changes::new();
        for ch in text.chars() {
            match ch {
                '\n' => self.newline(doc),
                '\t' => {
                    for _ in 0..4 {
                        changes.extend(self.put(doc, ' ', ctx));
                    }
                }
                c if c.is_control() => {}
                c => changes.extend(self.put(doc, c, ctx)),
            }
        }
        changes
    }

    fn put(&mut self, doc: &mut Document, ch: char, ctx: &DrawContext) -> Changes {
        let Some((x, y)) = self.cursor else { return Changes::new(); };
        let changes = apply_stamps(doc, &[(x, y, ch)], ctx);
        // Stay on the last column instead of running off the canvas.
        self.cursor = Some(((x + 1).min(doc.width - 1), y));
        changes
    }

    pub fn newline(&mut self, doc: &Document) {
        if let Some((_, y)) = self.cursor {
            self.cursor = Some((self.line_start, (y + 1).min(doc.height - 1)));
        }
    }

    /// Erases the cell left of the cursor and moves there.
    pub fn backspace(&mut self, doc: &mut Document, ctx: &DrawContext) -> Changes {
        let Some((x, y)) = self.cursor else { return Changes::new(); };
        if x == 0 { return Changes::new(); }
        self.cursor = Some((x - 1, y));
        self.erase_at_cursor(doc, ctx)
    }

    pub fn erase_at_cursor(&mut self, doc: &mut Document, ctx: &DrawContext) -> Changes {
        let Some((x, y)) = self.cursor else { return Changes::new(); };
        let layer = doc.active_layer_mut();
        let Some(old) = layer.get(x, y).cloned() else { return Changes::new(); };
        let new = ctx.erase(&old);
        layer.set(x, y, new.clone());
        Changes::from([((x, y), (old, new))])
    }

    pub fn move_by(&mut self, dx: isize, dy: isize, doc: &Document) {
        if let Some((x, y)) = self.cursor {
            let nx = (x as isize + dx).clamp(0, doc.width as isize - 1) as usize;
            let ny = (y as isize + dy).clamp(0, doc.height as isize - 1) as usize;
            self.cursor = Some((nx, ny));
        }
    }
}
