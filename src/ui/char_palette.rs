use egui::*;
use crate::icons;
use crate::theme;

pub const CATEGORIES: &[(&str, &str)] = &[
    ("Basic",
     " !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
    ("Letters",
     "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"),
    ("Numbers",
     "0123456789"),
    ("Lines",
     "─│┌┐└┘├┤┬┴┼━┃┏┓┗┛┣┫┳┻╋═║╔╗╚╝╠╣╦╩╬╒╓╕╖╘╙╛╜╞╟╡╢╤╥╧╨╪╫╭╮╯╰╌╍╎╏┄┅┆┇┈┉┊┋╱╲╳"),
    ("Blocks",
     "█▉▊▋▌▍▎▏▀▁▂▃▄▅▆▇▐░▒▓▖▗▘▙▚▛▜▝▞▟■□▪▫▬▭▮▯"),
    ("Geometry",
     "●○◐◑◒◓◔◕◖◗◘◙◚◛◜◝◞◟◠◡◢◣◤◥◦◧◨◩◪◫◬◭◮◯★☆✪⦿"),
    ("Arrows",
     "←↑→↓↔↕↖↗↘↙⇐⇑⇒⇓⇔⇕⇖⇗⇘⇙↩↪↵↺↻➔➜➝➞➟➠➡➢➣➤"),
    ("Dots",
     "·•§˙…∙⋮⋯"),
    ("Math",
     "±×÷=≠≈≡≢≤≥∞∑∏√∫∅∈∉∋⊂⊃⊆⊇∩∪¬∧∨⊕⊖⊗⊘⊙∀∃∂∇¼½¾⅓⅔⅕⅖⅗⅘⅙⅚⅛⅜⅝⅞⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎"),
    ("Brackets",
     "()[]{}⟨⟩⎛⎜⎝⎞⎟⎠⎡⎢⎣⎤⎥⎦⎧⎨⎩⎪⎫⎬⎭❨❩❪❫❬❭❮❯❰❱"),
    ("Greek",
     "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩαβγδεζηθικλμνξοπρστυφχψω"),
    ("Symbols",
     "☀☁☂☃★☆☉☐☑☒☺☻☹☠☢☣☭☮☯♠♡♢♣♤♥♦♧♨♪♫♩♬♀♂⚤⚥⚧⚨⚩⚫⚪⚰⚱⚲⚠⚡⌂⌐⌘⌥⌫⌦⌧⌨⏎⏏©®™"),
    ("Currency",
     "$¢£¤¥€₠₡₢₣₤₥₦₧₨₩₪₫₭₮₯₰₱₲₳₴₵₶₷₸₹₺₻₼₽₾"),
    ("Progress",
     "█▇▆▅▄▃▂▁▉▊▋▌▍▎▏░▒▓"),
    ("Nerd Dev",
     "\u{e600}\u{e601}\u{e602}\u{e603}\u{e604}\u{e605}\u{e624}\u{e718}\u{e70c}\u{e73c}\u{e7a8}\u{f013}\u{f015}\u{f017}\u{f023}\u{f025}\u{f02d}\u{f03d}\u{f040}\u{f044}\u{f071}\u{f07c}\u{f085}\u{f09b}\u{f113}\u{f11c}\u{f120}\u{f121}\u{f126}\u{f1d3}\u{f27d}\u{f296}\u{f308}\u{f31b}"),
    ("Nerd Files",
     "\u{f016}\u{f022}\u{f040}\u{f07b}\u{f07c}\u{f08e}\u{f0c5}\u{f0c7}\u{f0c8}\u{f0e7}\u{f0f4}\u{f115}\u{f1c0}\u{f1c1}\u{f1c2}\u{f1c3}\u{f1c4}\u{f1c5}\u{f1c6}\u{f1c7}\u{f1c8}\u{f1c9}\u{f15b}\u{f15c}"),
    ("Nerd Arrows",
     "\u{f04b}\u{f04c}\u{f04d}\u{f04e}\u{f050}\u{f051}\u{f053}\u{f054}\u{f060}\u{f061}\u{f062}\u{f063}\u{f064}\u{f0a4}\u{f0a5}\u{f0a6}\u{f0a7}\u{f0d7}\u{f0d8}\u{f0d9}\u{f0da}\u{f100}\u{f101}\u{f102}\u{f103}"),
    ("Nerd UI",
     "\u{f00c}\u{f00d}\u{f011}\u{f012}\u{f014}\u{f021}\u{f024}\u{f026}\u{f027}\u{f028}\u{f02e}\u{f030}\u{f031}\u{f032}\u{f033}\u{f06e}\u{f070}\u{f0c9}\u{f0e0}\u{f0eb}\u{f0f3}\u{f119}\u{f11a}\u{f155}"),
];

/// Characters for a search query: a code point (`2588`, `U+2588`, `0x2588`),
/// a category name (`arrow`), or the characters themselves (`─│`).
/// Typed characters that are in no palette are offered too.
pub fn search(query: &str) -> Vec<char> {
    const MAX_RESULTS: usize = 256;
    let query = query.trim();
    let lower = query.to_lowercase();

    let mut found: Vec<char> = Vec::new();
    let mut push = |c: char| {
        if !found.contains(&c) && found.len() < MAX_RESULTS { found.push(c); }
    };

    if let Some(c) = parse_code_point(&lower) { push(c); }

    let mut category_hit = false;
    for (name, chars) in CATEGORIES {
        let by_name = lower.chars().count() > 1 && name.to_lowercase().contains(&lower);
        category_hit |= by_name;
        for c in chars.chars() {
            if by_name || query.contains(c) { push(c); }
        }
    }

    if !category_hit {
        query.chars().filter(|c| !c.is_whitespace() && !c.is_control()).for_each(&mut push);
    }
    found
}

fn parse_code_point(query: &str) -> Option<char> {
    let explicit = ["u+", "0x", "\\u", "u"].iter().find_map(|p| query.strip_prefix(p));
    let hex = match explicit {
        Some(rest) => rest,
        // Bare hex only if it looks like a code point, so words like "face" stay words.
        None if (4..=6).contains(&query.len()) && query.chars().any(|c| c.is_ascii_digit()) => query,
        None => return None,
    };
    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
}

pub struct CharPalette {
    pub category: usize,
    pub search: String,
}

impl Default for CharPalette {
    fn default() -> Self {
        Self { category: 0, search: String::new() }
    }
}

impl CharPalette {
    /// Left click picks the primary glyph, right click the secondary one.
    pub fn show(&mut self, ui: &mut Ui, primary_char: &mut char, secondary_char: &mut char) {
        ui.add(
            TextEdit::singleline(&mut self.search)
                .hint_text(format!("{}  Search: arrow, 2588, or type a glyph", icons::SEARCH))
                .desired_width(f32::INFINITY),
        );

        let searching = !self.search.trim().is_empty();
        if !searching {
            ComboBox::from_id_salt("glyph_category")
                .width(ui.available_width())
                .selected_text(CATEGORIES[self.category].0)
                .show_ui(ui, |ui| {
                    for (i, (name, chars)) in CATEGORIES.iter().enumerate() {
                        let sample: String = chars.chars().take(6).collect();
                        ui.selectable_value(&mut self.category, i, format!("{name}   {sample}"));
                    }
                });
        }

        let chars: Vec<char> = if searching {
            search(&self.search)
        } else {
            CATEGORIES[self.category].1.chars().collect()
        };
        if chars.is_empty() {
            ui.label(RichText::new("Nothing found").color(theme::TEXT_FAINT));
            return;
        }

        ui.add_space(4.0);
        const CELL: f32 = 30.0;
        const GAP: f32 = 4.0;
        let cols = (((ui.available_width() + GAP) / (CELL + GAP)) as usize).max(1);
        for row in chars.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                for &ch in row {
                    let resp = glyph_cell(ui, ch, CELL, ch == *primary_char, ch == *secondary_char);
                    if resp.clicked() {
                        *primary_char = ch;
                    }
                    if resp.secondary_clicked() {
                        *secondary_char = ch;
                    }
                }
            });
        }
    }
}

/// A clickable glyph tile: filled with the accent if it's the primary glyph, ringed if secondary.
fn glyph_cell(ui: &mut Ui, ch: char, size: f32, primary: bool, secondary: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let resp = resp.on_hover_text(format!("U+{:04X}\nLeft click: primary · Right click: secondary", ch as u32));
    if ui.is_rect_visible(rect) {
        let bg = if primary {
            theme::ACCENT_STRONG
        } else if resp.hovered() {
            theme::SURFACE_HOVER
        } else {
            theme::SURFACE
        };
        ui.painter().rect_filled(rect, 5.0, bg);
        if secondary {
            ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.5, theme::ACCENT), StrokeKind::Inside);
        }
        let fg = if primary { Color32::WHITE } else { theme::TEXT };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, ch, theme::mono(17.0), fg);
    }
    resp
}
