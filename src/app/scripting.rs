//! Edit → Script…: write and run Rhai scripts that draw on the active layer.

use eframe::egui::{self, *};

use super::AsciiArtEditor;
use crate::script::{self, EXAMPLES, REFERENCE, ScriptContext};
use crate::tools::{ToolType, brush::BrushKind};
use crate::{icons, theme};

pub struct ScriptWindow {
    pub code: String,
    /// What the last run printed; `true` marks an error.
    output: Vec<(bool, String)>,
    show_reference: bool,
}

impl AsciiArtEditor {
    pub(super) fn open_script(&mut self) {
        if self.script.is_some() { return; }
        let code = if self.settings.script.trim().is_empty() { EXAMPLES[0].1.to_owned() } else { self.settings.script.clone() };
        self.script = Some(ScriptWindow { code, output: Vec::new(), show_reference: false });
    }

    /// Runs the script in the window; everything it changes becomes one undo step.
    pub(crate) fn run_script(&mut self) {
        if self.script.is_none() { return; }
        self.finish_3d_now();
        let Some(window) = &self.script else { return };
        let code = window.code.clone();
        self.settings.set_script(&code);
        let mut output: Vec<(bool, String)> = Vec::new();
        if self.doc.active_layer().locked || self.doc.active_layer().scene.is_some() {
            output.push((true, "The active layer is locked or 3D".into()));
        } else {
            let area = match self.tools.selection.selection {
                Some((x1, y1, x2, y2)) => (x1, y1, x2 + 1, y2 + 1),
                None => (0, 0, self.doc.width, self.doc.height),
            };
            let panel = &self.color_panel;
            let ctx = ScriptContext {
                layer: self.doc.active_layer,
                primary_char: self.primary_char,
                primary_fg: panel.primary_fg.clone(),
                secondary_fg: panel.secondary_fg.clone(),
                area,
            };
            match script::run(&code, &self.doc, ctx) {
                Ok(result) => {
                    output.extend(result.log.into_iter().map(|line| (false, line)));
                    match script::changes(&self.doc, &result.doc, self.doc.active_layer) {
                        Some(cmd) => {
                            self.stop_playback();
                            self.history.execute_as(cmd, "Script", &mut self.doc);
                            self.doc.show_frame(result.doc.frame);
                            self.show_status("Script done");
                        }
                        None => output.push((false, "The script changed nothing".into())),
                    }
                }
                Err((error, log)) => {
                    output.extend(log.into_iter().map(|line| (false, line)));
                    output.push((true, error));
                }
            }
        }
        if let Some(window) = &mut self.script {
            window.output = output;
        }
    }

    /// The window's script becomes the Brush, if it has `fn brush(x, y, dx, dy)`.
    fn use_script_as_brush(&mut self) {
        let Some(window) = &mut self.script else { return };
        match script::check_brush(&window.code) {
            Ok(()) => {
                window.output = vec![(false, "The Brush paints with this script now".into())];
                self.tool_options.brush = BrushKind::Script { code: window.code.clone() };
                self.tool_options.brush_name = "Script Brush".into();
                self.active_tool = ToolType::Brush;
            }
            Err(e) => window.output = vec![(true, e)],
        }
    }

    pub(super) fn script_window(&mut self, ctx: &egui::Context) {
        let Some(window) = &mut self.script else { return };
        let mut open = true;
        let mut run = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        let mut use_as_brush = false;
        egui::Window::new("Script")
            .open(&mut open)
            .default_pos(pos2(70.0, 110.0))
            .default_size([560.0, 520.0])
            .min_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    run |= ui.button(format!("{}  Run", icons::PLAY)).on_hover_text("Ctrl+Enter").clicked();
                    use_as_brush = ui.button("Use as Brush").on_hover_text("Paint with this script's fn brush(x, y, dx, dy)").clicked();
                    ComboBox::from_id_salt("script_examples").selected_text("Examples").show_ui(ui, |ui| {
                        for (name, code) in EXAMPLES {
                            if ui.selectable_label(false, *name).clicked() {
                                window.code = (*code).to_owned();
                            }
                        }
                    });
                    ui.toggle_value(&mut window.show_reference, "Reference");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.hyperlink_to("Rhai language", "https://rhai.rs/book/");
                    });
                });
                if window.show_reference {
                    ui.add_space(4.0);
                    Grid::new("script_reference").num_columns(2).spacing([14.0, 3.0]).striped(true).show(ui, |ui| {
                        for (call, what) in REFERENCE {
                            ui.label(RichText::new(*call).font(theme::mono(12.0)).color(theme::ACCENT));
                            ui.label(RichText::new(*what).color(theme::TEXT_DIM));
                            ui.end_row();
                        }
                    });
                }
                ui.add_space(4.0);
                let output_height = 110.0;
                ScrollArea::vertical()
                    .id_salt("script_code")
                    .max_height((ui.available_height() - output_height).max(120.0))
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut window.code)
                                .code_editor()
                                .font(theme::mono(13.0))
                                .desired_rows(14)
                                .desired_width(f32::INFINITY),
                        );
                    });
                ui.add_space(4.0);
                Frame::new().fill(theme::BG_DEEP).corner_radius(6.0).inner_margin(6.0).show(ui, |ui| {
                    ScrollArea::vertical().id_salt("script_output").max_height(output_height).stick_to_bottom(true).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if window.output.is_empty() {
                            ui.label(RichText::new("Output of print() and errors show here").color(theme::TEXT_FAINT));
                        }
                        for (error, line) in &window.output {
                            let color = if *error { theme::WARNING } else { theme::TEXT_DIM };
                            ui.label(RichText::new(line).font(theme::mono(12.0)).color(color));
                        }
                    });
                });
            });
        if !open {
            self.script = None;
        } else if run {
            self.run_script();
        } else if use_as_brush {
            self.use_script_as_brush();
        }
    }
}
