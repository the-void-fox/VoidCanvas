//! Animation: frames, playback and the timeline under the canvas.

use eframe::egui::{self, *};

use super::AsciiArtEditor;
use crate::document::Document;
use crate::history::Command;
use crate::ui::widgets::{icon_button, icon_toggle, vdivider};
use crate::{icons, theme};

#[derive(Default)]
pub struct Playback {
    pub playing: bool,
    /// Milliseconds the shown frame has been on screen.
    elapsed: f64,
    /// Frame durations before the duration field started changing (one undo step per edit).
    timing_before: Option<Vec<u32>>,
}

const THUMB: Vec2 = vec2(52.0, 34.0);

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

    pub(super) fn delete_frame(&mut self) {
        if self.doc.frame_count() < 2 { return; }
        self.stop_playback();
        let index = self.doc.frame;
        let data = self.doc.frame_data(index);
        self.history.execute(Command::FrameRemove { index, data }, &mut self.doc);
    }

    /// Moves the shown frame `step` places to the right (left if negative).
    pub(super) fn move_frame(&mut self, step: isize) {
        let from = self.doc.frame;
        let to = from as isize + step;
        if to < 0 || to >= self.doc.frame_count() as isize { return; }
        self.history.execute(Command::FrameMove { from, to: to as usize }, &mut self.doc);
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

    pub(super) fn timeline(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("timeline")
            .frame(Frame::NONE.fill(theme::PANEL).inner_margin(Margin::symmetric(10, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let playing = self.playback.playing;
                    if icon_button(ui, icons::SKIP_PREVIOUS, 26.0, false, "Previous frame  (,)").clicked() {
                        self.step_frame(-1);
                    }
                    let (icon, tip) = if playing { (icons::PAUSE, "Stop  (Enter)") } else { (icons::PLAY, "Play  (Enter)") };
                    if icon_toggle(ui, icon, 26.0, playing, tip).clicked() {
                        self.toggle_playback();
                    }
                    if icon_button(ui, icons::SKIP_NEXT, 26.0, false, "Next frame  (.)").clicked() {
                        self.step_frame(1);
                    }
                    vdivider(ui);

                    let strip_width = (ui.available_width() - 470.0).max(THUMB.x + 8.0);
                    ScrollArea::horizontal()
                        .max_width(strip_width)
                        .id_salt("frames")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for frame in 0..self.doc.frame_count() {
                                    let current = frame == self.doc.frame;
                                    let resp = thumbnail(ui, &self.doc, frame, current);
                                    if current && playing {
                                        resp.scroll_to_me(None);
                                    }
                                    let ms = self.doc.durations[frame];
                                    if resp.on_hover_text(format!("Frame {}  ·  {ms} ms", frame + 1)).clicked() {
                                        self.stop_playback();
                                        self.go_to_frame(frame);
                                    }
                                }
                            });
                        });
                    vdivider(ui);

                    let more = self.doc.frame_count() > 1;
                    if icon_button(ui, icons::PLUS, 26.0, false, "New frame: a copy of this one").clicked() { self.add_frame(false); }
                    if icon_button(ui, icons::FILE, 26.0, false, "New empty frame").clicked() { self.add_frame(true); }
                    if ui.add_enabled_ui(more, |ui| icon_button(ui, icons::DELETE, 26.0, false, "Delete frame")).inner.clicked() {
                        self.delete_frame();
                    }
                    let (first, last) = (self.doc.frame == 0, self.doc.frame + 1 == self.doc.frame_count());
                    if ui.add_enabled_ui(!first, |ui| icon_button(ui, icons::ARROW_LEFT, 26.0, false, "Move frame left")).inner.clicked() {
                        self.move_frame(-1);
                    }
                    if ui.add_enabled_ui(!last, |ui| icon_button(ui, icons::ARROW_RIGHT, 26.0, false, "Move frame right")).inner.clicked() {
                        self.move_frame(1);
                    }
                    vdivider(ui);

                    // The field edits the durations in place; the edit becomes one undo step when it ends.
                    let frame = self.doc.frame;
                    let mut ms = self.doc.durations[frame];
                    let field = ui.add(DragValue::new(&mut ms).range(10..=10_000).speed(5).suffix(" ms"))
                        .on_hover_text(format!("How long frame {} shows (≈ {:.1} fps)", frame + 1, 1000.0 / ms as f32));
                    if field.changed() {
                        self.playback.timing_before.get_or_insert_with(|| self.doc.durations.clone());
                        self.doc.durations[frame] = ms;
                    }
                    if !field.dragged() && !field.has_focus() {
                        self.commit_timing();
                    }
                    if ui.add_enabled(more, Button::new("All")).on_hover_text("Give every frame this duration").clicked() {
                        let old = self.doc.durations.clone();
                        let new = vec![ms; old.len()];
                        if new != old {
                            self.history.execute(Command::FrameTiming { old, new }, &mut self.doc);
                        }
                    }
                    vdivider(ui);
                    let onion = &mut self.canvas_state.onion_skin;
                    ui.toggle_value(onion, "Onion").on_hover_text("Show the previous and next frames faintly under empty cells");

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let total: u32 = self.doc.durations.iter().sum();
                        let info = format!("{} / {}  ·  {:.1} s", self.doc.frame + 1, self.doc.frame_count(), total as f32 / 1000.0);
                        ui.label(RichText::new(info).font(theme::mono(12.0)).color(theme::TEXT_DIM));
                    });
                });
            });
    }
}

/// A small picture of `frame`; the shown frame is outlined.
fn thumbnail(ui: &mut Ui, doc: &Document, frame: usize, current: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(THUMB, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::CANVAS_BG);

    // Cells are twice as tall as wide; sample the picture at most 32 columns across.
    let area = rect.shrink(3.0);
    let aspect = doc.width as f32 / (doc.height as f32 * 2.0);
    let size = if area.width() / area.height() > aspect {
        vec2(area.height() * aspect, area.height())
    } else {
        vec2(area.width(), area.width() / aspect)
    };
    let inner = Rect::from_center_size(area.center(), size);
    let cols = doc.width.min(32);
    let rows = doc.height.min(32).min(((cols as f32 / aspect) / 2.0).ceil() as usize).max(1);
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

    let outline = if current {
        Stroke::new(2.0, theme::ACCENT)
    } else if resp.hovered() {
        Stroke::new(1.0, theme::BORDER_STRONG)
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    painter.rect_stroke(rect, 4.0, outline, StrokeKind::Inside);
    let number = (frame + 1).to_string();
    let corner = rect.left_bottom() + vec2(4.0, -2.0);
    painter.text(corner + vec2(1.0, 1.0), Align2::LEFT_BOTTOM, &number, theme::mono(10.0), Color32::BLACK);
    painter.text(corner, Align2::LEFT_BOTTOM, &number, theme::mono(10.0), if current { theme::ACCENT } else { theme::TEXT_DIM });
    resp
}
