#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BorderStyle {
    #[default]
    Single,
    Double,
    Heavy,
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

impl BorderStyle {
    /// Box-drawing glyphs framing the box between corners `a` and `b`.
    pub fn frame(&self, a: (usize, usize), b: (usize, usize)) -> Vec<(usize, usize, char)> {
        let c = self.chars();
        let (min_x, max_x) = (a.0.min(b.0), a.0.max(b.0));
        let (min_y, max_y) = (a.1.min(b.1), a.1.max(b.1));
        let mut cells = vec![
            (min_x, min_y, c.tl), (max_x, min_y, c.tr),
            (min_x, max_y, c.bl), (max_x, max_y, c.br),
        ];
        for x in (min_x + 1)..max_x {
            cells.push((x, min_y, c.h));
            cells.push((x, max_y, c.h));
        }
        for y in (min_y + 1)..max_y {
            cells.push((min_x, y, c.v));
            cells.push((max_x, y, c.v));
        }
        cells
    }
}

pub struct BorderChars {
    pub h: char, pub v: char,
    pub tl: char, pub tr: char,
    pub bl: char, pub br: char,
}
