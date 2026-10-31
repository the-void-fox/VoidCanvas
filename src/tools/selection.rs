use crate::document::{Cell, Color, Document};
use super::{Changes, merge_changes};

#[derive(Default, Debug, Clone)]
pub struct SelectionTool {
    pub selection: Option<(usize, usize, usize, usize)>,
    pub start: Option<(usize, usize)>,
    pub is_dragging: bool,
    pub drag_offset: (isize, isize),
    pub content: Option<Vec<Vec<Cell>>>,
    pub source_rect: Option<(usize, usize, usize, usize)>,
    pub source_cleared: bool,
    pub move_started: bool,
}

impl SelectionTool {
    /// Is (x, y) inside the current selection? (Pressing there moves the selection.)
    pub fn hit(&self, x: usize, y: usize) -> bool {
        self.selection
            .is_some_and(|(x1, y1, x2, y2)| x >= x1 && x <= x2 && y >= y1 && y <= y2)
    }

    pub fn on_press(&mut self, doc: &Document, x: usize, y: usize) -> Changes {
        if let Some(sel) = self.selection {
            let (x1, y1, x2, y2) = sel;
            if x >= x1 && x <= x2 && y >= y1 && y <= y2 {
                self.is_dragging = true;
                self.drag_offset = (x as isize - x1 as isize, y as isize - y1 as isize);
                self.move_started = false;
                self.source_cleared = false;
                let (x1, y1, x2, y2) = sel;
                let layer = &doc.layers[doc.active_layer];
                self.content = Some(layer.get_region(x1, y1, x2, y2));
                self.source_rect = Some(sel);
                return Changes::new();
            }
        }

        self.start = Some((x, y));
        self.selection = None;
        self.content = None;
        self.source_rect = None;
        self.is_dragging = false;
        self.move_started = false;
        self.source_cleared = false;
        Changes::new()
    }

    pub fn on_drag(&mut self, doc: &mut Document, x: usize, y: usize) -> Changes {
        if self.is_dragging {
            if let Some(sel) = self.selection {
                let (x1, y1, x2, y2) = sel;
                let w = x2 as isize - x1 as isize;
                let h = y2 as isize - y1 as isize;
                let new_x1 = (x as isize - self.drag_offset.0).max(0) as usize;
                let new_y1 = (y as isize - self.drag_offset.1).max(0) as usize;
                let new_rect = (new_x1, new_y1,
                    (new_x1 as isize + w) as usize,
                    (new_y1 as isize + h) as usize);

                if new_rect != sel {
                    self.move_started = true;
                    let mut changes = Changes::new();
                    if !self.source_cleared
                        && let Some(src) = self.source_rect
                    {
                        changes.extend(clear_rect(doc, src));
                        self.source_cleared = true;
                    }
                    self.selection = Some(new_rect);
                    return changes;
                }
            }
        } else if let Some((sx, sy)) = self.start {
            self.selection = Some((sx.min(x), sy.min(y), sx.max(x), sy.max(y)));
        }
        Changes::new()
    }

    pub fn on_release(&mut self, doc: &mut Document) -> Changes {
        let mut changes = Changes::new();
        if self.is_dragging && self.move_started {
            if let (Some(sel), Some(content)) = (self.selection, &self.content) {
                let layer = doc.active_layer_mut();
                if !layer.locked {
                    let (x1, y1, _, _) = sel;
                    for (dy, row) in content.iter().enumerate() {
                        for (dx, cell) in row.iter().enumerate() {
                            let px = x1 + dx;
                            let py = y1 + dy;
                            if px < layer.width && py < layer.height {
                                let old = layer.get(px, py).cloned().unwrap_or_default();
                                layer.set(px, py, cell.clone());
                                changes.insert((px, py), (old, cell.clone()));
                            }
                        }
                    }
                }
            }
            self.source_rect = self.selection;
            self.source_cleared = false;
            self.move_started = false;
        }
        self.is_dragging = false;
        self.start = None;
        changes
    }

    pub fn copy(&self, doc: &Document) -> Option<Vec<Vec<Cell>>> {
        let (x1, y1, x2, y2) = self.selection?;
        let layer = &doc.layers[doc.active_layer];
        Some(layer.get_region(x1, y1, x2, y2))
    }

    pub fn paste(&mut self, doc: &mut Document, content: &[Vec<Cell>], x: usize, y: usize) -> Changes {
        let h = content.len();
        let w = content.first().map(|r| r.len()).unwrap_or(0);
        let layer = doc.active_layer_mut();
        let mut changes = Changes::new();

        for (dy, row) in content.iter().enumerate() {
            for (dx, cell) in row.iter().enumerate() {
                let px = x + dx; let py = y + dy;
                if px < layer.width && py < layer.height {
                    let old = layer.get(px, py).cloned().unwrap_or_default();
                    layer.set(px, py, cell.clone());
                    changes.insert((px, py), (old, cell.clone()));
                }
            }
        }

        let (max_x, max_y) = (doc.width.saturating_sub(1), doc.height.saturating_sub(1));
        self.selection = Some((x, y, (x + w).saturating_sub(1).min(max_x), (y + h).saturating_sub(1).min(max_y)));
        self.content = Some(content.to_vec());
        self.source_rect = self.selection;
        changes
    }

    /// Swaps the selected block for `block` (same top-left corner), e.g. a rotated copy.
    pub fn replace(&mut self, doc: &mut Document, block: &[Vec<Cell>]) -> Changes {
        let Some(sel) = self.selection else { return Changes::new(); };
        let mut changes = clear_rect(doc, sel);
        merge_changes(&mut changes, self.paste(doc, block, sel.0, sel.1));
        changes
    }

    /// Moves the selected block by (dx, dy) cells, keeping it inside the document.
    pub fn nudge(&mut self, doc: &mut Document, dx: isize, dy: isize) -> Changes {
        let Some(sel @ (x1, y1, x2, y2)) = self.selection else { return Changes::new(); };
        let Some(block) = self.copy(doc) else { return Changes::new(); };
        let max_x = (doc.width as isize - (x2 - x1 + 1) as isize).max(0);
        let max_y = (doc.height as isize - (y2 - y1 + 1) as isize).max(0);
        let nx = (x1 as isize + dx).clamp(0, max_x) as usize;
        let ny = (y1 as isize + dy).clamp(0, max_y) as usize;
        if (nx, ny) == (x1, y1) { return Changes::new(); }
        let mut changes = clear_rect(doc, sel);
        merge_changes(&mut changes, self.paste(doc, &block, nx, ny));
        changes
    }

    pub fn delete(&mut self, doc: &mut Document) -> Changes {
        let Some(sel) = self.selection.take() else { return Changes::new(); };
        let changes = clear_rect(doc, sel);
        self.content = None;
        changes
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Content being dragged right now, with the cell it would land at.
    pub fn moving_content(&self) -> Option<(usize, usize, &[Vec<Cell>])> {
        if !(self.is_dragging && self.move_started) { return None; }
        let (x1, y1, _, _) = self.selection?;
        self.content.as_deref().map(|c| (x1, y1, c))
    }
}

/// Plain-text form of a copied region (trailing spaces trimmed).
pub fn content_to_text(content: &[Vec<Cell>]) -> String {
    content
        .iter()
        .map(|row| row.iter().map(|c| c.ch).collect::<String>().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Turns pasted text into a block of cells; short lines are padded with empty cells.
pub fn text_to_content(text: &str, fg: &Color, bg: &Color) -> Vec<Vec<Cell>> {
    const TAB: usize = 4;
    let rows: Vec<Vec<char>> = text
        .lines()
        .map(|line| {
            let mut row = Vec::new();
            for ch in line.chars() {
                match ch {
                    '\t' => row.extend(std::iter::repeat_n(' ', TAB - row.len() % TAB)),
                    c if c.is_control() => {}
                    c => row.push(c),
                }
            }
            row
        })
        .collect();
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    rows.into_iter()
        .map(|row| {
            (0..width)
                .map(|i| match row.get(i) {
                    Some(&ch) => Cell::new(ch, fg.clone(), bg.clone()),
                    None => Cell::empty(),
                })
                .collect()
        })
        .collect()
}

fn clear_rect(doc: &mut Document, rect: (usize, usize, usize, usize)) -> Changes {
    let (x1, y1, x2, y2) = rect;
    let mut changes = Changes::new();
    let layer = doc.active_layer_mut();
    if layer.locked { return changes; }

    for y in y1..=y2 {
        for x in x1..=x2 {
            if x < layer.width && y < layer.height {
                let old = layer.get(x, y).cloned().unwrap_or_default();
                let empty = Cell::empty();
                layer.set(x, y, empty.clone());
                changes.insert((x, y), (old, empty));
            }
        }
    }
    changes
}