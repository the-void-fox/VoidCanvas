//! The `.vc` file format, version 2: every cell of every layer and frame, compressed.
//!
//! Layout: `VOIDCANV`, the format version (one byte), the coded body, and a CRC-32 of all of
//! that (4 bytes, little-endian). Older files are JSON and still load (see `file_io`).
//!
//! The body goes through an adaptive binary range coder (the kind LZMA uses): every yes/no
//! decision is coded with a probability learned from the decisions before it, so a decision
//! that is nearly always the same costs a few hundredths of a bit.
//!
//! Cells are coded frame by frame, layer by layer, row by row, and each one is first
//! *predicted*. The candidates are the same cell in the previous frame and the left, up,
//! up-left and up-right neighbours; for each (skipping repeats) one decision says "it's this
//! one". The probability depends on which candidate matched the cell before and on whether
//! the left and up neighbours agree, so runs sideways or downwards, flat areas and frames
//! that barely change are almost free. A cell that matches no candidate is spelled out:
//! glyph, foreground and background, each again tried against the neighbours' first. Then a
//! glyph is an index into the table of glyphs the file uses; a colour is either an index into
//! its table of colours or, for pictures with many colours (imported photos, gradients), the
//! difference from a colour predicted from the left, up and up-left neighbours, channel by
//! channel (the edge-following predictor of JPEG-LS, as in lossless WebP / PNG filters). The
//! encoder tries both and keeps the smaller file.

use std::collections::HashMap;

use super::{Cell, Color, Document, Layer};

pub const MAGIC: &[u8; 8] = b"VOIDCANV";
const VERSION: u8 = 2;

/// Sizes a file may declare; anything bigger is damaged (or would not fit in memory).
const MAX_SIDE: usize = 4096;
const MAX_CELLS: usize = 32 << 20;
const MAX_TEXT: usize = 1 << 16;

/// Is `bytes` a file in this format (rather than an older JSON one)?
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

mod range {
    //! An adaptive binary range coder, after the one in LZMA.

    const TOP: u32 = 1 << 24;
    /// Fine probabilities: a near-certain decision can cost well under a thousandth of a bit.
    const PROB_BITS: u32 = 15;
    const ONE: u16 = 1 << PROB_BITS;
    /// How fast probabilities adapt: each decision moves them 1/16 of the way.
    const ADAPT: u32 = 4;
    /// The probability of a 0 before anything is known.
    pub const EVEN: u16 = ONE / 2;

    pub struct Encoder {
        low: u64,
        range: u32,
        cache: u8,
        pending: u64,
        out: Vec<u8>,
    }

    impl Encoder {
        pub fn new() -> Self {
            Self { low: 0, range: u32::MAX, cache: 0, pending: 1, out: Vec::new() }
        }

        fn shift_low(&mut self) {
            if self.low < 0xFF00_0000 || self.low > 0xFFFF_FFFF {
                let carry = (self.low >> 32) as u8;
                let mut byte = self.cache;
                loop {
                    self.out.push(byte.wrapping_add(carry));
                    byte = 0xFF;
                    self.pending -= 1;
                    if self.pending == 0 { break; }
                }
                self.cache = (self.low >> 24) as u8;
            }
            self.pending += 1;
            self.low = (self.low & 0x00FF_FFFF) << 8;
        }

        fn normalize(&mut self) {
            while self.range < TOP {
                self.range <<= 8;
                self.shift_low();
            }
        }

        pub fn bit(&mut self, prob: &mut u16, bit: bool) {
            let bound = (self.range >> PROB_BITS) * *prob as u32;
            if bit {
                self.low += bound as u64;
                self.range -= bound;
                *prob -= *prob >> ADAPT;
            } else {
                self.range = bound;
                *prob += (ONE - *prob) >> ADAPT;
            }
            self.normalize();
        }

        /// `bits` low bits of `value`, each as likely 0 as 1.
        pub fn direct(&mut self, value: u32, bits: u32) {
            for i in (0..bits).rev() {
                self.range >>= 1;
                if (value >> i) & 1 == 1 {
                    self.low += self.range as u64;
                }
                self.normalize();
            }
        }

        pub fn finish(mut self) -> Vec<u8> {
            for _ in 0..5 {
                self.shift_low();
            }
            self.out
        }
    }

    /// Reading past the end gives zeros; a damaged file decodes to nonsense, never a panic.
    pub struct Decoder<'a> {
        data: &'a [u8],
        pos: usize,
        range: u32,
        code: u32,
    }

    impl<'a> Decoder<'a> {
        pub fn new(data: &'a [u8]) -> Self {
            let mut decoder = Self { data, pos: 0, range: u32::MAX, code: 0 };
            for _ in 0..5 {
                decoder.code = (decoder.code << 8) | decoder.next_byte() as u32;
            }
            decoder
        }

        fn next_byte(&mut self) -> u8 {
            let byte = self.data.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
            byte
        }

        fn normalize(&mut self) {
            while self.range < TOP {
                self.range <<= 8;
                self.code = (self.code << 8) | self.next_byte() as u32;
            }
        }

        pub fn bit(&mut self, prob: &mut u16) -> bool {
            let bound = (self.range >> PROB_BITS) * *prob as u32;
            let bit = if self.code < bound {
                self.range = bound;
                *prob += (ONE - *prob) >> ADAPT;
                false
            } else {
                self.code -= bound;
                self.range -= bound;
                *prob -= *prob >> ADAPT;
                true
            };
            self.normalize();
            bit
        }

        pub fn direct(&mut self, bits: u32) -> u32 {
            let mut value = 0;
            for _ in 0..bits {
                self.range >>= 1;
                let bit = self.code >= self.range;
                if bit {
                    self.code -= self.range;
                }
                value = (value << 1) | bit as u32;
                self.normalize();
            }
            value
        }

        /// Read more than the data held: the file was cut short.
        pub fn overran(&self) -> bool {
            self.pos > self.data.len() + 4
        }
    }
}

use range::{Decoder, EVEN, Encoder};

/// A `bits`-bit number with a learned probability for every prefix (most significant bit first).
struct Tree {
    bits: u32,
    probs: Vec<u16>,
}

impl Tree {
    fn new(bits: u32) -> Self {
        Self { bits, probs: vec![EVEN; 1 << bits] }
    }

    /// Wide enough for indices below `count`.
    fn for_count(count: usize) -> Self {
        Self::new(usize::BITS - count.saturating_sub(1).leading_zeros())
    }

    fn encode(&mut self, e: &mut Encoder, value: u32) {
        let mut node = 1;
        for i in (0..self.bits).rev() {
            let bit = (value >> i) & 1 == 1;
            e.bit(&mut self.probs[node], bit);
            node = (node << 1) | bit as usize;
        }
    }

    fn decode(&mut self, d: &mut Decoder) -> u32 {
        let mut node = 1;
        for _ in 0..self.bits {
            node = (node << 1) | d.bit(&mut self.probs[node]) as usize;
        }
        (node - (1 << self.bits)) as u32
    }
}

/// Any `u32`: its length in bits (learned), then the bits under the leading one.
struct Number(Tree);

impl Number {
    fn new() -> Self {
        Self(Tree::new(6))
    }

    fn encode(&mut self, e: &mut Encoder, value: u32) {
        let w = value as u64 + 1;
        let len = 64 - w.leading_zeros();
        self.0.encode(e, len - 1);
        e.direct((w - (1 << (len - 1))) as u32, len - 1);
    }

    fn decode(&mut self, d: &mut Decoder) -> u64 {
        let len = (self.0.decode(d) + 1).min(33);
        let w = (1u64 << (len - 1)) | d.direct(len - 1) as u64;
        w - 1
    }
}

/// Learned probabilities for everything in a file.
struct Models {
    number: Number,
    flag: u16,
    byte: Tree,
    same_duration: u16,
    channel: [Tree; 4],
    /// "The cell is candidate k": [kind][what the cell before matched][left = up].
    is: [[[u16; 2]; TOKENS]; KINDS],
    glyph_is: [u16; 3],
    /// [kind][glyph is a space]
    fg_is: [[u16; 2]; 3],
    bg_is: [u16; 3],
    glyph: Tree,
    fg: [Tree; 2],
    bg: Tree,
    /// Predicted-colour mode: the zigzagged differences, for [fg, bg][channel][how big the
    /// last difference of that channel was].
    residual: [[[Tree; ENERGY]; 4]; 2],
    /// The last zigzagged difference per [fg, bg][channel].
    last_residual: [[u32; 4]; 2],
}

/// Size classes of the last difference: noisy and smooth areas learn apart (as in JPEG-LS).
const ENERGY: usize = 6;

fn energy(z: u32) -> usize {
    match z {
        0 => 0,
        1..=2 => 1,
        3..=6 => 2,
        7..=14 => 3,
        15..=30 => 4,
        _ => 5,
    }
}

impl Models {
    /// Codes the colour `value` as its difference from `guess`. Green goes first and red and
    /// blue as their difference from green's ("subtract green", as in lossless WebP): the
    /// channels of a colour tend to move together.
    fn encode_residual(&mut self, e: &mut Encoder, which: usize, value: [u8; 4], guess: [u8; 4]) {
        let diff: [u8; 4] = std::array::from_fn(|k| value[k].wrapping_sub(guess[k]));
        let coded = [diff[1], diff[0].wrapping_sub(diff[1]), diff[2].wrapping_sub(diff[1]), diff[3]];
        for (k, d) in coded.into_iter().enumerate() {
            let z = zigzag(d);
            self.residual[which][k][energy(self.last_residual[which][k])].encode(e, z);
            self.last_residual[which][k] = z;
        }
    }

    fn decode_residual(&mut self, d: &mut Decoder, which: usize, guess: [u8; 4]) -> [u8; 4] {
        let mut coded = [0u8; 4];
        for (k, c) in coded.iter_mut().enumerate() {
            let z = self.residual[which][k][energy(self.last_residual[which][k])].decode(d);
            self.last_residual[which][k] = z;
            *c = unzigzag(z);
        }
        let diff = [coded[1].wrapping_add(coded[0]), coded[0], coded[2].wrapping_add(coded[0]), coded[3]];
        std::array::from_fn(|k| guess[k].wrapping_add(diff[k]))
    }
}

/// Candidates: the previous frame, left, up, up-left, up-right.
const KINDS: usize = 5;
/// What the cell before matched: a kind, nothing (`MISS`), or it was the first of a row (`ROW`).
const MISS: usize = KINDS;
const ROW: usize = KINDS + 1;
const TOKENS: usize = KINDS + 2;

impl Models {
    fn new(glyphs: usize, colors: usize) -> Self {
        Self {
            number: Number::new(),
            flag: EVEN,
            byte: Tree::new(8),
            same_duration: EVEN,
            channel: std::array::from_fn(|_| Tree::new(8)),
            is: [[[EVEN; 2]; TOKENS]; KINDS],
            glyph_is: [EVEN; 3],
            fg_is: [[EVEN; 2]; 3],
            bg_is: [EVEN; 3],
            glyph: Tree::for_count(glyphs),
            fg: [Tree::for_count(colors), Tree::for_count(colors)],
            bg: Tree::for_count(colors),
            residual: std::array::from_fn(|_| std::array::from_fn(|_| std::array::from_fn(|_| Tree::new(8)))),
            last_residual: [[0; 4]; 2],
        }
    }
}

type Rgba = (u8, u8, u8, u8);

fn rgba(c: &Color) -> Rgba {
    (c.r, c.g, c.b, c.a)
}

/// The candidates for the cell at `i` (in `cells`, whose cells before `i` are known), repeats left out.
fn candidates<'a>(cells: &'a [Cell], before: Option<&'a [Cell]>, i: usize, width: usize) -> Vec<(usize, &'a Cell)> {
    let (x, y) = (i % width, i / width);
    let all = [
        before.map(|b| &b[i]),
        (x > 0).then(|| &cells[i - 1]),
        (y > 0).then(|| &cells[i - width]),
        (x > 0 && y > 0).then(|| &cells[i - width - 1]),
        (x + 1 < width && y > 0).then(|| &cells[i - width + 1]),
    ];
    let mut out: Vec<(usize, &Cell)> = Vec::with_capacity(KINDS);
    for (kind, cell) in all.into_iter().enumerate() {
        if let Some(cell) = cell
            && out.iter().all(|(_, c)| *c != cell)
        {
            out.push((kind, cell));
        }
    }
    out
}

/// Does the left neighbour equal the one above? (Context for the predictions.)
fn agree(cells: &[Cell], i: usize, width: usize) -> usize {
    let (x, y) = (i % width, i / width);
    (x > 0 && y > 0 && cells[i - 1] == cells[i - width]) as usize
}

/// A colour expected from the left, up and up-left neighbours' (`part` picks fg or bg), channel
/// by channel with the median edge detector of JPEG-LS: across an edge it takes the side the
/// pixel is on, on a slope it continues the slope.
fn predict(cells: &[Cell], i: usize, width: usize, part: fn(&Cell) -> &Color) -> [u8; 4] {
    let channels = |j: usize| {
        let c = part(&cells[j]);
        [c.r, c.g, c.b, c.a]
    };
    let (x, y) = (i % width, i / width);
    match (x > 0, y > 0) {
        (true, true) => {
            let (l, u, ul) = (channels(i - 1), channels(i - width), channels(i - width - 1));
            std::array::from_fn(|k| {
                let (lo, hi) = (l[k].min(u[k]), l[k].max(u[k]));
                if ul[k] >= hi {
                    lo
                } else if ul[k] <= lo {
                    hi
                } else {
                    (l[k] as i16 + u[k] as i16 - ul[k] as i16) as u8
                }
            })
        }
        (true, false) => channels(i - 1),
        (false, true) => channels(i - width),
        (false, false) => [0; 4],
    }
}

fn zigzag(diff: u8) -> u32 {
    let d = diff as i8;
    ((d << 1) ^ (d >> 7)) as u8 as u32
}

fn unzigzag(z: u32) -> u8 {
    let z = z as u8;
    ((z >> 1) as i8 ^ -((z & 1) as i8)) as u8
}

fn fg(c: &Cell) -> &Color {
    &c.fg
}

fn bg(c: &Cell) -> &Color {
    &c.bg
}

/// Distinct values of `part` among the previous-frame, left and up candidates, with their kind.
fn part_candidates<T: PartialEq>(cands: &[(usize, &Cell)], part: impl Fn(&Cell) -> T) -> Vec<(usize, T)> {
    let mut out: Vec<(usize, T)> = Vec::new();
    for &(kind, cell) in cands.iter().filter(|(kind, _)| *kind < 3) {
        let value = part(cell);
        if out.iter().all(|(_, v)| *v != value) {
            out.push((kind, value));
        }
    }
    out
}

pub fn encode(doc: &Document) -> Vec<u8> {
    let indexed = encode_with(doc, false);
    let predicted = encode_with(doc, true);
    if predicted.len() < indexed.len() { predicted } else { indexed }
}

/// With `predicted` colours spelled out as differences from a prediction instead of table indices.
pub(crate) fn encode_with(doc: &Document, predicted: bool) -> Vec<u8> {
    let (w, h) = (doc.width, doc.height);
    let frames = doc.frame_count();

    // Tables: glyphs by code point, colours by how often they're used.
    let mut glyphs: Vec<char> = Vec::new();
    let mut uses: HashMap<Rgba, usize> = HashMap::new();
    for layer in &doc.layers {
        for f in 0..frames {
            for cell in layer.cells_in(f) {
                glyphs.push(cell.ch);
                *uses.entry(rgba(&cell.fg)).or_default() += 1;
                *uses.entry(rgba(&cell.bg)).or_default() += 1;
            }
        }
    }
    glyphs.sort_unstable();
    glyphs.dedup();
    let mut colors: Vec<Rgba> = uses.keys().copied().collect();
    colors.sort_by(|a, b| uses[b].cmp(&uses[a]).then(a.cmp(b)));
    let glyph_index: HashMap<char, u32> = glyphs.iter().enumerate().map(|(i, &c)| (c, i as u32)).collect();
    let color_index: HashMap<Rgba, u32> = colors.iter().enumerate().map(|(i, &c)| (c, i as u32)).collect();

    let mut e = Encoder::new();
    let mut m = Models::new(glyphs.len(), colors.len());
    for n in [w, h, doc.layers.len(), frames, doc.active_layer, doc.frame] {
        m.number.encode(&mut e, n as u32);
    }
    let mut last = 0;
    for &ms in &doc.durations {
        e.bit(&mut m.same_duration, ms == last);
        if ms != last {
            m.number.encode(&mut e, ms);
        }
        last = ms;
    }
    let text = |e: &mut Encoder, m: &mut Models, s: &str| {
        m.number.encode(e, s.len() as u32);
        for &b in s.as_bytes() {
            m.byte.encode(e, b as u32);
        }
    };
    for layer in &doc.layers {
        text(&mut e, &mut m, &layer.name);
        e.bit(&mut m.flag, layer.visible);
        e.bit(&mut m.flag, layer.locked);
        let opaque = layer.opacity == 1.0;
        e.bit(&mut m.flag, opaque);
        if !opaque {
            e.direct(layer.opacity.to_bits(), 32);
        }
    }
    m.number.encode(&mut e, glyphs.len() as u32);
    let mut previous = 0;
    for &g in &glyphs {
        m.number.encode(&mut e, g as u32 - previous);
        previous = g as u32;
    }
    e.bit(&mut m.flag, predicted);
    if !predicted {
        m.number.encode(&mut e, colors.len() as u32);
        for &(r, g, b, a) in &colors {
            for (channel, value) in m.channel.iter_mut().zip([r, g, b, a]) {
                channel.encode(&mut e, value as u32);
            }
        }
    }
    let colour = |e: &mut Encoder, m: &mut Models, cells: &[Cell], i: usize, which: usize, space: usize| {
        let (part, value): (fn(&Cell) -> &Color, &Color) = if which == 0 { (fg, &cells[i].fg) } else { (bg, &cells[i].bg) };
        if predicted {
            let guess = predict(cells, i, w, part);
            m.encode_residual(e, which, [value.r, value.g, value.b, value.a], guess);
        } else if which == 0 {
            m.fg[space].encode(e, color_index[&rgba(value)]);
        } else {
            m.bg.encode(e, color_index[&rgba(value)]);
        }
    };

    for f in 0..frames {
        for layer in &doc.layers {
            let cells = layer.cells_in(f);
            let before = (f > 0).then(|| layer.cells_in(f - 1));
            let mut token = ROW;
            for (i, cell) in cells.iter().enumerate() {
                if i % w == 0 {
                    token = ROW;
                }
                let cands = candidates(cells, before, i, w);
                let agree = agree(cells, i, w);
                let mut matched = MISS;
                for &(kind, cand) in &cands {
                    let hit = cell == cand;
                    e.bit(&mut m.is[kind][token][agree], hit);
                    if hit {
                        matched = kind;
                        break;
                    }
                }
                token = matched;
                if matched != MISS { continue; }

                if !part_candidates(&cands, |c| c.ch).into_iter().any(|(kind, g)| {
                    let hit = g == cell.ch;
                    e.bit(&mut m.glyph_is[kind], hit);
                    hit
                }) {
                    m.glyph.encode(&mut e, glyph_index[&cell.ch]);
                }
                let space = (cell.ch == ' ') as usize;
                if !part_candidates(&cands, |c| rgba(&c.fg)).into_iter().any(|(kind, fg)| {
                    let hit = fg == rgba(&cell.fg);
                    e.bit(&mut m.fg_is[kind][space], hit);
                    hit
                }) {
                    colour(&mut e, &mut m, cells, i, 0, space);
                }
                if !part_candidates(&cands, |c| rgba(&c.bg)).into_iter().any(|(kind, bg)| {
                    let hit = bg == rgba(&cell.bg);
                    e.bit(&mut m.bg_is[kind], hit);
                    hit
                }) {
                    colour(&mut e, &mut m, cells, i, 1, 0);
                }
            }
        }
    }

    let mut out = MAGIC.to_vec();
    out.push(VERSION);
    out.extend(e.finish());
    let crc = crc32fast::hash(&out);
    out.extend(crc.to_le_bytes());
    out
}

pub fn decode(bytes: &[u8]) -> Result<Document, String> {
    let damaged = |what: &str| format!("the file is damaged ({what})");
    if !is_binary(bytes) || bytes.len() < MAGIC.len() + 1 + 4 {
        return Err("not a Void Canvas file".into());
    }
    let version = bytes[MAGIC.len()];
    if version != VERSION {
        return Err(format!("the file was made by a newer Void Canvas (format {version})"));
    }
    let (data, crc) = bytes.split_at(bytes.len() - 4);
    if crc32fast::hash(data) != u32::from_le_bytes(crc.try_into().unwrap()) {
        return Err(damaged("checksum mismatch"));
    }
    let mut d = Decoder::new(&data[MAGIC.len() + 1..]);

    // The tables' sizes come later; the models for them are made once those are known.
    let mut m = Models::new(1, 1);
    let number = |d: &mut Decoder, m: &mut Models, max: usize| {
        let n = m.number.decode(d);
        if n as usize > max { Err(damaged("a size is out of range")) } else { Ok(n as usize) }
    };
    let (w, h) = (number(&mut d, &mut m, MAX_SIDE)?, number(&mut d, &mut m, MAX_SIDE)?);
    let layers = number(&mut d, &mut m, MAX_CELLS)?;
    let frames = number(&mut d, &mut m, MAX_CELLS)?;
    let total = (w * h).checked_mul(layers).and_then(|n| n.checked_mul(frames)).unwrap_or(usize::MAX);
    if total == 0 || total > MAX_CELLS {
        return Err(damaged("impossible size"));
    }
    let active_layer = number(&mut d, &mut m, layers - 1)?;
    let frame = number(&mut d, &mut m, frames - 1)?;
    let mut durations = Vec::with_capacity(frames);
    let mut last = 0;
    for _ in 0..frames {
        if !d.bit(&mut m.same_duration) {
            last = number(&mut d, &mut m, u32::MAX as usize)? as u32;
        }
        durations.push(last);
    }
    let mut info = Vec::with_capacity(layers);
    for _ in 0..layers {
        let len = number(&mut d, &mut m, MAX_TEXT)?;
        let name: Vec<u8> = (0..len).map(|_| m.byte.decode(&mut d) as u8).collect();
        let name = String::from_utf8(name).map_err(|_| damaged("a layer name"))?;
        let visible = d.bit(&mut m.flag);
        let locked = d.bit(&mut m.flag);
        let opacity = if d.bit(&mut m.flag) { 1.0 } else { f32::from_bits(d.direct(32)) };
        info.push((name, visible, locked, opacity));
    }
    // Every glyph and colour in the tables is used by some cell.
    let count = number(&mut d, &mut m, total)?;
    let mut glyphs = Vec::with_capacity(count);
    let mut code = 0u64;
    for i in 0..count {
        let step = m.number.decode(&mut d);
        if i > 0 && step == 0 { return Err(damaged("the glyph table")); }
        code += step;
        glyphs.push(u32::try_from(code).ok().and_then(char::from_u32).ok_or_else(|| damaged("the glyph table"))?);
    }
    let predicted = d.bit(&mut m.flag);
    let count = if predicted { 0 } else { number(&mut d, &mut m, 2 * total)? };
    let colors: Vec<Color> = (0..count)
        .map(|_| {
            let [r, g, b, a] = std::array::from_fn(|c| m.channel[c].decode(&mut d) as u8);
            Color::new(r, g, b, a)
        })
        .collect();
    if glyphs.is_empty() || (colors.is_empty() && !predicted) {
        return Err(damaged("empty tables"));
    }
    let colour = |d: &mut Decoder, cm: &mut Models, cells: &[Cell], i: usize, which: usize, space: usize| -> Result<Color, String> {
        if predicted {
            let guess = predict(cells, i, w, if which == 0 { fg } else { bg });
            let [r, g, b, a] = cm.decode_residual(d, which, guess);
            return Ok(Color::new(r, g, b, a));
        }
        let tree = if which == 0 { &mut cm.fg[space] } else { &mut cm.bg };
        let i = tree.decode(d) as usize;
        colors.get(i).cloned().ok_or_else(|| damaged("a cell"))
    };

    let mut cm = Models::new(glyphs.len(), colors.len());
    let index = |i: u32, len: usize| if (i as usize) < len { Ok(i as usize) } else { Err(damaged("a cell")) };
    let mut all: Vec<Vec<Vec<Cell>>> = vec![Vec::with_capacity(frames); layers];
    for f in 0..frames {
        for cells_of_layer in all.iter_mut() {
            let mut cells: Vec<Cell> = Vec::with_capacity(w * h);
            let before = (f > 0).then(|| cells_of_layer[f - 1].as_slice());
            let mut token = ROW;
            for i in 0..w * h {
                if i % w == 0 {
                    token = ROW;
                }
                // The cell itself isn't known yet; a placeholder keeps the indices right.
                cells.push(Cell::empty());
                let cands = candidates(&cells, before, i, w);
                let agree = agree(&cells, i, w);
                let mut matched = MISS;
                let mut cell = None;
                for &(kind, cand) in &cands {
                    if d.bit(&mut cm.is[kind][token][agree]) {
                        matched = kind;
                        cell = Some(cand.clone());
                        break;
                    }
                }
                token = matched;
                let cell = match cell {
                    Some(cell) => cell,
                    None => {
                        let found = part_candidates(&cands, |c| c.ch).into_iter().find(|&(kind, _)| d.bit(&mut cm.glyph_is[kind]));
                        let ch = match found {
                            Some((_, ch)) => ch,
                            None => glyphs[index(cm.glyph.decode(&mut d), glyphs.len())?],
                        };
                        let space = (ch == ' ') as usize;
                        let found = part_candidates(&cands, |c| c.fg.clone()).into_iter().find(|&(kind, _)| d.bit(&mut cm.fg_is[kind][space]));
                        let fg = match found {
                            Some((_, fg)) => fg,
                            None => colour(&mut d, &mut cm, &cells, i, 0, space)?,
                        };
                        let found = part_candidates(&cands, |c| c.bg.clone()).into_iter().find(|&(kind, _)| d.bit(&mut cm.bg_is[kind]));
                        let bg = match found {
                            Some((_, bg)) => bg,
                            None => colour(&mut d, &mut cm, &cells, i, 1, 0)?,
                        };
                        Cell { ch, fg, bg }
                    }
                };
                cells[i] = cell;
            }
            cells_of_layer.push(cells);
        }
        if d.overran() {
            return Err(damaged("it ends too early"));
        }
    }

    let layers = all
        .into_iter()
        .zip(info)
        .map(|(cells, (name, visible, locked, opacity))| Layer::from_frames(name, w, h, visible, locked, opacity, cells, frame))
        .collect();
    let mut doc = Document::new(w, h);
    doc.layers = layers;
    doc.active_layer = active_layer;
    doc.durations = durations;
    doc.frame = frame;
    Ok(doc)
}

/// Do `a` and `b` hold the same picture, as far as the file is concerned?
pub fn same_content(a: &Document, b: &Document) -> bool {
    a.width == b.width
        && a.height == b.height
        && a.active_layer == b.active_layer
        && a.durations == b.durations
        && a.frame == b.frame
        && a.layers == b.layers
}
