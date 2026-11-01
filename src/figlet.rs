//! FIGlet banner fonts: big letters made of characters.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The bundled fonts (from the FIGlet distribution, BSD licence in `assets/figlet/LICENSE`).
const SOURCES: &[(&str, &str)] = &[
    ("Standard", include_str!("assets/figlet/standard.flf")),
    ("Small", include_str!("assets/figlet/small.flf")),
    ("Slant", include_str!("assets/figlet/slant.flf")),
    ("Big", include_str!("assets/figlet/big.flf")),
    ("Shadow", include_str!("assets/figlet/shadow.flf")),
    ("Mini", include_str!("assets/figlet/mini.flf")),
    ("Banner", include_str!("assets/figlet/banner.flf")),
];

/// Results wider than this are cut off.
const MAX_WIDTH: usize = 500;

pub struct FigFont {
    pub name: &'static str,
    height: usize,
    /// Drawn as a space, but solid while letters are fitted together.
    hardblank: char,
    glyphs: HashMap<char, Vec<Vec<char>>>,
}

/// All bundled fonts, parsed on first use.
pub fn fonts() -> &'static [FigFont] {
    static FONTS: OnceLock<Vec<FigFont>> = OnceLock::new();
    FONTS.get_or_init(|| SOURCES.iter().filter_map(|&(name, src)| parse(name, src)).collect())
}

/// Reads a `.flf` font: header, comments, ASCII 32–126, the German letters, then code-tagged glyphs.
pub fn parse(name: &'static str, src: &str) -> Option<FigFont> {
    let mut lines = src.lines();
    let header = lines.next()?;
    let rest = header.strip_prefix("flf2a")?;
    let hardblank = rest.chars().next()?;
    let fields: Vec<usize> = rest[hardblank.len_utf8()..]
        .split_whitespace()
        .map_while(|f| f.parse().ok())
        .collect();
    let (height, comments) = (*fields.first()?, *fields.get(4)?);
    let mut lines = lines.skip(comments);

    let read_glyph = |lines: &mut dyn Iterator<Item = &str>| -> Option<Vec<Vec<char>>> {
        (0..height)
            .map(|_| {
                let line = lines.next()?.trim_end();
                // Every line ends with an end mark (doubled on the last line).
                let mark = line.chars().last()?;
                Some(line.trim_end_matches(mark).chars().collect())
            })
            .collect()
    };

    let mut glyphs = HashMap::new();
    for code in 32..=126u32 {
        glyphs.insert(char::from_u32(code)?, read_glyph(&mut lines)?);
    }
    for ch in ['Ä', 'Ö', 'Ü', 'ä', 'ö', 'ü', 'ß'] {
        match read_glyph(&mut lines) {
            Some(g) => { glyphs.insert(ch, g); }
            None => break,
        }
    }
    // Code-tagged glyphs: "<code> [comment]" followed by the glyph.
    while let Some(tag) = lines.next() {
        let Some(code) = tag.split_whitespace().next().and_then(parse_code) else { break };
        let Some(glyph) = read_glyph(&mut lines) else { break };
        if let Some(ch) = char::from_u32(code) {
            glyphs.entry(ch).or_insert(glyph);
        }
    }
    Some(FigFont { name, height, hardblank, glyphs })
}

/// FIGlet codes are decimal, `0x` hex or `0` octal; negative ones aren't characters.
fn parse_code(token: &str) -> Option<u32> {
    if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else if token.len() > 1 && token.starts_with('0') {
        u32::from_str_radix(&token[1..], 8).ok()
    } else {
        token.parse().ok()
    }
}

impl FigFont {
    /// `text` in big letters, one string per row; letters slide together until they touch.
    /// Characters the font lacks are skipped.
    pub fn render(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in text.lines() {
            let mut rows: Vec<Vec<char>> = vec![Vec::new(); self.height];
            for ch in line.chars() {
                let Some(glyph) = self.glyphs.get(&ch) else { continue };
                self.append(&mut rows, glyph);
            }
            out.extend(rows.into_iter().map(|row| {
                row.into_iter().take(MAX_WIDTH).map(|c| if c == self.hardblank { ' ' } else { c }).collect::<String>().trim_end().to_string()
            }));
        }
        while out.last().is_some_and(|row| row.is_empty()) {
            out.pop();
        }
        out
    }

    /// Joins `glyph` onto `rows`, removing the empty columns where they meet.
    fn append(&self, rows: &mut [Vec<char>], glyph: &[Vec<char>]) {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let glyph_width = glyph.iter().map(Vec::len).max().unwrap_or(0);
        let leading = |row: &[char]| row.iter().take_while(|&&c| c == ' ').count();
        // Like `figlet -k`, this also trims blank columns in front of the first letter.
        let overlap = rows
            .iter()
            .zip(glyph)
            .map(|(row, g)| (width - row.len()) + row.iter().rev().take_while(|&&c| c == ' ').count() + leading(g))
            .min()
            .unwrap_or(0)
            .min(glyph_width);
        for (row, g) in rows.iter_mut().zip(glyph) {
            row.resize(width, ' ');
            let mut piece: Vec<char> = g.clone();
            piece.resize(glyph_width, ' ');
            // Take the overlap from this row's trailing spaces first, then from the glyph's leading ones.
            let from_row = overlap.min(row.iter().rev().take_while(|&&c| c == ' ').count());
            row.truncate(width - from_row);
            row.extend(piece.into_iter().skip(overlap - from_row));
        }
    }
}
