#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(debug_assertions)]
mod dev;
mod document;
mod export;
mod history;
mod icons;
mod tools;
mod theme;
mod ui;

#[cfg(test)]
mod tests;

use app::AsciiArtEditor;

fn main() -> eframe::Result<()> {
    let icon = eframe::icon_data::from_png_bytes(
        include_bytes!("assets/icon.png")
    ).ok();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Void Canvas")
        .with_min_inner_size([1000.0, 640.0])
        .with_inner_size([1400.0, 900.0]);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon_data));
    }
    // Debug screenshots shouldn't steal keyboard focus from whatever the user is doing.
    #[cfg(debug_assertions)]
    if std::env::var_os("VOID_CANVAS_SCREENSHOT").is_some() {
        viewport = viewport.with_active(false);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Void Canvas",
        options,
        Box::new(|cc| {
            let mut app = AsciiArtEditor::new(cc);
            if let Some(path) = std::env::args_os().nth(1) {
                app.open_path(std::path::Path::new(&path));
            }
            Ok(Box::new(app))
        }),
    )
}