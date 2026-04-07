use egui::*;
use crate::document::Document;
use crate::tools::*;

pub struct CanvasState {
    pub zoom: f32,
    pub offset: Vec2,
    pub show_grid: bool,
    pub cell_w: f32,
    pub cell_h: f32,
    pub cursor_cell: Option<(usize, usize)>,
    pub march_phase: f32,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: Vec2::ZERO,
            show_grid: true,
            cell_w: 10.0,
            cell_h: 16.0,
            cursor_cell: None,
            march_phase: 0.0,
        }
    }
}

impl CanvasState {
    pub fn cell_size(&self) -> Vec2 {
        Vec2::new(self.cell_w * self.zoom, self.cell_h * self.zoom)
    }

    pub fn screen_to_cell(&self, pos: Pos2, canvas_origin: Pos2) -> Option<(usize, usize)> {
        let cs = self.cell_size();
        let rel = pos - canvas_origin - self.offset;
        let cx = (rel.x / cs.x) as isize;
        let cy = (rel.y / cs.y) as isize;
        if cx >= 0 && cy >= 0 {
            Some((cx as usize, cy as usize))
        } else {
            None
        }
    }

    pub fn cell_rect(&self, x: usize, y: usize, canvas_origin: Pos2) -> Rect {
        let cs = self.cell_size();
        let base = canvas_origin + self.offset;
        let min = base + Vec2::new(x as f32 * cs.x, y as f32 * cs.y);
        Rect::from_min_size(min, cs)
    }

    pub fn zoom_around(&mut self, factor: f32, pivot: Pos2, canvas_origin: Pos2) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.15, 8.0);
        let actual_factor = self.zoom / old_zoom;
        self.offset = pivot - canvas_origin - (pivot - canvas_origin - self.offset) * actual_factor;
    }
}

pub enum CanvasEvent {
    Press  { x: usize, y: usize, primary: bool },
    Drag   { x: usize, y: usize, primary: bool },
    Release,
}

pub fn draw_canvas(
    ui: &mut egui::Ui,
    doc: &Document,
    state: &mut CanvasState,
    ctx_tool: &DrawContext,
    active_tool: ToolType,
    tools: &mut crate::app::ToolState,
) -> Option<CanvasEvent> {
    let available = ui.available_rect_before_wrap();
    let (resp, painter) = ui.allocate_painter(available.size(), Sense::click_and_drag());
    let origin = available.min + Vec2::splat(20.0);

    painter.rect_filled(resp.rect, 0.0, Color32::from_rgb(28, 28, 28));

    let cs = state.cell_size();

    let canvas_rect = Rect::from_min_size(
        origin + state.offset,
        Vec2::new(doc.width as f32 * cs.x, doc.height as f32 * cs.y),
    );
    painter.rect_filled(canvas_rect, 0.0, Color32::BLACK);

    let font_id = FontId::monospace((state.cell_h * state.zoom * 0.85).max(6.0));
    for y in 0..doc.height {
        for x in 0..doc.width {
            let cell_rect = state.cell_rect(x, y, origin);
            if !resp.rect.intersects(cell_rect) { continue; }
            if let Some(cell) = doc.composite_cell(x, y) {
                if !cell.bg.is_transparent() {
                    painter.rect_filled(cell_rect, 0.0, cell.bg.to_egui());
                }
                if cell.ch != ' ' {
                    painter.text(
                        cell_rect.center(),
                        Align2::CENTER_CENTER,
                        cell.ch.to_string(),
                        font_id.clone(),
                        cell.fg.to_egui(),
                    );
                }
            }
        }
    }

    if state.show_grid && state.zoom >= 0.4 {
        let pen  = Stroke::new(0.5, Color32::from_gray(50));
        let base = origin + state.offset;
        for x in 0..=doc.width {
            let sx = base.x + x as f32 * cs.x;
            painter.line_segment(
                [Pos2::new(sx, base.y), Pos2::new(sx, base.y + doc.height as f32 * cs.y)],
                pen,
            );
        }
        for y in 0..=doc.height {
            let sy = base.y + y as f32 * cs.y;
            painter.line_segment(
                [Pos2::new(base.x, sy), Pos2::new(base.x + doc.width as f32 * cs.x, sy)],
                pen,
            );
        }
    }

    if let Some((cx, cy)) = state.cursor_cell {
        let preview = get_preview(active_tool, tools, doc, cx, cy, ctx_tool);
        for (px, py, cell) in &preview {
            let cell_rect = state.cell_rect(*px, *py, origin);
            if !cell.bg.is_transparent() {
                painter.rect_filled(
                    cell_rect, 0.0,
                    Color32::from_rgba_unmultiplied(cell.bg.r, cell.bg.g, cell.bg.b, 128),
                );
            }
            if cell.ch != ' ' {
                painter.text(
                    cell_rect.center(), Align2::CENTER_CENTER,
                    cell.ch.to_string(), font_id.clone(),
                    Color32::from_rgba_unmultiplied(cell.fg.r, cell.fg.g, cell.fg.b, 180),
                );
            }
        }
    }

    draw_selection(tools, state, &painter, origin, state.march_phase);

    let mut event: Option<CanvasEvent> = None;

    let hover_pos = ui.ctx().input(|i| i.pointer.hover_pos());
    let is_over_canvas = hover_pos
        .map(|p| resp.rect.contains(p))
        .unwrap_or(false);

    state.cursor_cell = if is_over_canvas {
        hover_pos.and_then(|p| state.screen_to_cell(p, origin))
    } else {
        None
    };

    if is_over_canvas {
        ui.ctx().input(|i| {
            if i.modifiers.ctrl {
                let zoom = i.zoom_delta();
                if zoom != 1.0 {
                    if let Some(pivot) = hover_pos {
                        state.zoom_around(zoom, pivot, origin);
                    }
                }
                let raw_y = i.raw_scroll_delta.y;
                if raw_y != 0.0 {
                    let factor = if raw_y > 0.0 { 1.12_f32 } else { 1.0 / 1.12 };
                    if let Some(pivot) = hover_pos {
                        state.zoom_around(factor, pivot, origin);
                    }
                }
            } else {
                let scroll = i.smooth_scroll_delta;
                if scroll != Vec2::ZERO {
                    state.offset += scroll;
                } else {
                    let raw = i.raw_scroll_delta;
                    if raw != Vec2::ZERO {
                        state.offset += raw * 0.8;
                    }
                }
            }
        });
    }

    if resp.dragged_by(PointerButton::Middle) {
        state.offset += resp.drag_delta();
    }

    let drag_btn_id = resp.id.with("drag_btn");

    if let Some(pos) = resp.interact_pointer_pos() {
        let cell = state.screen_to_cell(pos, origin);

        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().data_mut(|d| d.insert_temp::<bool>(drag_btn_id, true));
            if let Some((cx, cy)) = cell {
                event = Some(CanvasEvent::Press { x: cx, y: cy, primary: true });
            }
        } else if resp.drag_started_by(PointerButton::Secondary) {
            ui.ctx().data_mut(|d| d.insert_temp::<bool>(drag_btn_id, false));
            if let Some((cx, cy)) = cell {
                event = Some(CanvasEvent::Press { x: cx, y: cy, primary: false });
            }
        } else if resp.dragged_by(PointerButton::Primary)
               || resp.dragged_by(PointerButton::Secondary)
        {
            let is_primary: bool = ui.ctx()
                .data(|d| d.get_temp(drag_btn_id).unwrap_or(true));
            if let Some((cx, cy)) = cell {
                event = Some(CanvasEvent::Drag { x: cx, y: cy, primary: is_primary });
            }
        }
    }

    if resp.drag_stopped() {
        event = Some(CanvasEvent::Release);
    }

    event
}

fn get_preview(
    tool: ToolType,
    tools: &crate::app::ToolState,
    doc: &Document,
    x: usize, y: usize,
    ctx: &DrawContext,
) -> Vec<(usize, usize, crate::document::Cell)> {
    match tool {
        ToolType::Brush     => tools.brush.preview(doc, x, y, ctx),
        ToolType::Rectangle => tools.rect.preview(doc, x, y, ctx),
        ToolType::Ellipse   => tools.ellipse.preview(doc, x, y, ctx),
        ToolType::Line      => tools.line.preview(doc, x, y, ctx),
        ToolType::Border    => tools.border.preview(doc, x, y, ctx),
        _                   => vec![],
    }
}

fn draw_selection(
    tools: &crate::app::ToolState,
    state: &CanvasState,
    painter: &egui::Painter,
    origin: Pos2,
    phase: f32,
) {
    let Some((x1, y1, x2, y2)) = tools.selection.selection else { return; };

    let r1 = state.cell_rect(x1, y1, origin);
    let r2 = state.cell_rect(x2 + 1, y2 + 1, origin);
    let sel_rect = Rect::from_two_pos(r1.min, r2.min);

    let dash = 6.0_f32;
    let gap  = 4.0_f32;
    let total = 2.0 * (sel_rect.width() + sel_rect.height());
    let offset = (phase * total) % (dash + gap);

    draw_dashed_rect(painter, sel_rect, Color32::WHITE, offset,            dash, gap);
    draw_dashed_rect(painter, sel_rect, Color32::BLACK, offset + dash + gap, dash, gap);
}

fn draw_dashed_rect(
    painter: &egui::Painter,
    rect: Rect,
    color: Color32,
    offset: f32,
    dash: f32,
    gap: f32,
) {
    let corners = [
        rect.min,
        Pos2::new(rect.max.x, rect.min.y),
        rect.max,
        Pos2::new(rect.min.x, rect.max.y),
    ];
    let stroke = Stroke::new(1.0, color);
    let period = dash + gap;
    let mut dist = offset % period;

    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let seg_len = a.distance(b);
        if seg_len < 0.001 { continue; }
        let dir = (b - a) / seg_len;
        let mut pos = 0.0_f32;
        while pos < seg_len {
            let drawing   = dist < dash;
            let remaining = if drawing { dash - dist } else { period - dist };
            let step      = remaining.min(seg_len - pos);
            if drawing {
                painter.line_segment([a + dir * pos, a + dir * (pos + step)], stroke);
            }
            pos  += step;
            dist  = (dist + step) % period;
        }
    }
}