//! The shared pools the user can add to: glyph sets, density ramps, brushes and colour palettes.
//! Built-in entries come first; the user's own are kept in the settings.

use serde::{Deserialize, Serialize};

use crate::document::Color;

/// A named density ramp, from empty to full. The Shade and Gradient tools, 3D layers and
/// picture import all draw from the same ramps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ramp {
    pub name: String,
    pub glyphs: String,
}

/// A named set of colours for the Colors panel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<Color>,
}

pub fn builtin_ramps() -> Vec<Ramp> {
    let ramp = |name: &str, glyphs: &str| Ramp { name: name.to_owned(), glyphs: glyphs.to_owned() };
    vec![
        ramp("Blocks", " ░▒▓█"),
        ramp("ASCII", " .:-=+*#%@"),
        ramp("ASCII Fine", " .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"),
        ramp("Dots", " ⠁⠃⠇⡇⣇⣧⣷⣿"),
        ramp("Round", " .°oO@"),
    ]
}

pub fn builtin_palettes() -> Vec<Palette> {
    let hex = |list: &[u32]| list.iter().map(|&c| Color::new((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)).collect::<Vec<_>>();
    let palette = |name: &str, colors: Vec<Color>| Palette { name: name.to_owned(), colors };
    // xterm: 16 system colours, a 6×6×6 cube and 24 greys.
    let mut xterm = hex(&[
        0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xC0C0C0,
        0x808080, 0xFF0000, 0x00FF00, 0xFFFF00, 0x0000FF, 0xFF00FF, 0x00FFFF, 0xFFFFFF,
    ]);
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    for r in LEVELS { for g in LEVELS { for b in LEVELS { xterm.push(Color::new(r, g, b, 255)); } } }
    xterm.extend((0..24).map(|i| { let v = 8 + 10 * i; Color::new(v, v, v, 255) }));
    vec![
        palette("VGA", hex(&[
            0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA, 0xAAAAAA,
            0x555555, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
        ])),
        palette("xterm 256", xterm),
        palette("PICO-8", hex(&[
            0x000000, 0x1D2B53, 0x7E2553, 0x008751, 0xAB5236, 0x5F574F, 0xC2C3C7, 0xFFF1E8,
            0xFF004D, 0xFFA300, 0xFFEC27, 0x00E436, 0x29ADFF, 0x83769C, 0xFF77A8, 0xFFCCAA,
        ])),
        palette("Game Boy", hex(&[0x0F380F, 0x306230, 0x8BAC0F, 0x9BBC0F])),
        palette("Greys", (0..16).map(|i| { let v = (i * 17) as u8; Color::new(v, v, v, 255) }).collect()),
        palette("Void", hex(&[
            0x0B0B0F, 0x121218, 0x26263A, 0x3A2668, 0x7C4DFF, 0xA78BFA, 0xC4B5FD, 0xE4E4EE,
            0x78DCE6, 0xF5E6A8, 0xFBBF24, 0xF87171,
        ])),
    ]
}
