//! Rendering the document into image files.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};

use eframe::egui::Color32;

use crate::document::{Color, Document};
use crate::ui::glyph_render::{self, Surface};

/// The editor font; also used for the UI.
pub const FONT: &[u8] = include_bytes!("assets/FiraCode.ttf");

/// Width of one exported cell in pixels; the height follows the font's proportions.
pub const CELL_W: f32 = 10.0;

/// Renders the visible layers. Empty cells are black, or fully transparent if `transparent`.
///
/// A cell is exactly one glyph advance wide and one line high, so block and
/// box-drawing characters of neighbouring cells touch without seams.
pub fn render_png(doc: &Document, transparent: bool) -> Result<RgbaImage, String> {
    let font = FontRef::try_from_slice(FONT).map_err(|e| format!("font error: {e}"))?;

    // At a 1 px scale a line is exactly 1 px high, so the advance is the width : height ratio.
    let advance = font.as_scaled(PxScale::from(1.0)).h_advance(font.glyph_id('M'));
    let cell_h = (CELL_W / advance).round();
    let scale = PxScale { x: CELL_W / advance, y: cell_h };
    let scaled = font.as_scaled(scale);

    let (cell_w, cell_h) = (CELL_W as u32, cell_h as u32);
    let strokes = glyph_render::Strokes::new(CELL_W, 1.0);
    let (img_w, img_h) = (doc.width as u32 * cell_w, doc.height as u32 * cell_h);
    let base = if transparent { Rgba([0, 0, 0, 0]) } else { Rgba([0, 0, 0, 255]) };
    let mut img = RgbaImage::from_pixel(img_w, img_h, base);

    for cy in 0..doc.height {
        for cx in 0..doc.width {
            let cell = doc.composite_cell(cx, cy);
            let (px, py) = (cx as u32 * cell_w, cy as u32 * cell_h);

            if !cell.bg.is_transparent() {
                for y in py..py + cell_h {
                    for x in px..px + cell_w {
                        blend_over(img.get_pixel_mut(x, y), &cell.bg, 1.0);
                    }
                }
            }

            if cell.ch == ' ' { continue; }
            // Box-drawing, block and braille glyphs: the same shapes as on the canvas.
            let (x, y) = ((px as f32, (px + cell_w) as f32), (py as f32, (py + cell_h) as f32));
            if glyph_render::draw_on(&mut Pixels(&mut img), cell.ch, x, y, cell.fg.to_egui(), strokes) {
                continue;
            }
            let glyph_id = font.glyph_id(cell.ch);
            let left = px as f32 + (CELL_W - scaled.h_advance(glyph_id)) / 2.0;
            let glyph = glyph_id.with_scale_and_position(
                scale,
                ab_glyph::point(left, py as f32 + scaled.ascent()),
            );
            let Some(outlined) = font.outline_glyph(glyph) else { continue };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                let x = bounds.min.x as i32 + gx as i32;
                let y = bounds.min.y as i32 + gy as i32;
                if x >= 0 && y >= 0 && (x as u32) < img_w && (y as u32) < img_h {
                    blend_over(img.get_pixel_mut(x as u32, y as u32), &cell.fg, coverage);
                }
            });
        }
    }

    Ok(img)
}

/// The image being rendered, as a surface for procedural glyphs (with anti-aliased dots and lines).
struct Pixels<'a>(&'a mut RgbaImage);

impl Pixels<'_> {
    fn blend(&mut self, x: i64, y: i64, color: &Color, coverage: f32) {
        if x >= 0 && y >= 0 && (x as u32) < self.0.width() && (y as u32) < self.0.height() && coverage > 0.0 {
            blend_over(self.0.get_pixel_mut(x as u32, y as u32), color, coverage);
        }
    }

    /// Calls `coverage` for every pixel centre in the box and blends by the result.
    fn shade(&mut self, min: (f32, f32), max: (f32, f32), color: Color32, coverage: impl Fn(f32, f32) -> f32) {
        let color = Color::from_egui(color);
        for y in min.1.floor() as i64..max.1.ceil() as i64 {
            for x in min.0.floor() as i64..max.0.ceil() as i64 {
                let c = coverage(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, &color, c.clamp(0.0, 1.0));
            }
        }
    }
}

impl Surface for Pixels<'_> {
    fn fill(&mut self, x: (f32, f32), y: (f32, f32), color: Color32) {
        self.shade((x.0, y.0), (x.1, y.1), color, |_, _| 1.0);
    }

    fn dot(&mut self, (cx, cy): (f32, f32), radius: f32, color: Color32) {
        let r = radius + 1.0;
        self.shade((cx - r, cy - r), (cx + r, cy + r), color, |x, y| radius + 0.5 - ((x - cx).hypot(y - cy)));
    }

    fn polyline(&mut self, points: &[(f32, f32)], width: f32, color: Color32) {
        let half = width / 2.0;
        for pair in points.windows(2) {
            let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
            let pad = half + 1.0;
            let min = (ax.min(bx) - pad, ay.min(by) - pad);
            let max = (ax.max(bx) + pad, ay.max(by) + pad);
            self.shade(min, max, color, |x, y| {
                // Distance from (x, y) to the segment.
                let (dx, dy) = (bx - ax, by - ay);
                let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
                half + 0.5 - (x - ax - t * dx).hypot(y - ay - t * dy)
            });
        }
    }
}

/// Paints `src` (its alpha scaled by `coverage`) over `dst`; both are straight alpha.
fn blend_over(dst: &mut Rgba<u8>, src: &Color, coverage: f32) {
    let src_a = coverage.clamp(0.0, 1.0) * src.a as f32 / 255.0;
    if src_a <= 0.0 { return; }
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mix = |s: u8, d: u8| {
        ((s as f32 * src_a + d as f32 * dst_a * (1.0 - src_a)) / out_a).round() as u8
    };
    *dst = Rgba([
        mix(src.r, dst[0]),
        mix(src.g, dst[1]),
        mix(src.b, dst[2]),
        (out_a * 255.0).round() as u8,
    ]);
}
