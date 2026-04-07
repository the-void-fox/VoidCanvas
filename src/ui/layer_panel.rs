use egui::*;
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

pub struct LayerPanel {
    pub rename_idx:  Option<usize>,
    pub rename_buf:  String,
    drag_from:       Option<usize>,
    drag_target:     Option<usize>,
}

impl Default for LayerPanel {
    fn default() -> Self {
        Self {
            rename_idx:  None,
            rename_buf:  String::new(),
            drag_from:   None,
            drag_target: None,
        }
    }
}

impl LayerPanel {
    pub fn show(&mut self, ui: &mut Ui, doc: &Document) -> Option<LayerAction> {
        let mut action = None;

        ui.horizontal_wrapped(|ui| {
            if ui.button("➕").on_hover_text("Add layer").clicked()      { action = Some(LayerAction::Add);       }
            if ui.button("➖").on_hover_text("Remove layer").clicked()   { action = Some(LayerAction::Remove);    }
            if ui.button("\u{f0191}").on_hover_text("Duplicate").clicked()      { action = Some(LayerAction::Duplicate); }
            if ui.button("⬇M").on_hover_text("Merge down").clicked()    { action = Some(LayerAction::MergeDown); }
            if ui.button("↑").on_hover_text("Move layer up").clicked()   { action = Some(LayerAction::MoveUp);   }
            if ui.button("↓").on_hover_text("Move layer down").clicked() { action = Some(LayerAction::MoveDown); }
        });

        ui.separator();

        let n = doc.layers.len();
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
                                ui.painter().rect_filled(insert_rect, 2.0, Color32::from_rgb(0, 150, 255));
                            } else {
                                ui.painter().rect_filled(insert_rect, 2.0, Color32::TRANSPARENT);
                            }
                        }
                    }

                    let bg = if is_active {
                        Color32::from_rgb(0, 90, 180)
                    } else if self.drag_from == Some(layer_idx) {
                        Color32::from_gray(35)
                    } else {
                        Color32::from_gray(52)
                    };

                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 30.0),
                        Sense::click_and_drag(),
                    );

                    ui.painter().rect_filled(row_rect, 4.0, bg);

                    let mut child_ui = ui.child_ui(row_rect, Layout::left_to_right(Align::Center));
                    child_ui.add_space(6.0);

                    let vis_text = if layer.visible { "󰈈" } else { "󰈉" };
                    if child_ui.small_button(vis_text)
                        .on_hover_text(if layer.visible { "Hide layer" } else { "Show layer" })
                        .clicked()
                    {
                        action = Some(LayerAction::ToggleVisible(layer_idx));
                    }

                    let lck_text = if layer.locked { "🔒" } else { "🔓" };
                    if child_ui.small_button(lck_text)
                        .on_hover_text(if layer.locked { "Unlock layer" } else { "Lock layer" })
                        .clicked()
                    {
                        action = Some(LayerAction::ToggleLock(layer_idx));
                    }

                    child_ui.add_space(4.0);

                    if self.rename_idx == Some(layer_idx) {
                        let r = child_ui.text_edit_singleline(&mut self.rename_buf);
                        if r.lost_focus() || child_ui.input(|i| i.key_pressed(Key::Enter)) {
                            action = Some(LayerAction::Rename(layer_idx, self.rename_buf.clone()));
                            self.rename_idx = None;
                        }
                    } else {
                        let name_color = if is_active {
                            Color32::WHITE
                        } else if !layer.visible {
                            Color32::GRAY
                        } else {
                            Color32::from_gray(210)
                        };
                        let text = RichText::new(&layer.name)
                            .color(name_color)
                            .strong_if(is_active);
                        child_ui.label(text);
                    }

                    child_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("⠿").color(Color32::from_gray(100)));
                    });

                    if row_resp.drag_started() {
                        self.drag_from = Some(layer_idx);
                    }

                    if row_resp.clicked() && !is_active {
                        action = Some(LayerAction::Select(layer_idx));
                    }

                    if row_resp.double_clicked() {
                        self.rename_idx = Some(layer_idx);
                        self.rename_buf = layer.name.clone();
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
                            ui.painter().rect_filled(bottom_rect, 2.0, Color32::from_rgb(0, 150, 255));
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

trait RichTextExt {
    fn strong_if(self, condition: bool) -> Self;
}

impl RichTextExt for RichText {
    fn strong_if(self, condition: bool) -> Self {
        if condition { self.strong() } else { self }
    }
}