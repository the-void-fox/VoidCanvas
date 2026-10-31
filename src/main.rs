#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
#[cfg(debug_assertions)]
mod dev;
mod document;
mod export;
mod history;
mod tools;
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
        .with_min_inner_size([900.0, 600.0])
        .with_inner_size([1400.0, 900.0]);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon_data));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Void Canvas",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_theme(egui::Theme::Dark);

            let mut fonts = egui::FontDefinitions::default();
            let font_bytes = export::FONT.to_vec();
            fonts.font_data.insert(
                "FiraCode".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_bytes)),
            );

            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "FiraCode".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "FiraCode".to_owned());

            cc.egui_ctx.set_fonts(fonts);
            // Zoom on top of the system DPI scale (set_pixels_per_point would replace it).
            cc.egui_ctx.set_zoom_factor(1.2);
            // Ctrl +/-/0 zoom the canvas, not the whole UI.
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);

            Ok(Box::new(AsciiArtEditor::default()))
        }),
    )
}