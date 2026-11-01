//! Gradient: drag across the selection (or the whole layer) to blend from one slot to the other.
//! Glyphs step along a density ramp, colours blend in OKLab.

use crate::document::{Cell, Color, Document};
use super::{Changes, DrawContext};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientShape {
    #[default]
    Linear,
    /// Rings around the press point.
    Radial,
}

/// Options of a gradient, besides the slots and channels in [`DrawContext`].
#[derive(Clone, Copy, Debug)]
pub struct GradientSettings {
    pub shape: GradientShape,
    /// Density ramp from empty to full; the drag starts at full.
    pub ramp: &'static str,
    /// Ordered dithering between neighbouring ramp glyphs.
    pub dither: bool,
    /// Cells it covers: `x0..x1`, `y0..y1`.
    pub region: (usize, usize, usize, usize),
}

#[derive(Default, Debug)]
pub struct GradientTool {
    pub start: Option<(usize, usize)>,
}

impl GradientTool {
    pub fn on_press(&mut self, x: usize, y: usize) {
        self.start = Some((x, y));
    }

    pub fn preview(&self, doc: &Document, end: (usize, usize), ctx: &DrawContext, settings: GradientSettings) -> Vec<(usize, usize, Cell)> {
        match self.start {
            Some(start) => gradient(doc, start, end, ctx, settings),
            None => vec![],
        }
    }

    pub fn on_release(&mut self, doc: &mut Document, end: (usize, usize), ctx: &DrawContext, settings: GradientSettings) -> Changes {
        let Some(start) = self.start.take() else { return Changes::new() };
        let cells = gradient(doc, start, end, ctx, settings);
        let layer = doc.active_layer_mut();
        let mut changes = Changes::new();
        for (x, y, new) in cells {
            let Some(old) = layer.get(x, y).cloned() else { continue };
            layer.set(x, y, new.clone());
            changes.insert((x, y), (old, new));
        }
        changes
    }
}

/// 4×4 Bayer thresholds.
const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// The cells of the active layer a drag from `start` to `end` changes, with their new look.
/// The start gets the primary slot (the secondary one with the right button) and the full glyph.
pub fn gradient(doc: &Document, start: (usize, usize), end: (usize, usize), ctx: &DrawContext, settings: GradientSettings) -> Vec<(usize, usize, Cell)> {
    if start == end { return vec![]; }
    // Cell centres, in units of a cell's width (a cell is twice as tall as wide).
    let centre = |(x, y): (usize, usize)| (x as f32 + 0.5, (y as f32 + 0.5) * 2.0);
    let (a, b) = (centre(start), centre(end));
    let d = (b.0 - a.0, b.1 - a.1);
    let len2 = d.0 * d.0 + d.1 * d.1;

    let (fg, bg) = if ctx.is_primary {
        (Blend::new(&ctx.primary_fg, &ctx.secondary_fg), Blend::new(&ctx.primary_bg, &ctx.secondary_bg))
    } else {
        (Blend::new(&ctx.secondary_fg, &ctx.primary_fg), Blend::new(&ctx.secondary_bg, &ctx.primary_bg))
    };
    let ramp: Vec<char> = settings.ramp.chars().collect();
    let channels = ctx.channels;

    let layer = doc.active_layer();
    let (x0, y0, x1, y1) = settings.region;
    let mut out = Vec::new();
    for y in y0..y1.min(layer.height) {
        for x in x0..x1.min(layer.width) {
            let Some(old) = layer.get(x, y) else { continue };
            let p = centre((x, y));
            let p = (p.0 - a.0, p.1 - a.1);
            let t = match settings.shape {
                GradientShape::Linear => (p.0 * d.0 + p.1 * d.1) / len2,
                GradientShape::Radial => ((p.0 * p.0 + p.1 * p.1) / len2).sqrt(),
            }
            .clamp(0.0, 1.0);

            let ch = if channels.glyph && !ramp.is_empty() {
                let level = (1.0 - t) * (ramp.len() - 1) as f32;
                let step = if settings.dither {
                    let threshold = (BAYER[y % 4][x % 4] as f32 + 0.5) / 16.0;
                    level.floor() as usize + (level.fract() > threshold) as usize
                } else {
                    level.round() as usize
                };
                ramp[step.min(ramp.len() - 1)]
            } else {
                old.ch
            };
            let new = Cell {
                ch,
                // A foreground on an empty cell would be invisible, but still a change.
                fg: if channels.fg && ch != ' ' { fg.at(t) } else { old.fg.clone() },
                bg: if channels.bg { bg.at(t) } else { old.bg.clone() },
            };
            if new != *old {
                out.push((x, y, new));
            }
        }
    }
    out
}

/// Two colours, ready to be mixed.
struct Blend {
    from: Color,
    to: Color,
    lab: ([f32; 3], [f32; 3]),
}

impl Blend {
    fn new(from: &Color, to: &Color) -> Self {
        // A transparent end has no colour of its own: it only fades the other one out.
        let hue = |c: &Color, other: &Color| if c.is_transparent() { other.clone() } else { c.clone() };
        let lab = (oklab(&hue(from, to)), oklab(&hue(to, from)));
        Self { from: from.clone(), to: to.clone(), lab }
    }

    fn at(&self, t: f32) -> Color {
        if t <= 0.0 { return self.from.clone(); }
        if t >= 1.0 { return self.to.clone(); }
        let (a, b) = self.lab;
        let [r, g, bl] = from_oklab([0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t));
        let alpha = self.from.a as f32 + (self.to.a as f32 - self.from.a as f32) * t;
        Color::new(r, g, bl, alpha.round() as u8)
    }
}

fn oklab(c: &Color) -> [f32; 3] {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3]) -> [u8; 3] {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let srgb = |v: f32| {
        let v = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [
        srgb(4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_),
        srgb(-1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_),
        srgb(-0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_),
    ]
}
