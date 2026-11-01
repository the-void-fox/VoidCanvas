//! The window chrome around the canvas: menu, tool options, tool strip, dock and status bar.

use eframe::egui::{self, *};

use super::{AsciiArtEditor, DockTab, PendingAction};
use crate::glyphs::Transform;
use crate::icons;
use crate::theme;
use crate::tools::{ToolType, border::BorderStyle, shade::RAMPS};
use crate::ui::widgets::{self, color_chip, icon_button, icon_toggle, tab_bar, to_hex, vdivider};

/// Selection transforms with their menu label and shortcut.
const TRANSFORMS: &[(Transform, &str, &str)] = &[
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
                        if ui.add(Button::new("New…").shortcut_text("Ctrl+N")).clicked() {
                            self.request(PendingAction::New, &ctx);
                        }
                        if ui.add(Button::new("Open…").shortcut_text("Ctrl+O")).clicked() {
                            self.request(PendingAction::Open, &ctx);
                        }
                        ui.menu_button("Open Recent", |ui| {
                            if self.settings.recent_files.is_empty() {
                                ui.label(RichText::new("No recent files").color(theme::TEXT_FAINT));
                            }
                            for path in self.settings.recent_files.clone() {
                                let name = std::path::Path::new(&path)
                                    .file_name()
                                    .map_or_else(|| path.clone(), |n| n.to_string_lossy().to_string());
                                if ui.button(name).on_hover_text(&path).clicked() {
                                    self.request(PendingAction::OpenPath(path.into()), &ctx);
                                }
                            }
                            if !self.settings.recent_files.is_empty() {
                                ui.separator();
                                if ui.button("Clear Recent").clicked() {
                                    self.settings.clear_recent();
                                }
                            }
                        });
                        if ui.add(Button::new("Save").shortcut_text("Ctrl+S")).clicked() { self.save(); }
                        if ui.add(Button::new("Save As…").shortcut_text("Ctrl+Shift+S")).clicked() { self.save_as(); }
                        ui.separator();
                        if ui.button("Import Image…").clicked() { self.choose_image_to_import(); }
                        ui.separator();
                        if ui.button("Export Text…").clicked() { self.export_text(); }
                        if ui.button("Export ANSI…").clicked() { self.export_ansi(); }
                        if ui.button("Export HTML…").clicked() { self.export_html(); }
                        if ui.button("Export PNG…").clicked() { self.export_png(false); }
                        if ui.button("Export PNG (Transparent)…").clicked() { self.export_png(true); }
                        ui.separator();
                        if ui.button("Quit").clicked() { self.request(PendingAction::Quit, &ctx); }
                    });

                    ui.menu_button("Edit", |ui| {
                        let undo = Button::new("Undo").shortcut_text("Ctrl+Z");
                        if ui.add_enabled(self.history.can_undo(), undo).clicked() {
                            self.history.undo(&mut self.doc);
                        }
                        let redo = Button::new("Redo").shortcut_text("Ctrl+Y");
                        if ui.add_enabled(self.history.can_redo(), redo).clicked() {
                            self.history.redo(&mut self.doc);
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
                        if ui.button("Clear Layer").clicked() { self.clear_layer(); }
                    });

                    ui.menu_button("Canvas", |ui| {
                        if ui.button("Resize…").clicked() {
                            self.new_width  = self.doc.width;
                            self.new_height = self.doc.height;
                            self.show_resize_dialog = true;
                        }
                    });

                    ui.menu_button("View", |ui| {
                        if ui.add(Button::new("Zoom In").shortcut_text("Ctrl++")).clicked() {
                            self.canvas_state.zoom_view_center(1.25);
                        }
                        if ui.add(Button::new("Zoom Out").shortcut_text("Ctrl+-")).clicked() {
                            self.canvas_state.zoom_view_center(1.0 / 1.25);
                        }
                        if ui.add(Button::new("Fit to Window").shortcut_text("Ctrl+0")).clicked() {
                            self.canvas_state.request_fit();
                        }
                        if ui.add(Button::new("Actual Size").shortcut_text("Ctrl+1")).clicked() {
                            self.canvas_state.actual_size();
                        }
                        ui.separator();
                        ui.checkbox(&mut self.canvas_state.show_grid, "Show Grid");
                    });

                    ui.menu_button("Help", |ui| {
                        if ui.button("Keyboard Shortcuts").clicked() {
                            self.about_tab = 0;
                            self.show_about_dialog = true;
                        }
                        if ui.button("Author").clicked() {
                            self.about_tab = 1;
                            self.show_about_dialog = true;
                        }
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
                    let options = &mut self.tool_options;
                    match tool {
                        ToolType::Brush | ToolType::Eraser | ToolType::Pixel | ToolType::Shade => {
                            ui.label("Size");
                            if icon_button(ui, icons::MINUS, 24.0, false, "Smaller  (-)").clicked() {
                                options.brush_size = options.brush_size.saturating_sub(1).max(1);
                            }
                            ui.add(DragValue::new(&mut options.brush_size).range(1..=20).speed(0.1));
                            if icon_button(ui, icons::PLUS, 24.0, false, "Bigger  (+)").clicked() {
                                options.brush_size = (options.brush_size + 1).min(20);
                            }
                        }
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
                    if tool == ToolType::Shade {
                        vdivider(ui);
                        ui.label("Ramp");
                        for (i, (name, ramp)) in RAMPS.iter().enumerate() {
                            let label = format!("{name}  {}", ramp.trim_start());
                            ui.selectable_value(&mut self.tool_options.shade_ramp, i, label);
                        }
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
                    } else if tool == ToolType::Pixel {
                        vdivider(ui);
                        hint(ui, "Square pixels from half blocks · right button: secondary colour · Alt+click: pick");
                    } else if matches!(tool, ToolType::Brush | ToolType::Eraser | ToolType::Fill) {
                        vdivider(ui);
                        hint(ui, "Right button: secondary slot · Alt+click: pick");
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let grid = self.canvas_state.show_grid;
                        let icon = if grid { icons::GRID } else { icons::GRID_OFF };
                        if icon_toggle(ui, icon, 26.0, grid, "Show grid").clicked() {
                            self.canvas_state.show_grid = !grid;
                        }
                    });
                });
            });
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
                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.dock_tab {
                        DockTab::Glyphs => self.char_palette.show(ui, &mut self.primary_char, &mut self.secondary_char),
                        DockTab::Colors => self.color_panel.show(ui),
                        DockTab::History => self.history_panel(ui),
                    });
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

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        if self.status_timer > 0.0 {
                            vdivider(ui);
                            ui.label(RichText::new(&self.status_msg).size(12.0).color(theme::TEXT));
                        }
                    });
                });
            });
    }
}
