//! Rendering the document into image files.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};

use eframe::egui::Color32;

use crate::document::{Color, Document};
use crate::ui::glyph_render::{self, Surface};

/// The editor font; also used for the UI.
pub const FONT: &[u8] = include_bytes!("assets/FiraCode.ttf");
/// For the glyphs FiraCode lacks (braille, sextants, many symbols).
pub const FALLBACK_FONT: &[u8] = include_bytes!("assets/AdwaitaMono-Regular.ttf");

/// Width of one exported cell in pixels; the height follows the font's proportions.
pub const CELL_W: f32 = 10.0;

/// Renders the visible layers. Empty cells are black, or fully transparent if `transparent`.
///
/// A cell is exactly one glyph advance wide and one line high, so block and
/// box-drawing characters of neighbouring cells touch without seams.
pub fn render_png(doc: &Document, transparent: bool) -> Result<RgbaImage, String> {
    render_frame(doc, doc.frame, transparent)
}

/// Colour quantisation effort for GIF frames: 1 is slowest and best, 30 fastest.
const GIF_SPEED: i32 = 10;

/// An animated GIF of every frame, looping forever. Frames are rendered and compressed on
/// every core; `written` hears how many are in the file so far and answers whether to go on.
/// A GIF left unfinished (stopped, or failed) is removed.
pub fn write_gif(doc: &Document, path: &Path, mut written: impl FnMut(usize) -> bool) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let result = encode_gif(doc, std::io::BufWriter::new(file), &mut written);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

fn encode_gif(doc: &Document, out: impl Write, written: &mut dyn FnMut(usize) -> bool) -> Result<(), String> {
    let count = doc.frame_count();
    let (next, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(count);
    std::thread::scope(|scope| {
        let (send, frames) = std::sync::mpsc::channel();
        for _ in 0..workers {
            let (send, next, stop) = (send.clone(), &next, &stop);
            scope.spawn(move || {
                loop {
                    let frame = next.fetch_add(1, Ordering::Relaxed);
                    if frame >= count || stop.load(Ordering::Relaxed) || send.send((frame, gif_frame(doc, frame))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);

        let result = write_in_order(&frames, count, out, written);
        // The workers stop after the frame in hand.
        stop.store(true, Ordering::Relaxed);
        result
    })
}

/// A frame number and the frame, ready for the file.
type EncodedFrame = (usize, Result<gif::Frame<'static>, String>);

/// Frames finish in any order and go into the file in theirs.
fn write_in_order(frames: &Receiver<EncodedFrame>, count: usize, out: impl Write, written: &mut dyn FnMut(usize) -> bool) -> Result<(), String> {
    let (mut out, mut encoder) = (Some(out), None);
    let mut early = BTreeMap::new();
    let mut done = 0;
    for (frame, encoded) in frames {
        early.insert(frame, encoded?);
        while let Some(frame) = early.remove(&done) {
            let encoder = match &mut encoder {
                Some(encoder) => encoder,
                None => {
                    let mut first = gif::Encoder::new(out.take().unwrap(), frame.width, frame.height, &[]).map_err(|e| e.to_string())?;
                    first.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
                    encoder.insert(first)
                }
            };
            encoder.write_lzw_pre_encoded_frame(&frame).map_err(|e| e.to_string())?;
            done += 1;
            if !written(done) {
                return Err("stopped".into());
            }
        }
    }
    let Some(encoder) = encoder.filter(|_| done == count) else { return Err("a frame went missing".into()) };
    encoder.into_inner().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())
}

/// One frame for a GIF, its colours reduced to a palette and compressed.
fn gif_frame(doc: &Document, frame: usize) -> Result<gif::Frame<'static>, String> {
    let image = render_frame(doc, frame, false)?;
    let too_large = |_| format!("{}×{} pixels is too large for a GIF", image.width(), image.height());
    let (width, height) = (u16::try_from(image.width()).map_err(too_large)?, u16::try_from(image.height()).map_err(too_large)?);
    let mut pixels = image.into_raw();
    let mut frame_out = gif::Frame::from_rgba_speed(width, height, &mut pixels, GIF_SPEED);
    // GIF counts in hundredths of a second.
    frame_out.delay = u16::try_from(doc.durations[frame] / 10).unwrap_or(u16::MAX);
    frame_out.dispose = gif::DisposalMethod::Background;
    frame_out.make_lzw_pre_encoded();
    Ok(frame_out)
}

/// One animation frame as an image.
pub fn render_frame(doc: &Document, frame: usize, transparent: bool) -> Result<RgbaImage, String> {
    let font = FontRef::try_from_slice(FONT).map_err(|e| format!("font error: {e}"))?;
    let fallback = FontRef::try_from_slice(FALLBACK_FONT).map_err(|e| format!("font error: {e}"))?;

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
            let stack = doc.stack_in(frame, cx, cy);
            let (px, py) = (cx as u32 * cell_w, cy as u32 * cell_h);

            if !stack.bg.is_transparent() {
                for y in py..py + cell_h {
                    for x in px..px + cell_w {
                        blend_over(img.get_pixel_mut(x, y), &stack.bg, 1.0);
                    }
                }
            }

            // Every glyph, bottom first (more than one where a layer overlays).
            for (ch, fg) in &stack.glyphs {
                // Box-drawing, block and braille glyphs: the same shapes as on the canvas.
                let (x, y) = ((px as f32, (px + cell_w) as f32), (py as f32, (py + cell_h) as f32));
                if glyph_render::draw_on(&mut Pixels(&mut img), *ch, x, y, fg.to_egui(), strokes) {
                    continue;
                }
                // A glyph FiraCode lacks comes from the fallback font, at the same size.
                let face = if font.glyph_id(*ch).0 == 0 { &fallback } else { &font };
                let glyph_id = face.glyph_id(*ch);
                let left = px as f32 + (CELL_W - face.as_scaled(scale).h_advance(glyph_id)) / 2.0;
                let glyph = glyph_id.with_scale_and_position(
                    scale,
                    ab_glyph::point(left, py as f32 + scaled.ascent()),
                );
                let Some(outlined) = face.outline_glyph(glyph) else { continue };
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let x = bounds.min.x as i32 + gx as i32;
                    let y = bounds.min.y as i32 + gy as i32;
                    if x >= 0 && y >= 0 && (x as u32) < img_w && (y as u32) < img_h {
                        blend_over(img.get_pixel_mut(x as u32, y as u32), fg, coverage);
                    }
                });
            }
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

/// Cells `x0..x1` × `y0..y1` of the composited document, as they look.
fn visible_rows(doc: &Document, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> Vec<Vec<crate::document::Cell>> {
    (y0..y1.min(doc.height)).map(|y| (x0..x1.min(doc.width)).map(|x| doc.composite_cell(x, y)).collect()).collect()
}

/// The whole document as a region for [`to_ansi`] / [`to_html`].
pub fn whole(doc: &Document) -> (usize, usize, usize, usize) {
    (0, 0, doc.width, doc.height)
}

/// Plain text of a region, trailing spaces trimmed.
pub fn to_text(doc: &Document, region: (usize, usize, usize, usize)) -> String {
    let lines: Vec<String> = visible_rows(doc, region)
        .iter()
        .map(|row| row.iter().map(|c| c.ch).collect::<String>().trim_end().to_string())
        .collect();
    lines.join("\n").trim_end_matches('\n').to_string()
}

/// Text with 24-bit ANSI colours, for terminals (`cat art.ans`). Colour codes are only
/// emitted when they change; empty cells at the end of a line are dropped.
pub fn to_ansi(doc: &Document, region: (usize, usize, usize, usize)) -> String {
    let mut out = String::new();
    for row in visible_rows(doc, region) {
        let end = row.iter().rposition(|c| !c.is_empty()).map_or(0, |i| i + 1);
        let (mut fg, mut bg): (Option<Color>, Option<Color>) = (None, None);
        for cell in &row[..end] {
            let want_bg = (!cell.bg.is_transparent()).then(|| cell.bg.clone());
            if want_bg != bg {
                match &want_bg {
                    Some(c) => out.push_str(&format!("\x1b[48;2;{};{};{}m", c.r, c.g, c.b)),
                    None => out.push_str("\x1b[49m"),
                }
                bg = want_bg;
            }
            if cell.ch != ' ' && fg.as_ref() != Some(&cell.fg) {
                let c = &cell.fg;
                out.push_str(&format!("\x1b[38;2;{};{};{}m", c.r, c.g, c.b));
                fg = Some(cell.fg.clone());
            }
            out.push(cell.ch);
        }
        if fg.is_some() || bg.is_some() {
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// A standalone HTML page showing the art in a `<pre>` block.
pub fn to_html(doc: &Document, title: &str) -> String {
    let hex = |c: &Color| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut body = String::new();
    for row in visible_rows(doc, whole(doc)) {
        // Runs of cells with the same colours become one <span>.
        let mut i = 0;
        while i < row.len() {
            let style = |c: &crate::document::Cell| {
                let fg = if c.ch == ' ' { String::new() } else { format!("color:{};", hex(&c.fg)) };
                let bg = if c.bg.is_transparent() { String::new() } else { format!("background:{};", hex(&c.bg)) };
                fg + bg.as_str()
            };
            let current = style(&row[i]);
            let mut text = String::new();
            while i < row.len() && (style(&row[i]) == current || (row[i].ch == ' ' && row[i].bg.is_transparent() && current.is_empty())) {
                text.push(row[i].ch);
                i += 1;
            }
            if current.is_empty() {
                body.push_str(&escape(&text));
            } else {
                body.push_str(&format!("<span style=\"{current}\">{}</span>", escape(&text)));
            }
        }
        body.push('\n');
    }
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n\
         <body style=\"background:#000;margin:2em\">\n\
         <pre style=\"color:#fff;font:16px/1.25 'Fira Code','DejaVu Sans Mono',monospace;margin:0\">\n{body}</pre>\n</body>\n</html>\n",
        escape(title)
    )
}
