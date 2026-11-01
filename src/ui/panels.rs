use egui::*;
use crate::document::Color;
use crate::theme;
use crate::tools::{Channels, Mirror, border::BorderStyle};
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

/// Named swatch sets for the Colors panel.
pub const PALETTES: [&str; 6] = ["VGA", "xterm 256", "PICO-8", "Game Boy", "Greys", "Void"];

/// The colours of palette `index` (see [`PALETTES`]).
pub fn palette(index: usize) -> Vec<Color32> {
    let hex = |list: &[u32]| list.iter().map(|&c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)).collect();
    match index {
        0 => hex(&[
            0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA, 0xAAAAAA,
            0x555555, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
        ]),
        1 => {
            // xterm: 16 system colours, a 6×6×6 cube and 24 greys.
            let mut colors: Vec<Color32> = hex(&[
                0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xC0C0C0,
                0x808080, 0xFF0000, 0x00FF00, 0xFFFF00, 0x0000FF, 0xFF00FF, 0x00FFFF, 0xFFFFFF,
            ]);
            const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
            for r in LEVELS { for g in LEVELS { for b in LEVELS { colors.push(Color32::from_rgb(r, g, b)); } } }
            colors.extend((0..24).map(|i| Color32::from_gray(8 + 10 * i)));
            colors
        }
        2 => hex(&[
            0x000000, 0x1D2B53, 0x7E2553, 0x008751, 0xAB5236, 0x5F574F, 0xC2C3C7, 0xFFF1E8,
            0xFF004D, 0xFFA300, 0xFFEC27, 0x00E436, 0x29ADFF, 0x83769C, 0xFF77A8, 0xFFCCAA,
        ]),
        3 => hex(&[0x0F380F, 0x306230, 0x8BAC0F, 0x9BBC0F]),
        4 => (0..16).map(|i| Color32::from_gray(i * 17)).collect(),
        _ => hex(&[
            0x0B0B0F, 0x121218, 0x26263A, 0x3A2668, 0x7C4DFF, 0xA78BFA, 0xC4B5FD, 0xE4E4EE,
            0x78DCE6, 0xF5E6A8, 0xFBBF24, 0xF87171,
        ]),
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
    /// Index into [`PALETTES`].
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

    pub fn show(&mut self, ui: &mut Ui) {
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
        ui.horizontal(|ui| {
            section_label(ui, "Palette");
            ComboBox::from_id_salt("palette")
                .selected_text(PALETTES[self.palette])
                .show_ui(ui, |ui| {
                    for (i, name) in PALETTES.iter().enumerate() {
                        ui.selectable_value(&mut self.palette, i, *name);
                    }
                });
        });
        let colors = palette(self.palette);
        let size = if colors.len() > 32 { 15.0 } else { 22.0 };
        self.swatches(ui, &colors, size);
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
    pub filled:        bool,
    pub border_style:  BorderStyle,
    /// The Line tool draws box-drawing lines (in `border_style`) instead of the glyph.
    pub line_box:      bool,
    /// Index into `shade::RAMPS`.
    pub shade_ramp:    usize,
    pub channels:      Channels,
    pub mirror:        Mirror,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            brush_size:   1,
            filled:       false,
            border_style: BorderStyle::Single,
            line_box:     false,
            shade_ramp:   0,
            channels:     Channels::default(),
            mirror:       Mirror::default(),
        }
    }
}
