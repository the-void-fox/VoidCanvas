//! Debug-build helpers for checking the UI without clicking through it.

use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui::{self, Event, ViewportCommand};

/// Frames to wait before the screenshot, so fonts and layout have settled.
const SETTLE_FRAMES: u32 = 10;

static FRAME: AtomicU32 = AtomicU32::new(0);

/// With `VOID_CANVAS_SCREENSHOT=out.png` set, saves a screenshot of the window and quits.
pub fn screenshot_hook(ctx: &egui::Context) {
    let Ok(path) = std::env::var("VOID_CANVAS_SCREENSHOT") else { return };

    let frame = FRAME.fetch_add(1, Ordering::Relaxed);
    if frame < SETTLE_FRAMES {
        ctx.request_repaint();
    } else if frame == SETTLE_FRAMES {
        ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
    }

    let image = ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(image) = image {
        let [w, h] = image.size;
        let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        if let Err(e) = image::save_buffer(&path, &rgba, w as u32, h as u32, image::ColorType::Rgba8) {
            eprintln!("screenshot failed: {e}");
        }
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}
