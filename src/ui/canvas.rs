use egui::*;
use crate::document::{Document, Stack};
use crate::theme;
use crate::tools::*;
use crate::tools::gradient::GradientSettings;
use crate::tools::brush::BrushKind;
use crate::tools::pixel::PixelGrid;
use crate::tools::shapes::{Shape, ShapeMods};
use crate::ui::glyph_render;

pub const ZOOM_MIN: f32 = 0.15;
pub const ZOOM_MAX: f32 = 8.0;
/// Cell size at 100 %: FiraCode's advance is half its line height, so glyphs fill the cell.
pub const CELL_W: f32 = 10.0;
pub const CELL_H: f32 = 20.0;
/// Gap between the top-left of the view and the canvas at zero offset.
const CANVAS_MARGIN: f32 = 20.0;
/// Free space kept around the canvas when fitting it into the view.
const FIT_PADDING: f32 = 32.0;

pub struct CanvasState {
    pub zoom: f32,
    pub offset: Vec2,
    pub show_grid: bool,
    /// The active layer's previous and next animation frames show faintly where it is empty.
    pub onion_skin: bool,
    pub cursor_cell: Option<(usize, usize)>,
    /// Where the pointer is inside `cursor_cell` (0..1 across and down).
    pub cursor_sub: (f32, f32),
    pub march_phase: f32,
    /// Screen area of the canvas view in the last frame.
    pub view_rect: Rect,
    /// Fit and centre the canvas on the next frame (new or opened document).
    fit_pending: bool,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: Vec2::ZERO,
            show_grid: true,
            onion_skin: false,
            cursor_cell: None,
            cursor_sub: (0.0, 0.0),
            march_phase: 0.0,
            view_rect: Rect::NOTHING,
            fit_pending: true,
        }
    }
}

impl CanvasState {
    pub fn cell_size(&self) -> Vec2 {
        Vec2::new(CELL_W * self.zoom, CELL_H * self.zoom)
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

    /// Where `pos` is inside its cell, 0..1 across and down.
    pub fn sub_position(&self, pos: Pos2, canvas_origin: Pos2) -> (f32, f32) {
        let rel = pos - canvas_origin - self.offset;
        let cs = self.cell_size();
        let (x, y) = (rel.x / cs.x, rel.y / cs.y);
        (x - x.floor(), y - y.floor())
    }

    pub fn cell_rect(&self, x: usize, y: usize, canvas_origin: Pos2) -> Rect {
        // Both edges come from the same formula as the neighbours' edges, so after snapping
        // to pixels neighbouring cells share their border exactly (`min + size` can round
        // differently and leave a one-pixel seam).
        let cs = self.cell_size();
        let base = canvas_origin + self.offset;
        let edge = |x: usize, y: usize| base + Vec2::new(x as f32 * cs.x, y as f32 * cs.y);
        Rect::from_min_max(edge(x, y), edge(x + 1, y + 1))
    }

    /// Where cell (x, y) was on screen in the last frame.
    #[cfg(test)]
    pub fn screen_cell_rect(&self, x: usize, y: usize) -> Rect {
        self.cell_rect(x, y, canvas_origin(self.view_rect))
    }

    pub fn zoom_around(&mut self, factor: f32, pivot: Pos2, canvas_origin: Pos2) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let actual_factor = self.zoom / old_zoom;
        self.offset = pivot - canvas_origin - (pivot - canvas_origin - self.offset) * actual_factor;
    }

    /// Zooms around the middle of the view (menus and keyboard shortcuts).
    pub fn zoom_view_center(&mut self, factor: f32) {
        if self.view_rect.is_positive() {
            self.zoom_around(factor, self.view_rect.center(), canvas_origin(self.view_rect));
        } else {
            self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        }
    }

    /// Zooms so the whole canvas is visible, centred (Ctrl+0).
    pub fn request_fit(&mut self) {
        self.fit_pending = true;
    }

    /// 100 %, keeping the middle of the view in place (Ctrl+1).
    pub fn actual_size(&mut self) {
        self.zoom_view_center(1.0 / self.zoom);
    }

    fn fit(&mut self, doc: &Document) {
        let view = self.view_rect.shrink(FIT_PADDING);
        let full = Vec2::new(doc.width as f32 * CELL_W, doc.height as f32 * CELL_H);
        self.zoom = (view.width() / full.x).min(view.height() / full.y).clamp(ZOOM_MIN, ZOOM_MAX);
        let size = full * self.zoom;
        self.offset = self.view_rect.center() - size / 2.0 - canvas_origin(self.view_rect);
    }
}

fn canvas_origin(view: Rect) -> Pos2 {
    view.min + Vec2::splat(CANVAS_MARGIN)
}

pub enum CanvasEvent {
    /// `sub`: where the pointer is inside the cell, 0..1 across and down (for pixels in cells).
    Press   { x: usize, y: usize, primary: bool, mods: Modifiers, sub: (f32, f32) },
    Drag    { x: usize, y: usize, sub: (f32, f32) },
    /// Where the pointer was released, clamped to the document.
    Release { x: usize, y: usize, mods: Modifiers },
}

/// Settings of the active tool that its preview depends on.
/// Tint of the next frame in the onion skin.
const ONION_NEXT: Color32 = Color32::from_rgb(120, 220, 230);

#[derive(Clone, Copy)]
pub struct PreviewSettings<'a> {
    pub shape: Option<Shape>,
    pub ramp: &'a str,
    pub gradient: GradientSettings<'a>,
    pub pixel_grid: PixelGrid,
    pub brush: &'a BrushKind,
}

/// Shift constrains a shape, Alt draws it from the centre.
pub fn shape_mods(mods: Modifiers) -> ShapeMods {
    ShapeMods { constrain: mods.shift, from_center: mods.alt }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_canvas(
    ui: &mut egui::Ui,
    doc: &Document,
    state: &mut CanvasState,
    ctx_tool: &DrawContext,
    active_tool: ToolType,
    settings: PreviewSettings,
    pan_mode: bool,
    tools: &mut crate::app::ToolState,
) -> Vec<CanvasEvent> {
    let available = ui.available_rect_before_wrap();
    // Drag-only sense: a press starts the stroke immediately (no click-vs-drag delay).
    let (resp, painter) = ui.allocate_painter(available.size(), Sense::drag());
    let origin = canvas_origin(available);
    state.view_rect = resp.rect;
    if state.fit_pending && resp.rect.is_positive() {
        state.fit(doc);
        state.fit_pending = false;
    }

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
    state.cursor_sub = hover_pos.map_or((0.0, 0.0), |p| state.sub_position(p, origin));

    let cs = state.cell_size();
    let canvas_rect = Rect::from_min_size(
        origin + state.offset,
        Vec2::new(doc.width as f32 * cs.x, doc.height as f32 * cs.y),
    );
    let ppp = ui.ctx().pixels_per_point();
    let snap = |rect: Rect| glyph_render::snap_rect(rect, ppp);

    painter.add(Shadow { offset: [0, 8], blur: 28, spread: 2, color: Color32::from_black_alpha(170) }
        .as_shape(canvas_rect, CornerRadius::ZERO));
    painter.rect_filled(snap(canvas_rect), 0.0, theme::CANVAS_BG);

    // Only the cells inside the view.
    let first = |view_min: f32, base: f32, size: f32| ((view_min - base) / size).floor().max(0.0) as usize;
    let last = |view_max: f32, base: f32, size: f32, count: usize| (((view_max - base) / size).ceil().max(0.0) as usize).min(count);
    let (x0, x1) = (first(resp.rect.min.x, canvas_rect.min.x, cs.x), last(resp.rect.max.x, canvas_rect.min.x, cs.x, doc.width));
    let (y0, y1) = (first(resp.rect.min.y, canvas_rect.min.y, cs.y), last(resp.rect.max.y, canvas_rect.min.y, cs.y, doc.height));

    // The grid goes under the content, so it never cuts through glyphs.
    if state.show_grid && cs.x >= 6.0 {
        let pen = Stroke::new(1.0 / ppp, theme::GRID);
        for x in x0..=x1 {
            let sx = snap(state.cell_rect(x, 0, origin)).min.x;
            painter.vline(sx, canvas_rect.y_range(), pen);
        }
        for y in y0..=y1 {
            let sy = snap(state.cell_rect(0, y, origin)).min.y;
            painter.hline(canvas_rect.x_range(), sy, pen);
        }
    }

    let font_id = theme::mono(CELL_H * state.zoom);
    let strokes = glyph_render::Strokes::new(cs.x, ppp);
    for y in y0..y1 {
        for x in x0..x1 {
            let stack = doc.stack_in(doc.frame, x, y);
            if stack.is_empty() { continue; }
            let rect = snap(state.cell_rect(x, y, origin));
            draw_stack(&painter, rect, &stack, None, &font_id, strokes);
        }
    }

    // Onion skin of the active layer (like Aseprite's "current layer" mode, so a background
    // doesn't hide it): its previous frame (violet, wins) and next frame (teal), faintly on
    // top wherever the layer is empty in the shown frame.
    let mut ghosts = Vec::new();
    if state.onion_skin && doc.frame_count() > 1 {
        if doc.frame > 0 { ghosts.push((doc.frame - 1, theme::ACCENT)); }
        if doc.frame + 1 < doc.frame_count() { ghosts.push((doc.frame + 1, ONION_NEXT)); }
    }
    let layer = doc.active_layer();
    if !ghosts.is_empty() && layer.visible {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * doc.width + x;
                if !layer.cells_in(doc.frame)[i].is_empty() { continue; }
                let ghost = ghosts.iter().map(|&(f, tint)| (&layer.cells_in(f)[i], tint)).find(|(g, _)| !g.is_empty());
                if let Some((g, tint)) = ghost {
                    let rect = snap(state.cell_rect(x, y, origin));
                    let bg = if g.bg.is_transparent() { Color32::TRANSPARENT } else { tint.gamma_multiply(0.18) };
                    draw_cell(&painter, rect, g.ch, tint.gamma_multiply(0.55), bg, &font_id, strokes);
                }
            }
        }
    }

    let mods = ui.input(|i| i.modifiers);
    if let Some((cx, cy)) = state.cursor_cell.filter(|_| !pan_mode) {
        let preview = get_preview(active_tool, tools, doc, (cx, cy, state.cursor_sub), ctx_tool, settings, mods);
        let visible = |&&(px, py, _): &&(usize, usize, _)| (x0..x1).contains(&px) && (y0..y1).contains(&py);
        for (px, py, cell) in preview.iter().filter(visible) {
            // Show the result as it will look, with the other layers.
            let stack = doc.stack_with(*px, *py, cell);
            let rect = snap(state.cell_rect(*px, *py, origin));
            draw_stack(&painter, rect, &stack, Some((128, 180)), &font_id, strokes);
        }
    }

    // The gradient's direction while it is dragged.
    if active_tool == ToolType::Gradient
        && let (Some(start), Some(end)) = (tools.gradient.start, state.cursor_cell)
    {
        let (a, b) = (state.cell_rect(start.0, start.1, origin).center(), state.cell_rect(end.0, end.1, origin).center());
        painter.line_segment([a, b], Stroke::new(1.5, theme::ACCENT));
        painter.circle(a, 3.5, theme::ACCENT, Stroke::new(1.0, theme::BG_DEEP));
        painter.circle(b, 3.5, theme::BG_DEEP, Stroke::new(1.5, theme::ACCENT));
    }

    draw_moving_selection(tools, doc, state, &painter, origin, &font_id, strokes);

    // Hovered cell.
    if let Some((cx, cy)) = state.cursor_cell
        && cx < doc.width && cy < doc.height && !resp.dragged()
    {
        let rect = snap(state.cell_rect(cx, cy, origin));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
    }

    // Symmetry axes through the middle of the canvas.
    let axis = theme::ACCENT.gamma_multiply(0.7);
    let center = snap(canvas_rect).center();
    if ctx_tool.mirror.x {
        draw_dashed_line(&painter, pos2(center.x, canvas_rect.min.y), pos2(center.x, canvas_rect.max.y), axis);
    }
    if ctx_tool.mirror.y {
        draw_dashed_line(&painter, pos2(canvas_rect.min.x, center.y), pos2(canvas_rect.max.x, center.y), axis);
    }

    draw_selection(tools, state, &painter, origin, ppp);

    // Text cursor: an outlined cell with a blinking bar.
    if active_tool == ToolType::Text
        && let Some((cx, cy)) = tools.text.cursor
    {
        let rect = snap(state.cell_rect(cx, cy, origin));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::ACCENT), StrokeKind::Inside);
        if (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0 {
            let bar = Rect::from_min_max(pos2(rect.min.x, rect.max.y - 2.0), rect.max);
            painter.rect_filled(bar, 0.0, theme::ACCENT);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    painter.rect_stroke(snap(canvas_rect), 0.0, Stroke::new(1.0, theme::BORDER_STRONG), StrokeKind::Outside);

    if resp.contains_pointer() {
        let moving = active_tool == ToolType::Select
            && state.cursor_cell.is_some_and(|(x, y)| tools.selection.hit(x, y));
        let panning = resp.dragged_by(PointerButton::Middle) || (pan_mode && resp.dragged());
        ui.ctx().set_cursor_icon(if panning {
            CursorIcon::Grabbing
        } else if pan_mode {
            CursorIcon::Grab
        } else if moving {
            CursorIcon::Move
        } else if active_tool == ToolType::Text {
            CursorIcon::Text
        } else {
            CursorIcon::Crosshair
        });
    }

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

    if resp.dragged_by(PointerButton::Middle) || (pan_mode && resp.dragged_by(PointerButton::Primary)) {
        state.offset += resp.drag_delta();
    }

    let in_doc = |&(x, y): &(usize, usize)| x < doc.width && y < doc.height;

    let buttons = if pan_mode { &[][..] } else { &[(PointerButton::Primary, true), (PointerButton::Secondary, false)][..] };
    for &(button, primary) in buttons {
        if resp.drag_started_by(button) {
            let pos = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos());
            if let Some(p) = pos
                && let Some((x, y)) = state.screen_to_cell(p, origin).filter(in_doc)
            {
                let sub = state.sub_position(p, origin);
                events.push(CanvasEvent::Press { x, y, primary, mods, sub });
            }
        } else if resp.hovered()
            && ui.input(|i| i.pointer.button_pressed(button) && i.pointer.button_released(button))
        {
            // Press and release in the same frame (e.g. touchpad tap): egui reports no drag.
            let pos = ui.input(|i| i.pointer.latest_pos());
            if let Some(p) = pos
                && let Some((x, y)) = state.screen_to_cell(p, origin).filter(in_doc)
            {
                let sub = state.sub_position(p, origin);
                events.push(CanvasEvent::Press { x, y, primary, mods, sub });
                events.push(CanvasEvent::Release { x, y, mods });
            }
        }
    }

    if !resp.drag_started()
        && !pan_mode
        && (resp.dragged_by(PointerButton::Primary) || resp.dragged_by(PointerButton::Secondary))
        && let Some(p) = resp.interact_pointer_pos()
        && let Some((x, y)) = state.screen_to_cell(p, origin)
    {
        events.push(CanvasEvent::Drag { x, y, sub: state.sub_position(p, origin) });
    }

    if resp.drag_stopped() {
        let pos = ui.input(|i| i.pointer.latest_pos()).or(resp.interact_pointer_pos());
        if let Some(pos) = pos {
            let (x, y) = state.screen_to_cell_clamped(pos, origin, doc);
            events.push(CanvasEvent::Release { x, y, mods });
        }
    }

    events
}

/// A composited cell: its background, then each glyph, bottom first. `dim` caps the alpha of the
/// background and of the glyphs (for previews).
pub(crate) fn draw_stack(painter: &Painter, rect: Rect, stack: &Stack, dim: Option<(u8, u8)>, font_id: &FontId, strokes: glyph_render::Strokes) {
    let color = |c: &crate::document::Color, cap: Option<u8>| {
        Color32::from_rgba_unmultiplied(c.r, c.g, c.b, cap.map_or(c.a, |cap| c.a.min(cap)))
    };
    let mut bg = color(&stack.bg, dim.map(|d| d.0));
    for (ch, fg) in &stack.glyphs {
        draw_cell(painter, rect, *ch, color(fg, dim.map(|d| d.1)), bg, font_id, strokes);
        bg = Color32::TRANSPARENT;
    }
    if stack.glyphs.is_empty() && bg.a() > 0 {
        painter.rect_filled(rect, 0.0, bg);
    }
}

/// One cell: background, then the glyph (procedurally for box / block / braille characters).
pub(crate) fn draw_cell(painter: &Painter, rect: Rect, ch: char, fg: Color32, bg: Color32, font_id: &FontId, strokes: glyph_render::Strokes) {
    if bg.a() > 0 {
        painter.rect_filled(rect, 0.0, bg);
    }
    if ch != ' ' && !glyph_render::draw(painter, ch, rect, fg, strokes) {
        painter.text(rect.center(), Align2::CENTER_CENTER, ch, font_id.clone(), fg);
    }
}

fn get_preview(
    tool: ToolType,
    tools: &crate::app::ToolState,
    doc: &Document,
    (x, y, sub): (usize, usize, (f32, f32)),
    ctx: &DrawContext,
    settings: PreviewSettings,
    mods: Modifiers,
) -> Vec<(usize, usize, crate::document::Cell)> {
    match (tool, settings.shape) {
        (ToolType::Brush, _) => tools.brush.preview(doc, x, y, settings.brush, ctx),
        (ToolType::Pixel, _) => tools.pixel.preview(doc, settings.pixel_grid.pixel_at(x, y, sub), settings.pixel_grid, ctx),
        (ToolType::Shade, _) => tools.shade.preview(doc, x, y, ctx, settings.ramp),
        (ToolType::Gradient, _) => tools.gradient.preview(doc, (x, y), ctx, settings.gradient),
        (_, Some(shape)) => tools.shape.preview(doc, shape, (x, y), shape_mods(mods), ctx),
        _ => vec![],
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
    strokes: glyph_render::Strokes,
) {
    let Some((x0, y0, content)) = tools.selection.moving_content() else { return; };
    for (dy, row) in content.iter().enumerate() {
        for (dx, cell) in row.iter().enumerate() {
            let (x, y) = (x0 + dx, y0 + dy);
            if x >= doc.width || y >= doc.height { continue; }
            let rect = glyph_render::snap_rect(state.cell_rect(x, y, origin), painter.pixels_per_point());
            painter.rect_filled(rect, 0.0, theme::CANVAS_BG);
            draw_cell(painter, rect, cell.ch, cell.fg.to_egui(), cell.bg.to_egui(), font_id, strokes);
        }
    }
}

/// The selection: a violet tint and "marching ants" (white dashes on a black line,
/// visible on any background).
fn draw_selection(
    tools: &crate::app::ToolState,
    state: &CanvasState,
    painter: &egui::Painter,
    origin: Pos2,
    ppp: f32,
) {
    let Some((x1, y1, x2, y2)) = tools.selection.selection else { return; };

    let r1 = state.cell_rect(x1, y1, origin);
    let r2 = state.cell_rect(x2 + 1, y2 + 1, origin);
    let sel_rect = glyph_render::snap_rect(Rect::from_two_pos(r1.min, r2.min), ppp);

    painter.rect_filled(sel_rect, 0.0, theme::SELECTION_TINT);
    painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Inside);

    // The ants move at a constant speed, whatever the size of the selection.
    let (dash, gap) = (6.0_f32, 4.0_f32);
    let offset = state.march_phase * 4.0 * (dash + gap);
    draw_dashed_rect(painter, sel_rect.shrink(0.5), Color32::WHITE, offset, dash, gap);
}

fn draw_dashed_line(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
    let (dash, gap) = (8.0, 5.0);
    let len = a.distance(b);
    let dir = (b - a) / len.max(0.001);
    let mut pos = 0.0;
    while pos < len {
        let end = (pos + dash).min(len);
        painter.line_segment([a + dir * pos, a + dir * end], Stroke::new(1.0, color));
        pos += dash + gap;
    }
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