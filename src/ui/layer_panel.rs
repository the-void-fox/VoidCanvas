use egui::*;
use crate::icons;
use crate::theme;
use crate::ui::widgets::icon_button;
use crate::document::Document;

#[derive(Debug)]
pub enum LayerAction {
    Select(usize),
    Add,
    Remove,
    Duplicate,
    MergeDown,
    MoveUp,
    MoveDown,
    ToggleVisible(usize),
    ToggleLock(usize),
    Rename(usize, String),
    Reorder(usize, usize),
}

#[derive(Default)]
pub struct LayerPanel {
    pub rename_idx:  Option<usize>,
    pub rename_buf:  String,
    rename_focus:    bool,
    drag_from:       Option<usize>,
    drag_target:     Option<usize>,
}

impl LayerPanel {
    pub fn show(&mut self, ui: &mut Ui, doc: &Document) -> Option<LayerAction> {
        let mut action = None;

        let (n, active) = (doc.layers.len(), doc.active_layer);
        egui::Panel::bottom("layer_tools")
            .frame(Frame::NONE.inner_margin(Margin::symmetric(0, 6)))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let mut button = |ui: &mut Ui, enabled: bool, icon: &str, hint: &str, act: LayerAction| {
                        let clicked = ui.add_enabled_ui(enabled, |ui| icon_button(ui, icon, 28.0, false, hint)).inner.clicked();
                        if clicked {
                            action = Some(act);
                        }
                    };
                    button(ui, true,           icons::PLUS,       "New layer",       LayerAction::Add);
                    button(ui, true,           icons::DUPLICATE,  "Duplicate layer", LayerAction::Duplicate);
                    button(ui, active > 0,     icons::MERGE_DOWN, "Merge down",      LayerAction::MergeDown);
                    button(ui, active + 1 < n, icons::ARROW_UP,   "Move layer up",   LayerAction::MoveUp);
                    button(ui, active > 0,     icons::ARROW_DOWN, "Move layer down", LayerAction::MoveDown);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        button(ui, n > 1, icons::DELETE, "Delete layer", LayerAction::Remove);
                    });
                });
            });

        self.drag_target = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for display_idx in 0..n {
                    let layer_idx = n - 1 - display_idx;
                    let layer     = &doc.layers[layer_idx];
                    let is_active = layer_idx == doc.active_layer;

                    if self.drag_from.is_some() {
                        let (_id, insert_rect) = ui.allocate_space(Vec2::new(ui.available_width(), 4.0));
                        if let Some(ptr) = ui.ctx().input(|i| i.pointer.hover_pos()) {
                            let extended = Rect::from_min_size(
                                insert_rect.min - Vec2::new(0.0, 6.0),
                                Vec2::new(insert_rect.width(), 12.0),
                            );
                            if extended.contains(ptr) {
                                self.drag_target = Some(layer_idx + 1);
                                ui.painter().rect_filled(insert_rect, 2.0, theme::ACCENT);
                            } else {
                                ui.painter().rect_filled(insert_rect, 2.0, Color32::TRANSPARENT);
                            }
                        }
                    }

                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 32.0),
                        Sense::click_and_drag(),
                    );

                    let bg = if is_active {
                        theme::ACCENT_SOFT
                    } else if row_resp.hovered() || self.drag_from == Some(layer_idx) {
                        theme::SURFACE
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(row_rect, 6.0, bg);
                    if is_active {
                        let bar = Rect::from_min_size(row_rect.min + vec2(0.0, 6.0), vec2(3.0, row_rect.height() - 12.0));
                        ui.painter().rect_filled(bar, 1.5, theme::ACCENT);
                    }

                    let mut child_ui = ui.new_child(
                        UiBuilder::new().max_rect(row_rect).layout(Layout::left_to_right(Align::Center)),
                    );
                    child_ui.add_space(6.0);
                    child_ui.spacing_mut().item_spacing.x = 2.0;

                    let (eye, eye_hint) = if layer.visible { (icons::EYE, "Hide layer") } else { (icons::EYE_OFF, "Show layer") };
                    if icon_button(&mut child_ui, eye, 24.0, false, eye_hint).clicked() {
                        action = Some(LayerAction::ToggleVisible(layer_idx));
                    }
                    let (lock, lock_hint) = if layer.locked { (icons::LOCK, "Unlock layer") } else { (icons::LOCK_OPEN, "Lock layer") };
                    if icon_button(&mut child_ui, lock, 24.0, false, lock_hint).clicked() {
                        action = Some(LayerAction::ToggleLock(layer_idx));
                    }

                    child_ui.add_space(6.0);

                    if self.rename_idx == Some(layer_idx) {
                        let r = child_ui.text_edit_singleline(&mut self.rename_buf);
                        if self.rename_focus {
                            r.request_focus();
                            self.rename_focus = false;
                        }
                        if child_ui.input(|i| i.key_pressed(Key::Escape)) {
                            self.rename_idx = None;
                        } else if r.lost_focus() || child_ui.input(|i| i.key_pressed(Key::Enter)) {
                            action = Some(LayerAction::Rename(layer_idx, self.rename_buf.clone()));
                            self.rename_idx = None;
                        }
                    } else {
                        let name_color = if !layer.visible {
                            theme::TEXT_FAINT
                        } else if is_active {
                            theme::TEXT
                        } else {
                            theme::TEXT_DIM
                        };
                        child_ui.add(Label::new(RichText::new(&layer.name).color(name_color)).selectable(false).truncate());
                    }

                    if row_resp.hovered() {
                        child_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(6.0);
                            ui.label(RichText::new(icons::DRAG).font(theme::mono(14.0)).color(theme::TEXT_FAINT));
                        });
                    }

                    if row_resp.drag_started() {
                        self.drag_from = Some(layer_idx);
                    }

                    if row_resp.clicked() && !is_active {
                        action = Some(LayerAction::Select(layer_idx));
                    }

                    if row_resp.double_clicked() {
                        self.rename_idx = Some(layer_idx);
                        self.rename_buf = layer.name.clone();
                        self.rename_focus = true;
                    }

                    ui.add_space(2.0);
                }

                if self.drag_from.is_some() {
                    let (_id, bottom_rect) = ui.allocate_space(Vec2::new(ui.available_width(), 4.0));
                    if let Some(ptr) = ui.ctx().input(|i| i.pointer.hover_pos()) {
                        let extended = Rect::from_min_size(
                            bottom_rect.min - Vec2::new(0.0, 6.0),
                            Vec2::new(bottom_rect.width(), 12.0),
                        );
                        if extended.contains(ptr) {
                            self.drag_target = Some(0);
                            ui.painter().rect_filled(bottom_rect, 2.0, theme::ACCENT);
                        } else {
                            ui.painter().rect_filled(bottom_rect, 2.0, Color32::TRANSPARENT);
                        }
                    }
                }
            });

        let mouse_released = ui.ctx().input(|i| i.pointer.any_released());
        if mouse_released {
            if let (Some(from), Some(to)) = (self.drag_from.take(), self.drag_target.take()) {
                if from != to {
                    if from > to {
                        action = Some(LayerAction::Reorder(from, to));
                    } else {
                        action = Some(LayerAction::Reorder(from, to - 1));
                    }
                }
            } else {
                self.drag_from   = None;
                self.drag_target = None;
            }
        }

        action
    }
}
