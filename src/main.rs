#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod document;
mod history;
mod tools;
mod ui;

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
            cc.egui_ctx.set_visuals(egui::Visuals::dark());

            let mut fonts = egui::FontDefinitions::default();
            let font_bytes = include_bytes!("assets/FiraCode.ttf").to_vec();
            fonts.font_data.insert(
                "FiraCode".to_owned(),
                egui::FontData::from_owned(font_bytes).into(),
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
            cc.egui_ctx.set_pixels_per_point(1.2);

            Box::new(AsciiArtEditor::default())
        }),
    )
}