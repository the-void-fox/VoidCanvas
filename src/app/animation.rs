//! Animation: frames, playback and the timeline under the canvas.

use eframe::egui::{self, *};

use super::AsciiArtEditor;
use crate::document::Document;
use crate::history::Command;
use crate::ui::layer_panel::{LayerAction, layer_menu};
use crate::ui::widgets::{icon_button, icon_toggle};
use crate::{icons, theme};

#[derive(Default)]
pub struct Playback {
    pub playing: bool,
    /// Milliseconds the shown frame has been on screen.
    elapsed: f64,
    /// Frame durations before a duration field started changing (one undo step per edit).
    timing_before: Option<Vec<u32>>,
    /// Frames picked together in the timeline (Ctrl / Shift + click); empty: just the shown one.
    pub selected: std::collections::BTreeSet<usize>,
}

/// Width of the layer column and height of the frame-number row of the timeline.
const LAYER_COLUMN: f32 = 236.0;
const HEADER_H: f32 = 26.0;
/// One cel: a layer in a frame.
const CEL: Vec2 = vec2(28.0, 24.0);
/// The picture shown when hovering a frame number.
const PREVIEW: Vec2 = vec2(180.0, 110.0);
/// Next-frame tint of the onion skin, as on the canvas.
const ONION_NEXT: Color32 = Color32::from_rgb(120, 220, 230);

/// What the user did in the frame grid, carried out once drawing is done.
enum GridAction {
    Go { layer: Option<usize>, frame: usize },
    /// Ctrl+click adds or takes out one frame, Shift+click picks a range from the shown one.
    Pick { frame: usize, range: bool },
    Move { from: usize, to: usize },
    NewFrame { empty: bool },
    DeleteFrame,
    Duration { frame: usize, ms: u32 },
    DurationForAll(u32),
}

impl AsciiArtEditor {
    /// A new frame after the shown one: a copy of it (so still parts stay), or empty.
    pub(super) fn add_frame(&mut self, empty: bool) {
        let (data, label) = if empty {
            (self.doc.blank_frame(), "New Empty Frame")
        } else {
            (self.doc.frame_data(self.doc.frame), "New Frame")
        };
        self.stop_playback();
        self.history.execute_as(Command::FrameAdd { index: self.doc.frame + 1, data }, label, &mut self.doc);
    }

    /// The frames the next frame command works on: those picked in the timeline, or the shown one.
    pub(crate) fn frames_in_hand(&self) -> Vec<usize> {
        let picked: Vec<usize> = self.playback.selected.iter().copied().filter(|&f| f < self.doc.frame_count()).collect();
        if picked.len() > 1 { picked } else { vec![self.doc.frame] }
    }

    /// Deletes the frames in hand (one undo step); one frame always stays.
    pub(super) fn delete_frame(&mut self) {
        let frames = self.frames_in_hand();
        if frames.len() >= self.doc.frame_count() {
            self.show_status("An animation keeps at least one frame");
            return;
        }
        self.stop_playback();
        // From the last, so the indices of the rest stay right.
        let cmds: Vec<Command> = frames.iter().rev().map(|&index| Command::FrameRemove { index, data: self.doc.frame_data(index) }).collect();
        let label = if cmds.len() == 1 { "Delete Frame" } else { "Delete Frames" };
        self.history.execute_as(Command::Group(cmds), label, &mut self.doc);
        self.playback.selected.clear();
    }

    /// Copies of the frames in hand, right after the last of them (one undo step).
    pub(crate) fn duplicate_frames(&mut self) {
        let frames = self.frames_in_hand();
        let after = frames.last().copied().unwrap_or(self.doc.frame) + 1;
        let cmds: Vec<Command> = frames.iter().enumerate().map(|(k, &f)| Command::FrameAdd { index: after + k, data: self.doc.frame_data(f) }).collect();
        self.stop_playback();
        let label = if cmds.len() == 1 { "New Frame" } else { "Duplicate Frames" };
        self.history.execute_as(Command::Group(cmds), label, &mut self.doc);
        self.playback.selected.clear();
    }

    /// Moves the shown frame `step` places to the right (left if negative).
    pub(super) fn move_frame(&mut self, step: isize) {
        let to = self.doc.frame as isize + step;
        if to >= 0 {
            self.move_frame_to(self.doc.frame, to as usize);
        }
    }

    fn move_frame_to(&mut self, from: usize, to: usize) {
        if from == to || from >= self.doc.frame_count() || to >= self.doc.frame_count() { return; }
        self.stop_playback();
        self.history.execute(Command::FrameMove { from, to }, &mut self.doc);
    }

    pub(crate) fn go_to_frame(&mut self, frame: usize) {
        if self.drawing || frame == self.doc.frame { return; }
        self.tools.text.stop();
        self.doc.show_frame(frame);
        self.playback.elapsed = 0.0;
    }

    /// The next frame (`step` 1) or the previous one (-1), wrapping around.
    pub(super) fn step_frame(&mut self, step: isize) {
        let n = self.doc.frame_count() as isize;
        self.go_to_frame((self.doc.frame as isize + step).rem_euclid(n) as usize);
    }

    pub(super) fn toggle_playback(&mut self) {
        if self.playback.playing {
            self.stop_playback();
        } else if self.doc.frame_count() < 2 {
            self.show_status("Add a frame to play an animation");
        } else {
            self.tools.text.stop();
            self.playback.playing = true;
            self.playback.elapsed = 0.0;
        }
    }

    pub(super) fn stop_playback(&mut self) {
        self.playback.playing = false;
    }

    /// Shows the next frame whenever the current one has been on screen long enough.
    pub(super) fn advance_playback(&mut self, ctx: &egui::Context) {
        if !self.playback.playing { return; }
        if self.drawing || self.doc.frame_count() < 2 {
            self.stop_playback();
            return;
        }
        self.playback.elapsed += ctx.input(|i| i.unstable_dt) as f64 * 1000.0;
        loop {
            let shown_for = self.doc.durations[self.doc.frame].max(10) as f64;
            if self.playback.elapsed < shown_for {
                ctx.request_repaint_after_secs(((shown_for - self.playback.elapsed) / 1000.0) as f32);
                break;
            }
            self.playback.elapsed -= shown_for;
            self.doc.show_frame((self.doc.frame + 1) % self.doc.frame_count());
        }
    }

    /// Records a change of the frame durations made in place since `timing_before`.
    fn commit_timing(&mut self) {
        if let Some(old) = self.playback.timing_before.take()
            && old != self.doc.durations
        {
            let new = self.doc.durations.clone();
            self.history.push_as(Command::FrameTiming { old, new }, "Frame Timing");
            self.doc.modified = true;
        }
    }

    /// Aseprite-style: layers as rows, frames as columns, a dot per cel.
    pub(super) fn timeline(&mut self, ui: &mut Ui) {
        let rows = self.doc.layers.len() as f32;
        let fits = HEADER_H + rows * CEL.y + 16.0;
        egui::Panel::bottom("timeline")
            .frame(Frame::NONE.fill(theme::PANEL).inner_margin(Margin::symmetric(8, 6)))
            .resizable(true)
            .default_size(fits.min(190.0))
            .size_range(HEADER_H + CEL.y + 16.0..=480.0)
            .show(ui, |ui| {
                let mut editing = false;
                ScrollArea::vertical().id_salt("timeline_rows").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        ui.vertical(|ui| {
                            ui.set_width(LAYER_COLUMN);
                            editing |= self.timeline_corner(ui);
                            self.timeline_layers(ui);
                        });
                        ScrollArea::horizontal().id_salt("timeline_frames").show(ui, |ui| {
                            editing |= self.timeline_frames(ui);
                        });
                    });
                });
                // A duration edit ends when no duration field is being dragged or typed in.
                if !editing {
                    self.commit_timing();
                }
            });
    }

    /// Playback buttons, onion skin and the shown frame's duration. Returns whether the
    /// duration field is in use.
    fn timeline_corner(&mut self, ui: &mut Ui) -> bool {
        let mut editing = false;
        let size = vec2(LAYER_COLUMN, HEADER_H);
        ui.allocate_ui_with_layout(size, Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(size);
            ui.spacing_mut().item_spacing.x = 1.0;
            let last = self.doc.frame_count() - 1;
            let playing = self.playback.playing;
            if icon_button(ui, icons::FIRST, 22.0, false, "First frame").clicked() {
                self.stop_playback();
                self.go_to_frame(0);
            }
            if icon_button(ui, icons::PREVIOUS, 22.0, false, "Previous frame  (,)").clicked() {
                self.step_frame(-1);
            }
            let (icon, tip) = if playing { (icons::PAUSE, "Stop  (Enter)") } else { (icons::PLAY, "Play  (Enter)") };
            if icon_toggle(ui, icon, 22.0, playing, tip).clicked() {
                self.toggle_playback();
            }
            if icon_button(ui, icons::NEXT, 22.0, false, "Next frame  (.)").clicked() {
                self.step_frame(1);
            }
            if icon_button(ui, icons::LAST, 22.0, false, "Last frame").clicked() {
                self.stop_playback();
                self.go_to_frame(last);
            }
            ui.add_space(6.0);
            let onion = self.canvas_state.onion_skin;
            let tip = "Onion skin: the active layer's previous (violet) and next (teal) frames show faintly where it is empty";
            if icon_toggle(ui, icons::ONION, 22.0, onion, tip).clicked() {
                self.canvas_state.onion_skin = !onion;
            }
            ui.add_space(6.0);
            editing = self.duration_field(ui);
        });
        editing
    }

    /// Edits how long the shown frame lasts; the edit becomes one undo step when it ends.
    fn duration_field(&mut self, ui: &mut Ui) -> bool {
        let frame = self.doc.frame;
        let mut ms = self.doc.durations[frame];
        let total: u32 = self.doc.durations.iter().sum();
        let field = ui.add(DragValue::new(&mut ms).range(10..=10_000).speed(5).suffix(" ms")).on_hover_text(format!(
            "How long frame {} shows (≈ {:.1} fps)\nThe whole animation: {:.2} s",
            frame + 1,
            1000.0 / ms as f32,
            total as f32 / 1000.0,
        ));
        if field.changed() {
            self.playback.timing_before.get_or_insert_with(|| self.doc.durations.clone());
            self.doc.durations[frame] = ms;
        }
        field.dragged() || field.has_focus()
    }

    /// One row per layer, the top layer first: visibility, lock and name.
    fn timeline_layers(&mut self, ui: &mut Ui) {
        let mut action = None;
        let count = self.doc.layers.len();
        for i in (0..count).rev() {
            let layer = &self.doc.layers[i];
            let active = i == self.doc.active_layer;
            let row = Rect::from_min_size(ui.cursor().min, vec2(LAYER_COLUMN, CEL.y));
            ui.painter().rect_filled(row, 0.0, if active { theme::SURFACE_ACTIVE } else { theme::BG_DEEP });
            ui.painter().hline(row.x_range(), row.max.y - 0.5, Stroke::new(1.0, theme::PANEL));
            // Ids follow rows, not layers: when a layer comes or goes, the rows below keep
            // their place and their ids (a widget changing id in place is flagged by egui).
            let builder = UiBuilder::new().id(Id::new(("timeline_layer", count - 1 - i))).max_rect(row).sense(Sense::click()).layout(Layout::left_to_right(Align::Center));
            let resp = ui.scope_builder(builder, |ui| {
                ui.set_min_size(row.size());
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(4.0);
                let (eye, eye_tip) = if layer.visible { (icons::EYE, "Hide layer") } else { (icons::EYE_OFF, "Show layer") };
                if icon_button(ui, eye, 22.0, false, eye_tip).clicked() {
                    action = Some(LayerAction::ToggleVisible(i));
                }
                let (lock, lock_tip) = if layer.locked { (icons::LOCK, "Unlock layer") } else { (icons::LOCK_OPEN, "Lock layer") };
                if icon_button(ui, lock, 22.0, false, lock_tip).clicked() {
                    action = Some(LayerAction::ToggleLock(i));
                }
                ui.add_space(6.0);
                let color = if active { theme::TEXT } else if layer.visible { theme::TEXT_DIM } else { theme::TEXT_FAINT };
                ui.add(Label::new(RichText::new(&layer.name).color(color)).truncate().selectable(false));
            });
            if (resp.response.clicked() || resp.response.secondary_clicked()) && action.is_none() {
                action = Some(LayerAction::Select(i));
            }
            resp.response.context_menu(|ui| {
                if let Some(chosen) = layer_menu(ui, &self.doc, i) {
                    action = Some(chosen);
                }
            });
        }
        if let Some(action) = action {
            self.apply_layer_action(action);
        }
    }

    /// Frame numbers over the cels. Returns whether a duration field (in a frame's menu) is in use.
    fn timeline_frames(&mut self, ui: &mut Ui) -> bool {
        let doc = &self.doc;
        let (frames, layers) = (doc.frame_count(), doc.layers.len());
        let size = vec2((frames + 1) as f32 * CEL.x, HEADER_H + layers as f32 * CEL.y);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        let column = |f: usize| rect.min.x + f as f32 * CEL.x;
        let header = |f: usize| Rect::from_min_size(pos2(column(f), rect.min.y), vec2(CEL.x, HEADER_H));
        // Row 0 is the top layer.
        let cel = |row: usize, f: usize| Rect::from_min_size(pos2(column(f), rect.min.y + HEADER_H + row as f32 * CEL.y), CEL);
        let cels = Rect::from_min_max(pos2(rect.min.x, rect.min.y + HEADER_H), pos2(column(frames), rect.max.y));

        painter.rect_filled(cels, 0.0, theme::BG_DEEP);
        let active_row = layers - 1 - doc.active_layer;
        painter.rect_filled(cel(active_row, 0).union(cel(active_row, frames - 1)), 0.0, theme::SURFACE_ACTIVE);
        let shown = Rect::from_x_y_ranges(header(doc.frame).x_range(), rect.y_range());
        painter.rect_filled(shown, 0.0, theme::ACCENT_SOFT);
        for f in 0..=frames {
            painter.vline(column(f), cels.y_range(), Stroke::new(1.0, theme::PANEL));
        }
        for row in 0..=layers {
            painter.hline(cels.x_range(), cels.min.y + row as f32 * CEL.y, Stroke::new(1.0, theme::PANEL));
        }

        let mut act = None;
        let mut editing = false;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let target = |x: f32| (((x - rect.min.x) / CEL.x).floor().max(0.0) as usize).min(frames - 1);
        for f in 0..frames {
            let r = header(f);
            let current = f == doc.frame;
            // Only the columns in view: a long animation has hundreds, and each cel's dot looks
            // through the cel's cells.
            if !ui.is_rect_visible(Rect::from_x_y_ranges(r.x_range(), rect.y_range())) {
                if self.playback.playing && current {
                    ui.scroll_to_rect(r, None);
                }
                continue;
            }
            let resp = ui.interact(r, ui.id().with(("frame", f)), Sense::click_and_drag());
            if self.playback.selected.len() > 1 && self.playback.selected.contains(&f) {
                painter.rect_filled(r.shrink(2.0), 4.0, theme::ACCENT_SOFT);
                painter.rect_stroke(r.shrink(2.0), 4.0, Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
            }
            let color = if current { theme::ACCENT } else if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
            let font = if current { theme::semibold(12.0) } else { theme::mono(11.0) };
            painter.text(r.center(), Align2::CENTER_CENTER, (f + 1).to_string(), font, color);
            if self.canvas_state.onion_skin {
                let tint = if f + 1 == doc.frame { Some(theme::ACCENT) } else if f == doc.frame + 1 { Some(ONION_NEXT) } else { None };
                if let Some(tint) = tint {
                    painter.hline(r.shrink2(vec2(5.0, 0.0)).x_range(), r.max.y - 2.0, Stroke::new(2.0, tint));
                }
            }

            // Drag a frame number sideways to move the frame.
            if resp.dragged()
                && let Some(p) = pointer
            {
                let to = target(p.x);
                let x = if to > f { column(to + 1) } else { column(to) };
                painter.vline(x, rect.y_range(), Stroke::new(2.0, theme::ACCENT));
            }
            if resp.drag_stopped()
                && let Some(p) = pointer
            {
                act = Some(GridAction::Move { from: f, to: target(p.x) });
            } else if resp.clicked() {
                let mods = ui.input(|i| i.modifiers);
                act = Some(if mods.command || mods.shift {
                    GridAction::Pick { frame: f, range: mods.shift }
                } else {
                    GridAction::Go { layer: None, frame: f }
                });
            } else if resp.secondary_clicked() && !self.playback.selected.contains(&f) {
                act = Some(GridAction::Go { layer: None, frame: f });
            }
            if self.playback.playing && current {
                ui.scroll_to_rect(r, None);
            }

            let ms = doc.durations[f];
            let resp = resp.on_hover_ui(|ui| {
                preview(ui, doc, f);
                ui.label(RichText::new(format!("Frame {}  ·  {ms} ms", f + 1)).color(theme::TEXT_DIM));
                ui.label(RichText::new("Drag to move · Ctrl / Shift + click to pick several · right click for more").color(theme::TEXT_FAINT).small());
            });
            let in_hand = if self.playback.selected.len() > 1 && self.playback.selected.contains(&f) { self.playback.selected.len() } else { 1 };
            resp.context_menu(|ui| editing |= frame_menu(ui, doc, f, in_hand, &mut act));

            for row in 0..layers {
                let layer = layers - 1 - row;
                let r = cel(row, f);
                let filled = doc.layers[layer].cells_in(f).iter().any(|c| !c.is_empty());
                let active = layer == doc.active_layer && current;
                let resp = ui.interact(r, ui.id().with(("cel", row, f)), Sense::click());
                if active {
                    painter.rect_stroke(r.shrink(1.0), 2.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
                } else if resp.hovered() {
                    painter.rect_stroke(r.shrink(1.0), 2.0, Stroke::new(1.0, theme::BORDER_STRONG), StrokeKind::Inside);
                }
                let dot = if layer == doc.active_layer || current { theme::TEXT } else { theme::TEXT_DIM };
                if filled {
                    painter.circle_filled(r.center(), 4.5, dot);
                } else {
                    painter.circle_stroke(r.center(), 4.0, Stroke::new(1.0, theme::TEXT_FAINT));
                }
                if resp.clicked() {
                    act = Some(GridAction::Go { layer: Some(layer), frame: f });
                }
            }
        }

        // A last column to add a frame.
        let r = header(frames);
        // Ids follow columns: when a frame goes, the "+" takes its place under the same id
        // (a widget changing id in place is flagged by egui, and loses its hover).
        let resp = ui.interact(r, ui.id().with(("frame", frames)), Sense::click()).on_hover_text("New frame: a copy of the shown one");
        let color = if resp.hovered() { theme::ACCENT } else { theme::TEXT_FAINT };
        painter.text(r.center(), Align2::CENTER_CENTER, icons::PLUS, theme::mono(13.0), color);
        if resp.clicked() {
            act = Some(GridAction::NewFrame { empty: false });
        }

        match act {
            Some(GridAction::Go { layer, frame }) => {
                self.stop_playback();
                self.playback.selected.clear();
                if let Some(layer) = layer {
                    self.apply_layer_action(LayerAction::Select(layer));
                }
                self.go_to_frame(frame);
            }
            Some(GridAction::Pick { frame, range }) => {
                let picked = &mut self.playback.selected;
                if picked.is_empty() {
                    picked.insert(self.doc.frame);
                }
                if range {
                    let (a, b) = (self.doc.frame.min(frame), self.doc.frame.max(frame));
                    picked.extend(a..=b);
                } else if !picked.remove(&frame) {
                    picked.insert(frame);
                }
            }
            Some(GridAction::Move { from, to }) => self.move_frame_to(from, to),
            Some(GridAction::NewFrame { empty: false }) => self.duplicate_frames(),
            Some(GridAction::NewFrame { empty: true }) => self.add_frame(true),
            Some(GridAction::DeleteFrame) => self.delete_frame(),
            Some(GridAction::Duration { frame, ms }) => {
                self.playback.timing_before.get_or_insert_with(|| self.doc.durations.clone());
                // A duration set on one of the picked frames goes to all of them.
                let picked = self.playback.selected.len() > 1 && self.playback.selected.contains(&frame);
                let frames: Vec<usize> = if picked { self.playback.selected.iter().copied().collect() } else { vec![frame] };
                let count = self.doc.durations.len();
                for f in frames.into_iter().filter(|&f| f < count) {
                    self.doc.durations[f] = ms;
                }
            }
            Some(GridAction::DurationForAll(ms)) => {
                let old = self.doc.durations.clone();
                let new = vec![ms; old.len()];
                if new != old {
                    self.history.execute(Command::FrameTiming { old, new }, &mut self.doc);
                }
            }
            None => {}
        }
        editing
    }
}

/// Right click on a frame number (`count` frames picked with it). Returns whether its duration
/// field is in use.
fn frame_menu(ui: &mut Ui, doc: &Document, frame: usize, count: usize, act: &mut Option<GridAction>) -> bool {
    let mut editing = false;
    ui.horizontal(|ui| {
        ui.label("Duration");
        let mut ms = doc.durations[frame];
        let field = ui.add(DragValue::new(&mut ms).range(10..=10_000).speed(5).suffix(" ms"));
        if field.changed() {
            *act = Some(GridAction::Duration { frame, ms });
        }
        editing = field.dragged() || field.has_focus();
    });
    if ui.button("Use for All Frames").clicked() {
        *act = Some(GridAction::DurationForAll(doc.durations[frame]));
        ui.close();
    }
    ui.separator();
    let copy = if count > 1 { format!("Duplicate {count} Frames") } else { "New Frame (Copy)".to_owned() };
    if ui.button(copy).clicked() {
        *act = Some(GridAction::NewFrame { empty: false });
        ui.close();
    }
    if ui.button("New Empty Frame").clicked() {
        *act = Some(GridAction::NewFrame { empty: true });
        ui.close();
    }
    let delete = if count > 1 { format!("Delete {count} Frames") } else { "Delete Frame".to_owned() };
    if ui.add_enabled(doc.frame_count() > count, Button::new(delete)).clicked() {
        *act = Some(GridAction::DeleteFrame);
        ui.close();
    }
    editing
}

/// A picture of `frame`, for the tooltip of its number.
fn preview(ui: &mut Ui, doc: &Document, frame: usize) {
    let (rect, _) = ui.allocate_exact_size(PREVIEW, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::CANVAS_BG);

    // Cells are twice as tall as wide.
    let area = rect.shrink(4.0);
    let aspect = doc.width as f32 / (doc.height as f32 * 2.0);
    let size = if area.width() / area.height() > aspect {
        vec2(area.height() * aspect, area.height())
    } else {
        vec2(area.width(), area.width() / aspect)
    };
    let inner = Rect::from_center_size(area.center(), size);
    let cols = doc.width.min(96);
    let rows = doc.height.min(48).min(((cols as f32 / aspect) / 2.0).ceil() as usize).max(1);
    let cell = vec2(inner.width() / cols as f32, inner.height() / rows as f32);
    // Each dot shows the first visible cell of its block, so thin lines don't fall between samples.
    let mut dots: Vec<Option<Color32>> = vec![None; cols * rows];
    for y in 0..doc.height {
        for x in 0..doc.width {
            let dot = &mut dots[(y * rows / doc.height) * cols + x * cols / doc.width];
            if dot.is_some() { continue; }
            let sample = doc.composite_in(frame, x, y);
            if sample.ch != ' ' {
                *dot = Some(sample.fg.to_egui());
            } else if !sample.bg.is_transparent() {
                *dot = Some(sample.bg.to_egui());
            }
        }
    }
    for (i, color) in dots.into_iter().enumerate() {
        let Some(color) = color else { continue };
        let min = inner.min + vec2((i % cols) as f32 * cell.x, (i / cols) as f32 * cell.y);
        painter.rect_filled(Rect::from_min_size(min, cell), 0.0, color);
    }
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
}
