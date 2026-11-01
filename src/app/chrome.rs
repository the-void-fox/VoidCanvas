//! The window chrome around the canvas: menu, tool options, tool strip, dock and status bar.

use eframe::egui::{self, *};

use super::{AsciiArtEditor, DockTab, commands::Action, file_dialog::FileAction};
use crate::glyphs::Transform;
use crate::icons;
use crate::theme;
use crate::tools::{ToolType, border::BorderStyle, brush::{BrushKind, BrushPreset, builtin_presets}, gradient::GradientShape, pixel::PixelGrid};
use crate::library::Ramp;
use super::library::LibraryTab;
use crate::ui::panels::ToolOptions;
use crate::ui::widgets::{self, color_chip, icon_button, icon_toggle, tab_bar, to_hex, vdivider};

/// Selection transforms with their menu label and shortcut.
pub(super) const TRANSFORMS: &[(Transform, &str, &str)] = &[
    (Transform::FlipHorizontal, "Flip Horizontal", "H"),
    (Transform::FlipVertical, "Flip Vertical", "V"),
    (Transform::RotateCcw, "Rotate 90° Left", ""),
    (Transform::RotateCw, "Rotate 90° Right", ""),
];

impl AsciiArtEditor {
    pub(super) fn menu_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("menu")
            .frame(Frame::NONE.fill(theme::BG_DEEP).inner_margin(Margin::symmetric(8, 3)))
            .show(ui, |ui| {
                MenuBar::new().ui(ui, |ui| {
                    ui.label(RichText::new("▞").font(theme::mono(16.0)).color(theme::ACCENT));
                    ui.add_space(2.0);

                    ui.menu_button("File", |ui| {
                        if ui.add(Button::new("New…").shortcut_text("Ctrl+N")).clicked() { self.run(Action::New, &ctx); }
                        if ui.add(Button::new("Open…").shortcut_text("Ctrl+O")).clicked() { self.run(Action::Open, &ctx); }
                        ui.menu_button("Open Recent", |ui| {
                            if self.settings.recent_files.is_empty() {
                                ui.label(RichText::new("No recent files").color(theme::TEXT_FAINT));
                            }
                            for path in self.settings.recent_files.clone() {
                                let name = std::path::Path::new(&path)
                                    .file_name()
                                    .map_or_else(|| path.clone(), |n| n.to_string_lossy().to_string());
                                if ui.button(name).on_hover_text(&path).clicked() {
                                    self.run(Action::OpenRecent(path), &ctx);
                                }
                            }
                            if !self.settings.recent_files.is_empty() {
                                ui.separator();
                                if ui.button("Clear Recent").clicked() {
                                    self.settings.clear_recent();
                                }
                            }
                        });
                        if ui.add(Button::new("Save").shortcut_text("Ctrl+S")).clicked() { self.save(&ctx); }
                        if ui.add(Button::new("Save As…").shortcut_text("Ctrl+Shift+S")).clicked() { self.choose_file(FileAction::SaveAs { then: None }, &ctx); }
                        ui.separator();
                        if ui.button("Import Image…").clicked() { self.choose_file(FileAction::ImportImage, &ctx); }
                        ui.separator();
                        if ui.button("Export Text…").clicked() { self.choose_file(FileAction::ExportText, &ctx); }
                        if ui.button("Export ANSI…").clicked() { self.choose_file(FileAction::ExportAnsi, &ctx); }
                        if ui.button("Export HTML…").clicked() { self.choose_file(FileAction::ExportHtml, &ctx); }
                        if ui.button("Export PNG…").clicked() { self.choose_file(FileAction::ExportPng { transparent: false }, &ctx); }
                        if ui.button("Export PNG (Transparent)…").clicked() { self.choose_file(FileAction::ExportPng { transparent: true }, &ctx); }
                        let gif = Button::new("Export GIF…");
                        if ui.add_enabled(self.doc.frame_count() > 1, gif).on_disabled_hover_text("Needs more than one frame").clicked() {
                            self.choose_file(FileAction::ExportGif, &ctx);
                        }
                        ui.separator();
                        if ui.add(Button::new("Close Tab").shortcut_text("Ctrl+W")).clicked() { self.run(Action::CloseTab, &ctx); }
                        if ui.button("Quit").clicked() { self.run(Action::Quit, &ctx); }
                    });

                    ui.menu_button("Edit", |ui| {
                        let undo = Button::new("Undo").shortcut_text("Ctrl+Z");
                        if ui.add_enabled(self.history.can_undo(), undo).clicked() {
                            self.run(Action::Undo, &ctx);
                        }
                        let redo = Button::new("Redo").shortcut_text("Ctrl+Y");
                        if ui.add_enabled(self.history.can_redo(), redo).clicked() {
                            self.run(Action::Redo, &ctx);
                        }
                        ui.separator();
                        let has_selection = self.tools.selection.selection.is_some();
                        if ui.add_enabled(has_selection, Button::new("Copy").shortcut_text("Ctrl+C")).clicked() {
                            self.copy(&ctx);
                        }
                        if ui.add_enabled(has_selection, Button::new("Cut").shortcut_text("Ctrl+X")).clicked() {
                            self.cut(&ctx);
                        }
                        if ui.add_enabled(self.clipboard.is_some(), Button::new("Paste").shortcut_text("Ctrl+V")).clicked() {
                            self.paste(None);
                        }
                        if ui.button("Insert Banner…").clicked() { self.open_banner(); }
                        ui.menu_button("Copy As", |ui| {
                            let what = if has_selection { "selection" } else { "whole picture" };
                            if ui.button("Plain Text").on_hover_text(format!("Copies the {what}")).clicked() {
                                self.copy_as(&ctx, false);
                            }
                            if ui.button("ANSI Colours").on_hover_text(format!("Copies the {what} with terminal colour codes")).clicked() {
                                self.copy_as(&ctx, true);
                            }
                        });
                        if ui.add_enabled(has_selection, Button::new("Delete").shortcut_text("Del")).clicked() {
                            self.delete_selection();
                        }
                        ui.add_enabled_ui(has_selection, |ui| {
                            ui.menu_button("Transform", |ui| {
                                for (t, label, key) in TRANSFORMS {
                                    if ui.add(Button::new(*label).shortcut_text(*key)).clicked() {
                                        self.transform_selection(*t);
                                    }
                                }
                            });
                        });
                        ui.separator();
                        if ui.add(Button::new("Replace…").shortcut_text("Ctrl+H")).clicked() { self.open_replace(); }
                        if ui.button("Script…").on_hover_text("Draw with a small program (Rhai)").clicked() { self.run(Action::Script, &ctx); }
                        if ui.button("Library…").on_hover_text("Glyph sets, shade ramps, brushes and palettes: yours and the built-in ones").clicked() {
                            self.run(Action::Library, &ctx);
                        }
                        if ui.button("Clear Layer").clicked() { self.clear_layer(); }
                    });

                    ui.menu_button("Canvas", |ui| {
                        if ui.button("Resize…").clicked() { self.run(Action::Resize, &ctx); }
                    });

                    ui.menu_button("Animation", |ui| {
                        let frames = self.doc.frame_count();
                        if ui.button("New Frame (Copy)").on_hover_text("A copy of the frame shown, so still parts stay").clicked() {
                            self.run(Action::NewFrame, &ctx);
                        }
                        if ui.button("New Empty Frame").clicked() { self.run(Action::NewEmptyFrame, &ctx); }
                        if ui.add_enabled(frames > 1, Button::new("Delete Frame")).clicked() { self.run(Action::DeleteFrame, &ctx); }
                        ui.separator();
                        let play = if self.playback.playing { "Stop" } else { "Play" };
                        if ui.add_enabled(frames > 1, Button::new(play).shortcut_text("Enter")).clicked() { self.run(Action::Play, &ctx); }
                        if ui.add_enabled(frames > 1, Button::new("Previous Frame").shortcut_text(",")).clicked() { self.run(Action::PrevFrame, &ctx); }
                        if ui.add_enabled(frames > 1, Button::new("Next Frame").shortcut_text(".")).clicked() { self.run(Action::NextFrame, &ctx); }
                        ui.separator();
                        ui.checkbox(&mut self.canvas_state.onion_skin, "Onion Skin");
                    });

                    ui.menu_button("View", |ui| {
                        if ui.add(Button::new("Command Palette…").shortcut_text("Ctrl+Shift+P")).clicked() {
                            self.run(Action::CommandPalette, &ctx);
                        }
                        ui.separator();
                        if ui.add(Button::new("Zoom In").shortcut_text("Ctrl++")).clicked() { self.run(Action::ZoomIn, &ctx); }
                        if ui.add(Button::new("Zoom Out").shortcut_text("Ctrl+-")).clicked() { self.run(Action::ZoomOut, &ctx); }
                        if ui.add(Button::new("Fit to Window").shortcut_text("Ctrl+0")).clicked() { self.run(Action::Fit, &ctx); }
                        if ui.add(Button::new("Actual Size").shortcut_text("Ctrl+1")).clicked() { self.run(Action::ActualSize, &ctx); }
                        ui.separator();
                        ui.checkbox(&mut self.canvas_state.show_grid, "Show Grid");
                    });

                    ui.menu_button("Help", |ui| {
                        if ui.button("Keyboard Shortcuts").clicked() { self.run(Action::Shortcuts, &ctx); }
                        if ui.button("Author").clicked() { self.run(Action::Author, &ctx); }
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.doc.modified {
                            ui.label(RichText::new("●").color(theme::WARNING)).on_hover_text("Unsaved changes");
                        }
                        let size = format!("{}×{}", self.doc.width, self.doc.height);
                        ui.label(RichText::new(size).font(theme::mono(12.0)).color(theme::TEXT_FAINT));
                        ui.label(RichText::new(self.doc_name()).color(theme::TEXT_DIM));
                    });
                });
            });
    }

    /// Settings of the active tool.
    pub(super) fn options_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("options")
            .frame(Frame::NONE.fill(theme::PANEL).inner_margin(Margin::symmetric(12, 5)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let tool = self.active_tool;
                    ui.label(RichText::new(tool.icon()).font(theme::mono(18.0)).color(theme::ACCENT));
                    ui.label(RichText::new(tool.name()).font(theme::semibold(13.5)));
                    vdivider(ui);

                    let hint = |ui: &mut Ui, text: &str| {
                        ui.label(RichText::new(text).color(theme::TEXT_FAINT));
                    };
                    if tool == ToolType::Brush {
                        self.brush_options(ui);
                    }
                    let options = &mut self.tool_options;
                    match tool {
                        ToolType::Brush => {}
                        ToolType::Eraser | ToolType::Pixel | ToolType::Shade => size_field(ui, options),
                        ToolType::Rectangle | ToolType::Ellipse => {
                            ui.toggle_value(&mut options.filled, "Filled");
                        }
                        ToolType::Border | ToolType::Line => {
                            ui.label("Style");
                            if tool == ToolType::Line && ui.selectable_label(!options.line_box, "Glyph").clicked() {
                                options.line_box = false;
                            }
                            for (style, label) in [
                                (BorderStyle::Single, "┌─┐  Single"),
                                (BorderStyle::Double, "╔═╗  Double"),
                                (BorderStyle::Heavy,  "┏━┓  Heavy"),
                            ] {
                                let on = options.border_style == style && (tool == ToolType::Border || options.line_box);
                                if ui.selectable_label(on, label).clicked() {
                                    options.border_style = style;
                                    options.line_box = true;
                                }
                            }
                        }
                        ToolType::Fill => hint(ui, "Fills connected cells that look the same"),
                        ToolType::Gradient => {
                            ui.selectable_value(&mut options.gradient, GradientShape::Linear, "Linear");
                            ui.selectable_value(&mut options.gradient, GradientShape::Radial, "Radial");
                        }
                        ToolType::Eyedropper => hint(ui, "Click: pick into primary · right click: secondary · Alt+click works with any drawing tool"),
                        ToolType::Text => hint(ui, "Click and type · Enter: next line · Esc: done"),
                        ToolType::Select => {
                            if let Some((x1, y1, x2, y2)) = self.tools.selection.selection {
                                let info = format!("{}×{} at {x1}, {y1}", x2 - x1 + 1, y2 - y1 + 1);
                                ui.label(RichText::new(info).font(theme::mono(12.5)));
                                vdivider(ui);
                                for (t, label, key) in TRANSFORMS {
                                    let icon = match t {
                                        Transform::FlipHorizontal => icons::FLIP_H,
                                        Transform::FlipVertical   => icons::FLIP_V,
                                        Transform::RotateCcw      => icons::ROTATE_LEFT,
                                        Transform::RotateCw       => icons::ROTATE_RIGHT,
                                    };
                                    let tip = if key.is_empty() { label.to_string() } else { format!("{label}  ({key})") };
                                    if icon_button(ui, icon, 26.0, false, &tip).clicked() {
                                        self.transform_selection(*t);
                                    }
                                }
                                vdivider(ui);
                                hint(ui, "Drag inside or use arrows to move · Del clears");
                            } else {
                                hint(ui, "Drag to select · Ctrl+A selects all");
                            }
                        }
                    }
                    if tool == ToolType::Pixel {
                        vdivider(ui);
                        ui.label("Grid");
                        for grid in PixelGrid::ALL {
                            ui.selectable_value(&mut self.tool_options.pixel_grid, grid, grid.name());
                        }
                    }
                    if matches!(tool, ToolType::Shade | ToolType::Gradient) {
                        vdivider(ui);
                        ui.label("Ramp");
                        if let Some(ramp) = ramp_menu(ui, "tool_ramp", &self.tool_options.ramp_name, &self.settings.all_ramps()) {
                            match ramp {
                                Some(ramp) => {
                                    self.tool_options.ramp = ramp.glyphs;
                                    self.tool_options.ramp_name = ramp.name;
                                }
                                None => self.open_library(LibraryTab::Ramps),
                            }
                        }
                        if tool == ToolType::Gradient {
                            ui.toggle_value(&mut self.tool_options.dither, "Dither")
                                .on_hover_text("Mix neighbouring ramp glyphs for smoother steps");
                        }
                    }
                    if tool == ToolType::Shade {
                        vdivider(ui);
                        ui.label("Colour");
                        let channels = &mut self.tool_options.channels;
                        ui.toggle_value(&mut channels.fg, "Fore").on_hover_text("Also paint the foreground colour");
                        ui.toggle_value(&mut channels.bg, "Back").on_hover_text("Also paint the background colour");
                    }
                    if (tool.draws() || tool == ToolType::Eyedropper) && !matches!(tool, ToolType::Pixel | ToolType::Shade) {
                        if tool != ToolType::Line {
                            vdivider(ui);
                        }
                        let channels = &mut self.tool_options.channels;
                        let verb = match tool {
                            ToolType::Eraser => "Erase",
                            ToolType::Eyedropper => "Pick",
                            ToolType::Text => "Type",
                            _ => "Paint",
                        };
                        ui.label(verb);
                        for (on, label, what) in [
                            (&mut channels.glyph, "Glyph", "the character"),
                            (&mut channels.fg, "Fore", "the foreground colour"),
                            (&mut channels.bg, "Back", "the background colour"),
                        ] {
                            ui.toggle_value(on, label).on_hover_text(format!("{verb} {what}"));
                        }
                    }
                    if tool.is_shape() || matches!(tool, ToolType::Brush | ToolType::Pixel | ToolType::Shade | ToolType::Eraser | ToolType::Fill) {
                        vdivider(ui);
                        let mirror = &mut self.tool_options.mirror;
                        if icon_toggle(ui, icons::FLIP_H, 26.0, mirror.x, "Mirror left ↔ right").clicked() {
                            mirror.x = !mirror.x;
                        }
                        if icon_toggle(ui, icons::FLIP_V, 26.0, mirror.y, "Mirror top ↔ bottom").clicked() {
                            mirror.y = !mirror.y;
                        }
                    }
                    if tool.is_shape() {
                        vdivider(ui);
                        let joins = tool == ToolType::Border || (tool == ToolType::Line && self.tool_options.line_box);
                        hint(ui, if joins {
                            "Joins existing lines · Shift: square / straight · Alt: from centre"
                        } else {
                            "Shift: square / straight · Alt: from centre"
                        });
                    } else if tool == ToolType::Shade {
                        vdivider(ui);
                        hint(ui, "Each stroke one step denser · right button: lighter");
                    } else if tool == ToolType::Gradient {
                        vdivider(ui);
                        hint(ui, "Drag across the selection or the layer · right button: reversed");
                    } else if tool == ToolType::Pixel {
                        vdivider(ui);
                        hint(ui, if self.tool_options.pixel_grid == PixelGrid::Halves {
                            "Two colours per cell · right button: secondary colour · Alt+click: pick"
                        } else {
                            "One colour per cell · right button: clear a pixel · Alt+click: pick"
                        });
                    } else if tool == ToolType::Brush && matches!(self.tool_options.brush, BrushKind::Direction { .. }) {
                        vdivider(ui);
                        hint(ui, "Each cell gets the glyph for the way the stroke goes · right button: secondary slot");
                    } else if matches!(tool, ToolType::Brush | ToolType::Eraser | ToolType::Fill) {
                        vdivider(ui);
                        hint(ui, "Right button: secondary slot · Alt+click: pick");
                    }

                    // An explicit id keeps the ids of these widgets the same whatever the
                    // tool-specific controls to the left are (otherwise hover state is lost).
                    ui.scope_builder(UiBuilder::new().id(Id::new("options_right")).layout(Layout::right_to_left(Align::Center)), |ui| {
                        let grid = self.canvas_state.show_grid;
                        let icon = if grid { icons::GRID } else { icons::GRID_OFF };
                        if icon_toggle(ui, icon, 26.0, grid, "Show grid").clicked() {
                            self.canvas_state.show_grid = !grid;
                        }
                    });
                });
            });
    }

    /// The Brush's presets and the settings of the kind of brush it is.
    fn brush_options(&mut self, ui: &mut Ui) {
        let mut chosen: Option<BrushPreset> = None;
        let mut delete = None;
        let mut from_selection = false;
        let mut manage = false;
        ComboBox::from_id_salt("brush_preset")
            .selected_text(&self.tool_options.brush_name)
            .width(118.0)
            .show_ui(ui, |ui| {
                let current = &self.tool_options.brush_name;
                for preset in builtin_presets() {
                    if ui.selectable_label(*current == preset.name, &preset.name).clicked() {
                        chosen = Some(preset);
                    }
                }
                if !self.settings.brushes.is_empty() {
                    ui.separator();
                }
                for (i, preset) in self.settings.brushes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(*current == preset.name, &preset.name).clicked() {
                            chosen = Some(preset.clone());
                        }
                        if ui.small_button(icons::CLOSE).on_hover_text("Delete this brush").clicked() {
                            delete = Some(i);
                        }
                    });
                }
                ui.separator();
                from_selection = ui.add(Button::new("Use Selection as Brush").shortcut_text("Ctrl+B")).clicked();
                manage = ui.button("Manage…").clicked();
            });
        if manage {
            self.open_library(LibraryTab::Brushes);
        }
        if let Some(preset) = chosen {
            self.tool_options.brush = preset.kind;
            self.tool_options.brush_name = preset.name;
        }
        if let Some(i) = delete {
            self.settings.edit_brushes(|brushes| { brushes.remove(i); });
        }
        if from_selection {
            self.brush_from_selection();
        }

        let options = &mut self.tool_options;
        match &mut options.brush {
            BrushKind::Square => size_field(ui, options),
            BrushKind::Scatter { glyphs, density } => {
                ui.add(TextEdit::singleline(glyphs).desired_width(70.0).font(TextStyle::Monospace))
                    .on_hover_text("Glyphs to scatter, picked at random");
                let mut percent = *density * 100.0;
                ui.spacing_mut().slider_width = 70.0;
                if ui.add(Slider::new(&mut percent, 1.0..=100.0).suffix("%").fixed_decimals(0)).on_hover_text("How many cells get a glyph").changed() {
                    *density = percent / 100.0;
                }
                size_field(ui, options);
            }
            BrushKind::Direction { glyphs } => {
                ui.add(TextEdit::singleline(glyphs).char_limit(4).desired_width(48.0).font(TextStyle::Monospace))
                    .on_hover_text("Glyphs for lines going across, down, up-right and down-right");
            }
            BrushKind::Script { code } => {
                if ui.button("Edit…").on_hover_text("Open the brush's script in the Script window").clicked() {
                    let code = code.clone();
                    self.open_script();
                    if let Some(window) = &mut self.script {
                        window.code = code;
                    }
                }
            }
            BrushKind::Stamp { cells, recolor } => {
                let size = format!("{}×{}", cells.first().map_or(0, Vec::len), cells.len());
                ui.label(RichText::new(size).font(theme::mono(12.5)).color(theme::TEXT_DIM));
                ui.toggle_value(recolor, "Slot Colours").on_hover_text("Paint the stamp in the slot's colours instead of its own");
            }
        }

        ui.menu_button(format!("{}  Save", icons::PLUS), |ui| {
            if self.brush_save_name.is_empty() {
                self.brush_save_name = self.tool_options.brush_name.clone();
            }
            ui.label("Keep this brush as");
            let field = ui.add(TextEdit::singleline(&mut self.brush_save_name).desired_width(160.0));
            let save = ui.button("Save Brush").clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
            let name = self.brush_save_name.trim().to_owned();
            if save && !name.is_empty() {
                let preset = BrushPreset { name: name.clone(), kind: self.tool_options.brush.clone() };
                self.settings.edit_brushes(|brushes| match brushes.iter_mut().find(|b| b.name == name) {
                    Some(same) => *same = preset,
                    None => brushes.push(preset),
                });
                self.tool_options.brush_name = name;
                self.brush_save_name.clear();
                self.show_status("Brush saved");
                ui.close();
            }
        });
        vdivider(ui);
    }

    pub(super) fn tool_strip(&mut self, ui: &mut Ui) {
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(52.0)
            .frame(Frame::NONE.fill(theme::PANEL).inner_margin(Margin::symmetric(8, 10)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (i, group) in ToolType::groups().iter().enumerate() {
                    if i > 0 {
                        ui.add_space(2.0);
                        widgets::hline(ui);
                        ui.add_space(2.0);
                    }
                    for &tool in *group {
                        let tip = format!("{}  ({})", tool.name(), tool.key());
                        if icon_button(ui, tool.icon(), 36.0, self.active_tool == tool, &tip).clicked() {
                            self.active_tool = tool;
                        }
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    if icon_button(ui, icons::SWAP, 24.0, false, "Swap primary and secondary  (X)").clicked() {
                        self.swap_slots();
                    }
                    if self.slot_swatch(ui).clicked() {
                        self.dock_tab = DockTab::Colors;
                    }
                });
            });
    }

    /// The primary glyph and colours on top of the secondary ones.
    fn slot_swatch(&self, ui: &mut Ui) -> Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 40.0), Sense::click());
        let tile = vec2(25.0, 28.0);
        let slots = [
            (Rect::from_min_size(rect.max - tile, tile), self.secondary_char, &self.color_panel.secondary_fg, &self.color_panel.secondary_bg),
            (Rect::from_min_size(rect.min, tile), self.primary_char, &self.color_panel.primary_fg, &self.color_panel.primary_bg),
        ];
        let painter = ui.painter();
        for (i, (r, ch, fg, bg)) in slots.into_iter().enumerate() {
            painter.rect_filled(r, 4.0, theme::CANVAS_BG);
            painter.rect_filled(r, 4.0, bg.to_egui());
            painter.text(r.center(), Align2::CENTER_CENTER, ch, theme::mono(18.0), fg.to_egui());
            let front = i == 1;
            let ring = if front && resp.hovered() { theme::ACCENT } else { theme::BORDER_STRONG };
            painter.rect_stroke(r, 4.0, Stroke::new(1.0, ring), StrokeKind::Outside);
        }
        resp.on_hover_text(format!(
            "Primary '{}' · secondary '{}'\nClick to edit the colours",
            self.primary_char, self.secondary_char
        ))
    }

    /// Glyphs and colours on top, layers below.
    pub(super) fn dock(&mut self, ui: &mut Ui) {
        egui::Panel::right("dock")
            .resizable(true)
            .default_size(300.0)
            .size_range(250.0..=480.0)
            .frame(Frame::NONE.fill(theme::PANEL))
            .show(ui, |ui| {
                egui::Panel::bottom("layers_dock")
                    .resizable(true)
                    .default_size(250.0)
                    .size_range(140.0..=640.0)
                    .frame(Frame::NONE.inner_margin(Margin { left: 12, right: 12, top: 10, bottom: 4 }))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icons::LAYERS).font(theme::mono(14.0)).color(theme::TEXT_DIM));
                            ui.label(RichText::new("Layers").font(theme::semibold(13.0)));
                        });
                        if let Some(action) = self.layer_panel.show(ui, &self.doc) {
                            self.apply_layer_action(action);
                        }
                    });

                Frame::NONE.inner_margin(Margin { left: 12, right: 12, top: 2, bottom: 8 }).show(ui, |ui| {
                    const TABS: [(DockTab, &str, &str); 3] = [
                        (DockTab::Glyphs, icons::GLYPHS, "Glyphs"),
                        (DockTab::Colors, icons::PALETTE, "Colors"),
                        (DockTab::History, icons::HISTORY, "History"),
                    ];
                    let labels = TABS.map(|(_, icon, label)| (icon, label));
                    let current = TABS.iter().position(|t| t.0 == self.dock_tab).unwrap_or(0);
                    if let Some(i) = tab_bar(ui, &labels, current) {
                        self.dock_tab = TABS[i].0;
                    }
                    widgets::hline(ui);
                    ui.add_space(6.0);
                    let mut set_action = None;
                    let mut manage_palettes = false;
                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.dock_tab {
                        DockTab::Glyphs => {
                            let sets = &self.settings.glyph_sets;
                            set_action = self.char_palette.show(ui, &mut self.primary_char, &mut self.secondary_char, sets);
                        }
                        DockTab::Colors => {
                            if self.color_panel.show(ui, &self.settings.all_palettes()) {
                                manage_palettes = true;
                            }
                        }
                        DockTab::History => self.history_panel(ui),
                    });
                    if let Some(action) = set_action {
                        self.apply_set_action(action, ui.ctx());
                    }
                    if manage_palettes {
                        self.open_library(LibraryTab::Palettes);
                    }
                });
            });
    }

    /// Every recorded step; clicking one undoes or redoes up to it.
    fn history_panel(&mut self, ui: &mut Ui) {
        let (done, undone) = self.history.labels();
        let applied = done.len();
        let mut jump = None;
        let mut row = |ui: &mut Ui, label: &str, position: usize| {
            let current = position == applied;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
            let bg = if current {
                theme::ACCENT_SOFT
            } else if resp.hovered() {
                theme::SURFACE
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, 5.0, bg);
            if current {
                let bar = Rect::from_min_size(rect.min + vec2(0.0, 5.0), vec2(3.0, rect.height() - 10.0));
                ui.painter().rect_filled(bar, 1.5, theme::ACCENT);
            }
            let color = if position > applied {
                theme::TEXT_FAINT
            } else if current {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            };
            let text_pos = rect.left_center() + vec2(12.0, 0.0);
            ui.painter().text(text_pos, Align2::LEFT_CENTER, label, FontId::proportional(13.0), color);
            if resp.clicked() {
                jump = Some(position);
            }
        };

        row(ui, "Start", 0);
        for (i, label) in done.iter().enumerate() {
            row(ui, label, i + 1);
        }
        for (i, label) in undone.iter().enumerate() {
            row(ui, label, applied + 1 + i);
        }
        if let Some(position) = jump {
            self.history.jump_to(position, &mut self.doc);
        }
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status")
            .frame(Frame::NONE.fill(theme::BG_DEEP).inner_margin(Margin::symmetric(10, 3)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let dim = |text: String| RichText::new(text).size(12.0).color(theme::TEXT_DIM);
                    let mono = |text: String| RichText::new(text).font(theme::mono(12.0)).color(theme::TEXT_DIM);

                    let (w, h) = (self.doc.width, self.doc.height);
                    match self.canvas_state.cursor_cell.filter(|&(x, y)| x < w && y < h) {
                        Some((x, y)) => {
                            ui.label(mono(format!("{x:>3}, {y:<3}")));
                            let cell = self.doc.composite_cell(x, y);
                            ui.label(RichText::new(cell.ch.to_string()).font(theme::mono(13.0)).color(theme::TEXT));
                            ui.label(mono(format!("U+{:04X}", cell.ch as u32)));
                            for (name, color) in [("glyph", cell.fg.to_egui()), ("back", cell.bg.to_egui())] {
                                let shown = if name == "glyph" && cell.ch == ' ' { Color32::TRANSPARENT } else { color };
                                color_chip(ui, shown, vec2(11.0, 11.0), false)
                                    .on_hover_text(format!("{name}: {}", to_hex(shown)));
                            }
                        }
                        None => {
                            ui.label(mono("  –, –  ".into()));
                        }
                    }
                    if let Some((x1, y1, x2, y2)) = self.tools.selection.selection {
                        vdivider(ui);
                        ui.label(dim(format!("Selection {}×{}", x2 - x1 + 1, y2 - y1 + 1)));
                    }
                    vdivider(ui);
                    let layer = self.doc.active_layer();
                    let mut text = format!("Layer: {}", layer.name);
                    if layer.locked { text.push_str("  (locked)"); }
                    if !layer.visible { text.push_str("  (hidden)"); }
                    ui.label(dim(text));

                    // Stable ids: the left part grows and shrinks as the pointer moves.
                    ui.scope_builder(UiBuilder::new().id(Id::new("status_right")).layout(Layout::right_to_left(Align::Center)), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if icon_button(ui, icons::ZOOM_IN, 20.0, false, "Zoom in  (Ctrl +)").clicked() {
                            self.canvas_state.zoom_view_center(1.25);
                        }
                        let zoom = format!("{:.0}%", self.canvas_state.zoom * 100.0);
                        if ui.add(Label::new(mono(zoom)).sense(Sense::click())).on_hover_text("Fit to window  (Ctrl+0)").clicked() {
                            self.canvas_state.request_fit();
                        }
                        if icon_button(ui, icons::ZOOM_OUT, 20.0, false, "Zoom out  (Ctrl -)").clicked() {
                            self.canvas_state.zoom_view_center(1.0 / 1.25);
                        }
                        if let Some(job) = &self.render_job {
                            vdivider(ui);
                            let progress = job.done as f32 / job.total.max(1) as f32;
                            ui.add(ProgressBar::new(progress).desired_width(110.0).desired_height(10.0).fill(theme::ACCENT_STRONG));
                            ui.label(RichText::new(format!("Rendering 3D  {} / {}", job.done, job.total)).size(12.0).color(theme::TEXT_DIM));
                        }
                        if self.status_timer > 0.0 {
                            vdivider(ui);
                            let warning = self.status_msg.starts_with(icons::ALERT);
                            let color = if warning { theme::WARNING } else { theme::TEXT };
                            ui.label(RichText::new(&self.status_msg).size(12.0).color(color));
                        }
                    });
                });
            });
    }
}

/// Brush size: − / value / +.
fn size_field(ui: &mut Ui, options: &mut ToolOptions) {
    ui.label("Size");
    if icon_button(ui, icons::MINUS, 24.0, false, "Smaller  (-)").clicked() {
        options.brush_size = options.brush_size.saturating_sub(1).max(1);
    }
    ui.add(DragValue::new(&mut options.brush_size).range(1..=20).speed(0.1));
    if icon_button(ui, icons::PLUS, 24.0, false, "Bigger  (+)").clicked() {
        options.brush_size = (options.brush_size + 1).min(20);
    }
}

/// A drop-down of the density ramps, each drawn as it looks, with "Manage…" at the end.
/// `Some(Some(ramp))`: a ramp was picked; `Some(None)`: manage the ramps.
pub(super) fn ramp_menu(ui: &mut Ui, id: &str, current: &str, ramps: &[Ramp]) -> Option<Option<Ramp>> {
    let mut picked = None;
    ComboBox::from_id_salt(id).selected_text(current).show_ui(ui, |ui| {
        for ramp in ramps {
            let sample: String = ramp.glyphs.trim_start().chars().take(12).collect();
            if ui.selectable_label(ramp.name == current, format!("{}   {sample}", ramp.name)).clicked() {
                picked = Some(Some(ramp.clone()));
            }
        }
        ui.separator();
        if ui.selectable_label(false, "Manage…").clicked() {
            picked = Some(None);
        }
    });
    picked
}
