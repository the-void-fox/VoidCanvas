use egui::*;
use crate::document::Color;
use crate::tools::border::BorderStyle;

pub struct ColorPanel {
    pub primary_fg:   Color,
    pub primary_bg:   Color,
    pub secondary_fg: Color,
    pub secondary_bg: Color,
    open_picker: Option<String>,
    picker_color: Color32,
}

impl Default for ColorPanel {
    fn default() -> Self {
        Self {
            primary_fg:   Color::WHITE,
            primary_bg:   Color::TRANSPARENT,
            secondary_fg: Color::new(128, 128, 128, 255),
            secondary_bg: Color::TRANSPARENT,
            open_picker:  None,
            picker_color: Color32::WHITE,
        }
    }
}

impl ColorPanel {
    pub fn show(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Colors").strong());
        ui.separator();

        ui.group(|ui| {
            ui.label("Primary");
            ui.horizontal(|ui| {
                ui.label("FG:");
                if self.color_button(ui, &self.primary_fg.clone(), "pri_fg") {
                    self.open_picker  = Some("pri_fg".into());
                    self.picker_color = self.primary_fg.to_egui();
                }
                ui.label("BG:");
                if self.color_button(ui, &self.primary_bg.clone(), "pri_bg") {
                    self.open_picker  = Some("pri_bg".into());
                    self.picker_color = self.primary_bg.to_egui();
                }
            });
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label("Secondary");
            ui.horizontal(|ui| {
                ui.label("FG:");
                if self.color_button(ui, &self.secondary_fg.clone(), "sec_fg") {
                    self.open_picker  = Some("sec_fg".into());
                    self.picker_color = self.secondary_fg.to_egui();
                }
                ui.label("BG:");
                if self.color_button(ui, &self.secondary_bg.clone(), "sec_bg") {
                    self.open_picker  = Some("sec_bg".into());
                    self.picker_color = self.secondary_bg.to_egui();
                }
            });
        });

        if let Some(ref target) = self.open_picker.clone() {
            ui.separator();
            ui.label(format!("Picking: {target}"));

            egui::color_picker::color_picker_color32(
                ui,
                &mut self.picker_color,
                egui::color_picker::Alpha::OnlyBlend,
            );

            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    let c = Color::from_egui(self.picker_color);
                    match target.as_str() {
                        "pri_fg" => self.primary_fg   = c,
                        "pri_bg" => self.primary_bg   = c,
                        "sec_fg" => self.secondary_fg = c,
                        "sec_bg" => self.secondary_bg = c,
                        _ => {}
                    }
                    self.open_picker = None;
                }
                if ui.button("Cancel").clicked() {
                    self.open_picker = None;
                }
            });

            ui.separator();
        }

        ui.add_space(4.0);
        ui.label("Presets (LClick → primary FG, RClick → secondary FG):");

        let presets: &[(u8, u8, u8)] = &[
            (255,255,255), (192,192,192), (128,128,128), (0,  0,  0  ),
            (255,0,  0  ), (128,0,  0  ), (255,255,0  ), (128,128,0  ),
            (0,  255,0  ), (0,  128,0  ), (0,  255,255), (0,  128,128),
            (0,  0,  255), (0,  0,  128), (255,0,  255), (128,0,  128),
        ];

        let cols = 8usize;
        Grid::new("color_presets").spacing([2.0, 2.0]).show(ui, |ui| {
            for (i, &(r, g, b)) in presets.iter().enumerate() {
                if i > 0 && i % cols == 0 {
                    ui.end_row();
                }
                let c32   = Color32::from_rgb(r, g, b);
                let color = Color::new(r, g, b, 255);

                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                ui.painter().rect_filled(rect, 2.0, c32);
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(1.0, Color32::from_gray(80)),
                    StrokeKind::Inside,
                );
                resp.clone().on_hover_text(format!("#{r:02X}{g:02X}{b:02X}"));

                if resp.clicked() {
                    self.primary_fg = color.clone();
                }
                if resp.secondary_clicked() {
                    self.secondary_fg = color;
                }
            }
        });
    }

    fn color_button(&self, ui: &mut Ui, color: &Color, _id: &str) -> bool {
        let size = Vec2::new(44.0, 22.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());

        if color.a == 0 {
            let hw = rect.width()  / 2.0;
            let hh = rect.height() / 2.0;
            ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
            for (dx, dy) in [(0.0, 0.0), (hw, hh)] {
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min + Vec2::new(dx, dy), Vec2::new(hw, hh)),
                    0.0,
                    Color32::LIGHT_GRAY,
                );
            }
        } else {
            ui.painter().rect_filled(rect, 2.0, color.to_egui());
        }

        ui.painter().rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, Color32::from_gray(100)),
            StrokeKind::Inside,
        );

        resp.clicked()
    }
}

pub struct ToolOptionsPanel {
    pub brush_size:    usize,
    pub filled:        bool,
    pub border_style:  BorderStyle,
}

impl Default for ToolOptionsPanel {
    fn default() -> Self {
        Self {
            brush_size:   1,
            filled:       false,
            border_style: BorderStyle::Single,
        }
    }
}

impl ToolOptionsPanel {
    pub fn show(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Tool Options").strong());
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Brush size:");
            ui.add(Slider::new(&mut self.brush_size, 1..=20));
        });

        ui.checkbox(&mut self.filled, "Filled shapes");

        ui.horizontal(|ui| {
            ui.label("Border:");
            ComboBox::from_id_salt("border_style")
                .selected_text(match self.border_style {
                    BorderStyle::Single => "Single ─│",
                    BorderStyle::Double => "Double ═║",
                    BorderStyle::Heavy  => "Heavy  ━┃",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.border_style,
                        BorderStyle::Single,
                        "Single ─│",
                    );
                    ui.selectable_value(
                        &mut self.border_style,
                        BorderStyle::Double,
                        "Double ═║",
                    );
                    ui.selectable_value(
                        &mut self.border_style,
                        BorderStyle::Heavy,
                        "Heavy  ━┃",
                    );
                });
        });
    }
}