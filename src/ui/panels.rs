use egui::*;
use crate::document::Color;
use crate::library::Palette;
use crate::theme;
use crate::tools::{Channels, Mirror, border::BorderStyle, brush::BrushKind, gradient::GradientShape, pixel::PixelGrid};
use crate::ui::widgets::{color_chip, parse_hex, section_label, to_hex};

/// One of the four colour slots.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorSlot {
    PrimaryFg,
    PrimaryBg,
    SecondaryFg,
    SecondaryBg,
}

impl ColorSlot {
    fn describe(self) -> &'static str {
        match self {
            ColorSlot::PrimaryFg   => "Primary · foreground",
            ColorSlot::PrimaryBg   => "Primary · background",
            ColorSlot::SecondaryFg => "Secondary · foreground",
            ColorSlot::SecondaryBg => "Secondary · background",
        }
    }
}

pub struct ColorPanel {
    pub primary_fg:   Color,
    pub primary_bg:   Color,
    pub secondary_fg: Color,
    pub secondary_bg: Color,
    /// The slot the picker and the palette edit.
    pub slot: ColorSlot,
    hex: String,
    /// Index into all palettes (`Settings::all_palettes`).
    pub palette: usize,
    /// Recently used colours, newest first.
    pub recent: Vec<Color>,
}

impl Default for ColorPanel {
    fn default() -> Self {
        Self {
            primary_fg:   Color::WHITE,
            primary_bg:   Color::TRANSPARENT,
            secondary_fg: Color::new(128, 128, 128, 255),
            secondary_bg: Color::TRANSPARENT,
            slot:         ColorSlot::PrimaryFg,
            hex:          String::new(),
            palette:      0,
            recent:       Vec::new(),
        }
    }
}

impl ColorPanel {
    pub fn get(&self, slot: ColorSlot) -> &Color {
        match slot {
            ColorSlot::PrimaryFg   => &self.primary_fg,
            ColorSlot::PrimaryBg   => &self.primary_bg,
            ColorSlot::SecondaryFg => &self.secondary_fg,
            ColorSlot::SecondaryBg => &self.secondary_bg,
        }
    }

    fn set(&mut self, slot: ColorSlot, color: Color) {
        *match slot {
            ColorSlot::PrimaryFg   => &mut self.primary_fg,
            ColorSlot::PrimaryBg   => &mut self.primary_bg,
            ColorSlot::SecondaryFg => &mut self.secondary_fg,
            ColorSlot::SecondaryBg => &mut self.secondary_bg,
        } = color;
    }

    pub fn remember(&mut self, color: &Color) {
        if color.is_transparent() { return; }
        self.recent.retain(|c| c != color);
        self.recent.insert(0, color.clone());
        self.recent.truncate(16);
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.primary_fg, &mut self.secondary_fg);
        std::mem::swap(&mut self.primary_bg, &mut self.secondary_bg);
    }

    /// Returns whether the user asked to manage palettes.
    pub fn show(&mut self, ui: &mut Ui, palettes: &[Palette]) -> bool {
        let mut manage = false;
        section_label(ui, "Slots");
        Grid::new("color_slots").num_columns(3).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Fore").small().color(theme::TEXT_DIM));
            ui.label(RichText::new("Back").small().color(theme::TEXT_DIM));
            ui.end_row();
            for (label, fg, bg) in [
                ("Primary", ColorSlot::PrimaryFg, ColorSlot::PrimaryBg),
                ("Secondary", ColorSlot::SecondaryFg, ColorSlot::SecondaryBg),
            ] {
                ui.label(label);
                for slot in [fg, bg] {
                    let chip = color_chip(ui, self.get(slot).to_egui(), vec2(48.0, 24.0), self.slot == slot);
                    if chip.on_hover_text(slot.describe()).clicked() {
                        self.slot = slot;
                    }
                }
                ui.end_row();
            }
        });

        ui.add_space(6.0);
        section_label(ui, self.slot.describe());
        let mut color = self.get(self.slot).to_egui();
        let changed = ui.scope(|ui| {
            // The picker is a square as wide as the slider width.
            ui.spacing_mut().slider_width = (ui.available_width() - 20.0).clamp(120.0, 200.0);
            color_picker::color_picker_color32(ui, &mut color, color_picker::Alpha::OnlyBlend)
        }).inner;
        if changed {
            self.set(self.slot, Color::from_egui(color));
        }

        let hex_id = ui.make_persistent_id("color_hex");
        if !ui.memory(|m| m.has_focus(hex_id)) {
            self.hex = to_hex(self.get(self.slot).to_egui());
        }
        ui.horizontal(|ui| {
            let edit = ui.add(
                TextEdit::singleline(&mut self.hex)
                    .id(hex_id)
                    .font(TextStyle::Monospace)
                    .desired_width(100.0),
            );
            if edit.lost_focus()
                && let Some(c) = parse_hex(&self.hex)
            {
                self.set(self.slot, Color::from_egui(c));
            }
            if ui.button("None").on_hover_text("Fully transparent").clicked() {
                let mut c = self.get(self.slot).clone();
                c.a = 0;
                self.set(self.slot, c);
            }
        });

        if !self.recent.is_empty() {
            ui.add_space(6.0);
            section_label(ui, "Recent");
            let recent: Vec<Color32> = self.recent.iter().map(Color::to_egui).collect();
            self.swatches(ui, &recent, 22.0);
        }

        ui.add_space(6.0);
        self.palette = self.palette.min(palettes.len().saturating_sub(1));
        ui.horizontal(|ui| {
            section_label(ui, "Palette");
            ComboBox::from_id_salt("palette")
                .selected_text(palettes.get(self.palette).map_or("", |p| p.name.as_str()))
                .show_ui(ui, |ui| {
                    for (i, p) in palettes.iter().enumerate() {
                        ui.selectable_value(&mut self.palette, i, &p.name);
                    }
                    ui.separator();
                    manage = ui.selectable_label(false, "Manage…").clicked();
                });
        });
        let colors: Vec<Color32> = palettes.get(self.palette).map_or(Vec::new(), |p| p.colors.iter().map(Color::to_egui).collect());
        let size = if colors.len() > 32 { 15.0 } else { 22.0 };
        self.swatches(ui, &colors, size);
        manage
    }

    /// Clickable colour chips in rows; a click puts the colour into the selected slot.
    fn swatches(&mut self, ui: &mut Ui, colors: &[Color32], size: f32) {
        let gap = if size < 20.0 { 2.0 } else { 5.0 };
        let per_row = (((ui.available_width() + gap) / (size + gap)) as usize).max(1);
        for row in colors.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(gap, gap);
                for &c in row {
                    if color_chip(ui, c, vec2(size, size), false).on_hover_text(to_hex(c)).clicked() {
                        self.set(self.slot, Color::from_egui(c));
                    }
                }
            });
        }
    }
}

/// Settings of the drawing tools, edited in the options bar.
pub struct ToolOptions {
    pub brush_size:    usize,
    /// What the Brush paints, and the name of the preset it came from.
    pub brush:         BrushKind,
    pub brush_name:    String,
    pub filled:        bool,
    pub border_style:  BorderStyle,
    /// The Line tool draws box-drawing lines (in `border_style`) instead of the glyph.
    pub line_box:      bool,
    /// The density ramp of Shade and Gradient (glyphs, and its name in the library).
    pub ramp:          String,
    pub ramp_name:     String,
    pub gradient:      GradientShape,
    /// How finely the Pixel tool splits a cell.
    pub pixel_grid:    PixelGrid,
    /// Gradient glyphs are dithered.
    pub dither:        bool,
    pub channels:      Channels,
    pub mirror:        Mirror,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            brush_size:   1,
            brush:        BrushKind::Square,
            brush_name:   "Square".into(),
            filled:       false,
            border_style: BorderStyle::Single,
            line_box:     false,
            ramp:         " ░▒▓█".into(),
            ramp_name:    "Blocks".into(),
            gradient:     GradientShape::Linear,
            pixel_grid:   PixelGrid::Halves,
            dither:       true,
            channels:     Channels::default(),
            mirror:       Mirror::default(),
        }
    }
}
