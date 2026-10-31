use egui::*;
use crate::document::Color;
use crate::theme;
use crate::tools::{Channels, border::BorderStyle};
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

/// 16 VGA colours and a grey ramp.
const PALETTE: [(u8, u8, u8); 24] = [
    (0, 0, 0), (170, 0, 0), (0, 170, 0), (170, 85, 0), (0, 0, 170), (170, 0, 170), (0, 170, 170), (170, 170, 170),
    (85, 85, 85), (255, 85, 85), (85, 255, 85), (255, 255, 85), (85, 85, 255), (255, 85, 255), (85, 255, 255), (255, 255, 255),
    (18, 18, 24), (38, 38, 50), (64, 64, 80), (96, 96, 118), (128, 128, 148), (167, 139, 250), (124, 77, 255), (228, 228, 238),
];

pub struct ColorPanel {
    pub primary_fg:   Color,
    pub primary_bg:   Color,
    pub secondary_fg: Color,
    pub secondary_bg: Color,
    /// The slot the picker and the palette edit.
    pub slot: ColorSlot,
    hex: String,
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

        ui.add_space(6.0);
        section_label(ui, "Palette");
        let size = vec2(22.0, 22.0);
        for row in PALETTE.chunks(8) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                for &(r, g, b) in row {
                    let c = Color32::from_rgb(r, g, b);
                    if color_chip(ui, c, size, false).on_hover_text(to_hex(c)).clicked() {
                        self.set(self.slot, Color::new(r, g, b, 255));
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
    pub channels:      Channels,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            brush_size:   1,
            filled:       false,
            border_style: BorderStyle::Single,
            channels:     Channels::default(),
        }
    }
}
