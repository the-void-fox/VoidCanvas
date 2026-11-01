//! Turning a picture into character art.

use image::{RgbaImage, imageops};

use crate::document::{Cell, Color};
use crate::tools::pixel::from_halves;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertMode {
    /// Brightness as density: ` .:-=+*#%@`.
    Ascii,
    /// Brightness as shade blocks: ` ░▒▓█`.
    Shades,
    /// Two colour pixels per cell (`▀` with foreground and background).
    HalfBlocks,
    /// Eight dots per cell.
    Braille,
}

impl ConvertMode {
    pub const ALL: [ConvertMode; 4] = [ConvertMode::HalfBlocks, ConvertMode::Ascii, ConvertMode::Shades, ConvertMode::Braille];

    pub fn name(&self) -> &'static str {
        match self {
            ConvertMode::Ascii => "ASCII",
            ConvertMode::Shades => "Shades",
            ConvertMode::HalfBlocks => "Half blocks",
            ConvertMode::Braille => "Braille",
        }
    }

    /// Samples per cell, across and down.
    fn samples(&self) -> (usize, usize) {
        match self {
            ConvertMode::Ascii | ConvertMode::Shades => (1, 1),
            ConvertMode::HalfBlocks => (1, 2),
            ConvertMode::Braille => (2, 4),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConvertSettings {
    /// Width of the result in cells.
    pub width: usize,
    pub mode: ConvertMode,
    /// Keep the picture's colours (otherwise white glyphs / grey pixels).
    pub color: bool,
    /// Dense glyphs for dark areas (for light backgrounds).
    pub invert: bool,
    /// Added to brightness, −1 … 1.
    pub brightness: f32,
    /// Contrast factor, 1 = unchanged.
    pub contrast: f32,
    pub dither: bool,
    /// The density ramp for the ASCII mode, empty to full.
    pub ramp: String,
}

impl Default for ConvertSettings {
    fn default() -> Self {
        Self { width: 80, mode: ConvertMode::HalfBlocks, color: true, invert: false, brightness: 0.0, contrast: 1.0, dither: true, ramp: " .:-=+*#%@".into() }
    }
}

/// Height in cells that keeps the picture's proportions (cells are twice as tall as wide).
pub fn height_for(image: &RgbaImage, width: usize) -> usize {
    let (w, h) = image.dimensions();
    ((width as f32 * h as f32 / w.max(1) as f32) / 2.0).round().max(1.0) as usize
}

/// The picture as rows of cells.
pub fn convert(image: &RgbaImage, settings: &ConvertSettings) -> Vec<Vec<Cell>> {
    let width = settings.width.max(1);
    let height = height_for(image, width);
    let (sx, sy) = settings.mode.samples();
    let (gw, gh) = (width * sx, height * sy);
    let small = imageops::resize(image, gw as u32, gh as u32, imageops::FilterType::Triangle);

    let pixel = |x: usize, y: usize| small.get_pixel(x as u32, y as u32).0;
    let color_at = |x: usize, y: usize| {
        let [r, g, b, _] = pixel(x, y);
        Color::new(r, g, b, 255)
    };
    let visible = |x: usize, y: usize| pixel(x, y)[3] >= 128;

    // Brightness of every sample after the tone settings; transparent samples count as empty.
    let mut tone: Vec<f32> = (0..gh)
        .flat_map(|y| (0..gw).map(move |x| (x, y)))
        .map(|(x, y)| {
            if !visible(x, y) { return 0.0; }
            let [r, g, b, _] = pixel(x, y);
            let lum = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
            let lum = ((lum - 0.5) * settings.contrast + 0.5 + settings.brightness).clamp(0.0, 1.0);
            if settings.invert { 1.0 - lum } else { lum }
        })
        .collect();

    let grey = |t: f32| {
        let v = (t * 255.0).round() as u8;
        Color::new(v, v, v, 255)
    };

    match settings.mode {
        ConvertMode::Ascii | ConvertMode::Shades => {
            let ramp: Vec<char> = if settings.mode == ConvertMode::Ascii { settings.ramp.as_str() } else { " ░▒▓█" }.chars().collect();
            let ramp = if ramp.len() < 2 { vec![' ', '#'] } else { ramp };
            let levels = quantize(&mut tone, gw, gh, ramp.len(), settings.dither);
            grid(width, height, |x, y| {
                let ch = ramp[levels[y * gw + x]];
                if ch == ' ' || !visible(x, y) { return Cell::empty(); }
                let fg = if settings.color { color_at(x, y) } else { Color::WHITE };
                Cell::new(ch, fg, Color::TRANSPARENT)
            })
        }
        ConvertMode::HalfBlocks => grid(width, height, |x, y| {
            let half = |py: usize| {
                visible(x, py).then(|| if settings.color { color_at(x, py) } else { grey(tone[py * gw + x]) })
            };
            from_halves(half(y * 2), half(y * 2 + 1))
        }),
        ConvertMode::Braille => {
            let on = quantize(&mut tone, gw, gh, 2, settings.dither);
            // Bit of each dot in the 2×4 grid: (column, row) → bit.
            const BITS: [[u32; 4]; 2] = [[0, 1, 2, 6], [3, 4, 5, 7]];
            grid(width, height, |x, y| {
                let (mut bits, mut sum, mut lit) = (0u32, [0u32; 3], 0u32);
                for (col, rows) in BITS.iter().enumerate() {
                    for (row, bit) in rows.iter().enumerate() {
                        let (px, py) = (x * 2 + col, y * 4 + row);
                        if on[py * gw + px] == 1 && visible(px, py) {
                            bits |= 1 << bit;
                            let c = color_at(px, py);
                            sum = [sum[0] + c.r as u32, sum[1] + c.g as u32, sum[2] + c.b as u32];
                            lit += 1;
                        }
                    }
                }
                if bits == 0 { return Cell::empty(); }
                let fg = if settings.color {
                    Color::new((sum[0] / lit) as u8, (sum[1] / lit) as u8, (sum[2] / lit) as u8, 255)
                } else {
                    Color::WHITE
                };
                Cell::new(char::from_u32(0x2800 + bits).unwrap(), fg, Color::TRANSPARENT)
            })
        }
    }
}

fn grid(width: usize, height: usize, cell: impl Fn(usize, usize) -> Cell) -> Vec<Vec<Cell>> {
    (0..height).map(|y| (0..width).map(|x| cell(x, y)).collect()).collect()
}

/// Rounds every value (0…1) to one of `levels` steps; with `dither`, Floyd–Steinberg
/// error diffusion keeps the average brightness of each area.
fn quantize(values: &mut [f32], w: usize, h: usize, levels: usize, dither: bool) -> Vec<usize> {
    let top = (levels - 1) as f32;
    let mut out = vec![0; values.len()];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let level = (values[i].clamp(0.0, 1.0) * top).round();
            out[i] = level as usize;
            if !dither { continue; }
            let error = values[i] - level / top;
            let mut spread = |dx: isize, dy: usize, share: f32| {
                let nx = x as isize + dx;
                if nx >= 0 && (nx as usize) < w && y + dy < h {
                    values[(y + dy) * w + nx as usize] += error * share;
                }
            };
            spread(1, 0, 7.0 / 16.0);
            spread(-1, 1, 3.0 / 16.0);
            spread(0, 1, 5.0 / 16.0);
            spread(1, 1, 1.0 / 16.0);
        }
    }
    out
}
