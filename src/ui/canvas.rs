use egui::*;
use crate::document::Document;
use crate::tools::*;

pub const ZOOM_MIN: f32 = 0.15;
pub const ZOOM_MAX: f32 = 8.0;
/// Gap between the top-left of the view and the canvas at zero offset.
const CANVAS_MARGIN: f32 = 20.0;

pub struct CanvasState {
    pub zoom: f32,
    pub offset: Vec2,
    pub show_grid: bool,
    pub cell_w: f32,
    pub cell_h: f32,
    pub cursor_cell: Option<(usize, usize)>,
    pub march_phase: f32,
    /// Screen area of the canvas view in the last frame.
    pub view_rect: Rect,
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
            view_rect: Rect::NOTHING,
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
        let cx = (rel.x / cs.x).floor();
        let cy = (rel.y / cs.y).floor();
        if cx >= 0.0 && cy >= 0.0 {
            Some((cx as usize, cy as usize))
        } else {
            None
        }
    }

    /// Like [`Self::screen_to_cell`], but always lands on a cell of the document.
    pub fn screen_to_cell_clamped(&self, pos: Pos2, canvas_origin: Pos2, doc: &Document) -> (usize, usize) {
        let cs = self.cell_size();
        let rel = pos - canvas_origin - self.offset;
        let max_x = doc.width.saturating_sub(1) as f32;
        let max_y = doc.height.saturating_sub(1) as f32;
        (
            (rel.x / cs.x).floor().clamp(0.0, max_x) as usize,
            (rel.y / cs.y).floor().clamp(0.0, max_y) as usize,
        )
    }

    pub fn cell_rect(&self, x: usize, y: usize, canvas_origin: Pos2) -> Rect {
        let cs = self.cell_size();
        let base = canvas_origin + self.offset;
        let min = base + Vec2::new(x as f32 * cs.x, y as f32 * cs.y);
        Rect::from_min_size(min, cs)
    }

    pub fn zoom_around(&mut self, factor: f32, pivot: Pos2, canvas_origin: Pos2) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let actual_factor = self.zoom / old_zoom;
        self.offset = pivot - canvas_origin - (pivot - canvas_origin - self.offset) * actual_factor;
    }

    /// Where cell (x, y) was on screen in the last frame.
    pub fn screen_cell_rect(&self, x: usize, y: usize) -> Rect {
        self.cell_rect(x, y, canvas_origin(self.view_rect))
    }

    /// Zooms around the middle of the view (menus and keyboard shortcuts).
    pub fn zoom_view_center(&mut self, factor: f32) {
        if self.view_rect.is_positive() {
            self.zoom_around(factor, self.view_rect.center(), canvas_origin(self.view_rect));
        } else {
            self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        }
    }
}

fn canvas_origin(view: Rect) -> Pos2 {
    view.min + Vec2::splat(CANVAS_MARGIN)
}

pub enum CanvasEvent {
    Press   { x: usize, y: usize, primary: bool },
    Drag    { x: usize, y: usize },
    /// Where the pointer was released, clamped to the document.
    Release { x: usize, y: usize },
}

pub fn draw_canvas(
    ui: &mut egui::Ui,
    doc: &Document,
    state: &mut CanvasState,
    ctx_tool: &DrawContext,
    active_tool: ToolType,
    tools: &mut crate::app::ToolState,
) -> Vec<CanvasEvent> {
    let available = ui.available_rect_before_wrap();
    // Drag-only sense: a press starts the stroke immediately (no click-vs-drag delay).
    let (resp, painter) = ui.allocate_painter(available.size(), Sense::drag());
    let origin = canvas_origin(available);
    state.view_rect = resp.rect;

    let hover_pos = ui.input(|i| i.pointer.hover_pos());
    // While a stroke is dragged outside the canvas, keep the cursor on its edge,
    // so shape previews match what the release will draw.
    state.cursor_cell = if resp.dragged() {
        ui.input(|i| i.pointer.latest_pos()).map(|p| state.screen_to_cell_clamped(p, origin, doc))
    } else if resp.contains_pointer() {
        hover_pos.and_then(|p| state.screen_to_cell(p, origin))
    } else {
        None
    };

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
            let cell = doc.composite_cell(x, y);
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

    draw_moving_selection(tools, doc, state, &painter, origin, &font_id);
    draw_selection(tools, state, &painter, origin, state.march_phase);

    let mut events = Vec::new();

    if resp.contains_pointer() {
        let (zoom, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if zoom != 1.0 {
            if let Some(pivot) = hover_pos {
                state.zoom_around(zoom, pivot, origin);
            }
        } else if scroll != Vec2::ZERO {
            state.offset += scroll;
        }
    }

    if resp.dragged_by(PointerButton::Middle) {
        state.offset += resp.drag_delta();
    }

    let in_doc = |&(x, y): &(usize, usize)| x < doc.width && y < doc.height;

    for (button, primary) in [(PointerButton::Primary, true), (PointerButton::Secondary, false)] {
        if resp.drag_started_by(button) {
            let pos = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos());
            if let Some((x, y)) = pos.and_then(|p| state.screen_to_cell(p, origin)).filter(in_doc) {
                events.push(CanvasEvent::Press { x, y, primary });
            }
        } else if resp.hovered()
            && ui.input(|i| i.pointer.button_pressed(button) && i.pointer.button_released(button))
        {
            // Press and release in the same frame (e.g. touchpad tap): egui reports no drag.
            let pos = ui.input(|i| i.pointer.latest_pos());
            if let Some((x, y)) = pos.and_then(|p| state.screen_to_cell(p, origin)).filter(in_doc) {
                events.push(CanvasEvent::Press { x, y, primary });
                events.push(CanvasEvent::Release { x, y });
            }
        }
    }

    if !resp.drag_started()
        && (resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Secondary))
        && let Some((x, y)) = resp.interact_pointer_pos().and_then(|p| state.screen_to_cell(p, origin))
    {
        events.push(CanvasEvent::Drag { x, y });
    }

    if resp.drag_stopped() {
        let pos = ui.input(|i| i.pointer.latest_pos()).or(resp.interact_pointer_pos());
        if let Some(pos) = pos {
            let (x, y) = state.screen_to_cell_clamped(pos, origin, doc);
            events.push(CanvasEvent::Release { x, y });
        }
    }

    events
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

/// The block being moved with the Select tool; it covers what is underneath.
fn draw_moving_selection(
    tools: &crate::app::ToolState,
    doc: &Document,
    state: &CanvasState,
    painter: &egui::Painter,
    origin: Pos2,
    font_id: &FontId,
) {
    let Some((x0, y0, content)) = tools.selection.moving_content() else { return; };
    for (dy, row) in content.iter().enumerate() {
        for (dx, cell) in row.iter().enumerate() {
            let (x, y) = (x0 + dx, y0 + dy);
            if x >= doc.width || y >= doc.height { continue; }
            let rect = state.cell_rect(x, y, origin);
            let bg = if cell.bg.is_transparent() { Color32::BLACK } else { cell.bg.to_egui() };
            painter.rect_filled(rect, 0.0, bg);
            if cell.ch != ' ' {
                painter.text(rect.center(), Align2::CENTER_CENTER, cell.ch.to_string(), font_id.clone(), cell.fg.to_egui());
            }
        }
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