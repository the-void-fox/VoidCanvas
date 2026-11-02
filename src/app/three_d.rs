//! The 3D window: makes and edits 3D layers (see `document::scene`).

use eframe::egui::{self, *};

use super::AsciiArtEditor;
use crate::document::scene::{Object3D, Scene3D, Style3D, parse_obj};
use crate::document::{Cell, Layer};
use crate::history::Command;
use crate::{figlet, icons, theme};

pub struct ThreeDWindow {
    /// The 3D layer being edited.
    pub layer: usize,
    /// Its scene as of the last undo step; edits since then become the next one.
    before: Option<Box<Scene3D>>,
    /// Frames for the turntable.
    turntable_frames: usize,
}

/// The font 3D text starts with: solid letters read best in depth.
fn solid_font() -> usize {
    figlet::fonts().iter().position(|f| f.name == "Banner").unwrap_or(0)
}

impl AsciiArtEditor {
    /// A new 3D layer above the active one, showing `scene`, with the 3D window open on it.
    pub(super) fn add_3d_layer(&mut self, name: &str, scene: Scene3D) {
        let mut layer = Layer::new(self.doc.width, self.doc.height, name);
        layer.scene = Some(Box::new(scene));
        let index = self.doc.active_layer + 1;
        self.stop_playback();
        self.history.execute_as(Command::LayerAdd { index, layer }, "New 3D Layer", &mut self.doc);
        self.edit_3d(index);
    }

    /// A model from an OBJ file: into the 3D window's layer if it is open (one undo step), else
    /// into a new 3D layer.
    pub(super) fn load_model(&mut self, path: &std::path::Path) {
        let text = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                self.show_status(format!("Can't read the model: {e}"));
                return;
            }
        };
        let (vertices, faces) = match parse_obj(&text) {
            Ok(mesh) => mesh,
            Err(e) => {
                self.show_status(format!("Can't read the model: {e}"));
                return;
            }
        };
        let name = path.file_stem().map_or_else(|| "Model".to_owned(), |s| s.to_string_lossy().to_string());
        let triangles = faces.len();
        let object = Object3D::Mesh { name: name.clone(), vertices, faces, smooth: true };
        match self.three_d.as_ref().map(|w| w.layer) {
            Some(index) if self.doc.layers.get(index).is_some_and(|l| l.scene.is_some()) => {
                if let Some(scene) = self.doc.layers[index].scene.as_mut() {
                    scene.object = object;
                }
                self.doc.render_3d_frame(self.doc.frame);
                self.commit_3d();
            }
            _ => self.add_3d_layer(&name, Scene3D { object, turn: [15.0, -30.0, 0.0], size: 0.8, ..Scene3D::default() }),
        }
        self.show_status(format!("{name}: {triangles} triangles"));
    }

    /// The selected cells of the active layer, or of the nearest 2D layer below a 3D one.
    fn selected_art(&self) -> Option<Vec<Vec<Cell>>> {
        let (x1, y1, x2, y2) = self.tools.selection.selection?;
        let layer = self.doc.layers[..=self.doc.active_layer].iter().rev().find(|l| l.scene.is_none())?;
        let cells: Vec<Vec<Cell>> = (y1..=y2).map(|y| (x1..=x2).map(|x| layer.get(x, y).cloned().unwrap_or_default()).collect()).collect();
        (!cells.iter().flatten().all(Cell::is_empty)).then_some(cells)
    }

    /// The selected art of the active layer, pushed out into a 3D layer in place.
    pub(super) fn add_3d_from_selection(&mut self) {
        let Some((x1, y1, x2, y2)) = self.tools.selection.selection else {
            self.show_status("Select the art to turn into 3D");
            return;
        };
        let Some(cells) = self.selected_art() else {
            self.show_status("The selection is empty on this layer");
            return;
        };
        // Face-on, it covers the selection exactly; a slight turn shows the depth.
        let (w, h) = (self.doc.width as f32, self.doc.height as f32 * 2.0);
        let (sw, sh) = ((x2 - x1 + 1) as f32, (y2 - y1 + 1) as f32 * 2.0);
        let scene = Scene3D {
            object: Object3D::Art { cells, depth: 3.0 },
            style: Style3D::Blocks,
            turn: [12.0, -20.0, 0.0],
            size: sw.max(sh) / w.min(h),
            position: [(x1 as f32 + sw / 2.0) / w - 0.5, (y1 as f32 * 2.0 + sh / 2.0) / h - 0.5],
            ..Scene3D::default()
        };
        self.tools.selection.selection = None;
        self.add_3d_layer("3D Art", scene);
    }

    pub(super) fn edit_3d(&mut self, layer: usize) {
        let Some(scene) = self.doc.layers.get(layer).and_then(|l| l.scene.clone()) else { return };
        self.three_d = Some(ThreeDWindow { layer, before: Some(scene), turntable_frames: 24 });
    }

    /// The active 3D layer becomes an ordinary one, its cells as they are.
    pub(super) fn rasterize_3d(&mut self) {
        let index = self.doc.active_layer;
        let Some(old) = self.doc.active_layer().scene.clone() else { return };
        self.commit_3d();
        self.finish_3d_now();
        self.history.execute_as(Command::Scene { index, old: Some(old), new: None }, "Rasterize 3D", &mut self.doc);
        self.three_d = None;
    }

    /// Records pending edits and closes the window (the layers are about to change under it).
    pub(super) fn close_3d(&mut self) {
        self.commit_3d();
        self.three_d = None;
    }

    /// Records the edits made in the window since the last undo step.
    fn commit_3d(&mut self) {
        let Some(window) = &mut self.three_d else { return };
        let index = window.layer;
        let Some(now) = self.doc.layers.get(index).and_then(|l| l.scene.clone()) else { return };
        if window.before.as_ref() != Some(&now) {
            let old = window.before.replace(now.clone());
            self.history.push_as(Command::Scene { index, old, new: Some(now) }, "Edit 3D");
            self.doc.invalidate_3d();
        }
    }

    /// Several frames turning the object once around: one undo step.
    pub(crate) fn turntable(&mut self, frames: usize) {
        self.commit_3d();
        let Some(window) = &self.three_d else { return };
        let index = window.layer;
        let Some(old) = self.doc.layers[index].scene.clone() else { return };
        let mut cmds = Vec::new();
        for i in self.doc.frame_count()..frames {
            cmds.push(Command::FrameAdd { index: i, data: self.doc.frame_data(self.doc.frame_count() - 1) });
        }
        let total = frames.max(self.doc.frame_count());
        let mut new = old.clone();
        new.spin = [0.0, 360.0 / total as f32, 0.0];
        cmds.push(Command::Scene { index, old: Some(old), new: Some(new.clone()) });
        self.stop_playback();
        self.history.execute_as(Command::Group(cmds), "Turntable", &mut self.doc);
        if let Some(window) = &mut self.three_d {
            window.before = Some(new);
        }
        self.show_status(format!("{total} frames: press Enter to play"));
    }

    pub(super) fn three_d_window(&mut self, ctx: &egui::Context) {
        let Some(window) = &self.three_d else { return };
        let index = window.layer;
        let Some(current) = self.doc.layers.get(index).and_then(|l| l.scene.clone()) else {
            self.three_d = None;
            return;
        };
        let mut scene = (*current).clone();
        let mut frames = window.turntable_frames;
        let (mut open, mut rasterize, mut turntable, mut take_selection) = (true, false, false, false);
        let mut manage_ramps = false;
        let mut load_model = false;
        let has_selection = self.tools.selection.selection.is_some();
        let name = self.doc.layers[index].name.clone();

        egui::Window::new(format!("3D  ·  {name}"))
            .id(Id::new("three_d"))
            .open(&mut open)
            .default_pos(pos2(ctx.content_rect().right() - 700.0, 110.0))
            .resizable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                row(ui, "Object", |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            for kind in [Object3D::Cube, Object3D::Sphere, Object3D::Torus, Object3D::Cylinder, Object3D::Cone] {
                                let on = std::mem::discriminant(&scene.object) == std::mem::discriminant(&kind);
                                if ui.selectable_label(on, kind.name()).clicked() {
                                    scene.object = kind;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            let text = matches!(scene.object, Object3D::Text { .. });
                            if ui.selectable_label(text, "Text").clicked() && !text {
                                scene.object = Object3D::Text { text: "VOID".into(), font: solid_font(), depth: 4.0 };
                            }
                            let model = matches!(scene.object, Object3D::Mesh { .. });
                            if ui.selectable_label(model, "Model").on_hover_text("A 3D model from an OBJ file").clicked() && !model {
                                load_model = true;
                            }
                            let art = matches!(scene.object, Object3D::Art { .. });
                            if ui.add_enabled(art || has_selection, Button::selectable(art, "Art"))
                                .on_hover_text("The selected art, pushed out into blocks")
                                .on_disabled_hover_text("Select some art first")
                                .clicked()
                                && !art
                            {
                                take_selection = true;
                            }
                        });
                    });
                });

                match &mut scene.object {
                    Object3D::Text { text, font, depth } => row(ui, "Text", |ui| {
                        ui.add(TextEdit::singleline(text).desired_width(110.0));
                        let fonts = figlet::fonts();
                        ComboBox::from_id_salt("three_d_font")
                            .selected_text(fonts.get(*font).map_or("", |f| f.name))
                            .show_ui(ui, |ui| {
                                for (i, f) in fonts.iter().enumerate() {
                                    ui.selectable_value(font, i, f.name);
                                }
                            });
                        ui.label("Depth");
                        ui.add(DragValue::new(depth).range(0.5..=40.0).speed(0.1).fixed_decimals(1));
                    }),
                    Object3D::Mesh { name, faces, smooth, .. } => row(ui, "Model", |ui| {
                        ui.label(RichText::new(format!("{name}  ·  {} triangles", faces.len())).color(theme::TEXT_DIM));
                        if ui.button("Load OBJ…").clicked() {
                            load_model = true;
                        }
                        ui.toggle_value(smooth, "Smooth").on_hover_text("Smooth shading across faces (off: flat faces)");
                    }),
                    Object3D::Art { depth, .. } => row(ui, "Art", |ui| {
                        if ui.add_enabled(has_selection, Button::new("Take Selection")).on_hover_text("Use the selected art instead").clicked() {
                            take_selection = true;
                        }
                        ui.label("Depth");
                        ui.add(DragValue::new(depth).range(0.5..=40.0).speed(0.1).fixed_decimals(1));
                    }),
                    _ => {}
                }

                row(ui, "Look", |ui| {
                    ComboBox::from_id_salt("three_d_style").selected_text(scene.style.name()).show_ui(ui, |ui| {
                        for style in Style3D::ALL {
                            ui.selectable_value(&mut scene.style, style, style.name());
                        }
                    });
                    if scene.style == Style3D::Shade {
                        let ramps = self.settings.all_ramps();
                        let name = ramps.iter().find(|r| r.glyphs == scene.ramp).map_or("Custom", |r| r.name.as_str()).to_owned();
                        match super::chrome::ramp_menu(ui, "three_d_ramp", &name, &ramps) {
                            Some(Some(ramp)) => scene.ramp = ramp.glyphs,
                            Some(None) => manage_ramps = true,
                            None => {}
                        }
                    }
                    let mut color = scene.color.to_egui();
                    if color_picker::color_edit_button_srgba(ui, &mut color, color_picker::Alpha::Opaque).changed() {
                        scene.color = crate::document::Color::from_egui(color);
                    }
                    ui.toggle_value(&mut scene.shine, "Shine");
                });

                row(ui, "Turn", |ui| {
                    turn_pad(ui, &mut scene.turn);
                    ui.vertical(|ui| {
                        ui.spacing_mut().slider_width = 150.0;
                        for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(*label);
                                ui.add(Slider::new(&mut scene.turn[axis], -180.0..=180.0).suffix("°").fixed_decimals(0));
                            });
                        }
                    });
                });

                row(ui, "Place", |ui| {
                    ui.label("Size");
                    ui.add(DragValue::new(&mut scene.size).range(0.05..=3.0).speed(0.01).fixed_decimals(2));
                    ui.label("X");
                    ui.add(DragValue::new(&mut scene.position[0]).range(-1.0..=1.0).speed(0.005).fixed_decimals(2));
                    ui.label("Y");
                    ui.add(DragValue::new(&mut scene.position[1]).range(-1.0..=1.0).speed(0.005).fixed_decimals(2));
                });

                row(ui, "Light", |ui| {
                    ui.label("Side");
                    ui.add(DragValue::new(&mut scene.light[0]).range(-180.0..=180.0).suffix("°").fixed_decimals(0));
                    ui.label("Up");
                    ui.add(DragValue::new(&mut scene.light[1]).range(-90.0..=90.0).suffix("°").fixed_decimals(0));
                    ui.label("Ambient");
                    ui.add(DragValue::new(&mut scene.ambient).range(0.0..=1.0).speed(0.01).fixed_decimals(2));
                });

                row(ui, "Animation", |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Spin per frame");
                            for axis in 0..3 {
                                ui.add(DragValue::new(&mut scene.spin[axis]).range(-180.0..=180.0).suffix("°").fixed_decimals(1));
                            }
                        });
                        ui.horizontal(|ui| {
                            turntable = ui.button(format!("{}  Turntable", icons::PLAY)).on_hover_text("Frames that turn the object once around").clicked();
                            ui.add(DragValue::new(&mut frames).range(2..=120).suffix(" frames"));
                        });
                    });
                });
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    rasterize = ui.button("Rasterize").on_hover_text("Make it an ordinary layer to paint on").clicked();
                    ui.label(RichText::new("The layer shows the scene; painting tools leave it alone").small().color(theme::TEXT_FAINT));
                });
            });

        if take_selection {
            match self.selected_art() {
                Some(cells) => {
                    let depth = if let Object3D::Art { depth, .. } = scene.object { depth } else { 3.0 };
                    scene.object = Object3D::Art { cells, depth };
                }
                None => self.show_status("The selection is empty on the layer below"),
            }
        }
        if let Some(window) = &mut self.three_d {
            window.turntable_frames = frames;
        }
        // Live: the frame in view follows every change; the rest catch up with the undo step.
        if scene != *current {
            self.doc.layers[index].scene = Some(Box::new(scene));
            self.doc.render_3d_frame(self.doc.frame);
            self.doc.modified = true;
        }
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.text_edit_focused();
        if !busy {
            self.commit_3d();
        }
        if turntable {
            self.turntable(frames);
        }
        if rasterize {
            self.doc.active_layer = index;
            self.rasterize_3d();
        }
        if !open {
            self.commit_3d();
            self.three_d = None;
        }
        if load_model {
            self.choose_file(super::file_dialog::FileAction::LoadModel);
        }
        if manage_ramps {
            self.open_library(super::library::LibraryTab::Ramps);
        }
    }
}

/// A labelled line of the window: the label in a column of its own.
fn row(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(76.0, 20.0), Sense::hover());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(13.0), theme::TEXT_DIM);
        content(ui);
    });
}

/// Drag inside to turn the object: sideways around y, up and down around x.
fn turn_pad(ui: &mut Ui, turn: &mut [f32; 3]) {
    let (rect, resp) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, theme::BG_DEEP);
    let c = rect.center();
    painter.circle_stroke(c, 32.0, Stroke::new(1.0, theme::BORDER_STRONG));
    // The axes as they point now.
    let [x, y, _] = turn.map(f32::to_radians);
    let axis = |dx: f32, dy: f32, color: Color32| painter.line_segment([c, c + vec2(dx, dy) * 28.0], Stroke::new(2.0, color));
    axis(y.cos(), y.sin() * x.sin(), Color32::from_rgb(248, 113, 113));
    axis(0.0, -x.cos(), Color32::from_rgb(134, 239, 172));
    axis(-y.sin(), y.cos() * x.sin(), Color32::from_rgb(120, 170, 250));
    painter.text(rect.left_bottom() + vec2(6.0, -4.0), Align2::LEFT_BOTTOM, "drag", theme::mono(10.0), theme::TEXT_FAINT);
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    let delta = resp.drag_delta();
    let wrap = |a: f32| (a + 180.0).rem_euclid(360.0) - 180.0;
    turn[1] = wrap(turn[1] + delta.x * 0.9);
    turn[0] = wrap(turn[0] + delta.y * 0.9);
}
