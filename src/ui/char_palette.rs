use egui::*;

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
    pub fn show(&mut self, ui: &mut Ui, primary_char: &mut char, secondary_char: &mut char) {
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, (name, _)) in CATEGORIES.iter().enumerate() {
                    let mut btn = Button::new(*name);
                    if i == self.category {
                        btn = btn.fill(ui.visuals().widgets.active.bg_fill);
                    }
                    if ui.add(btn).clicked() {
                        self.category = i;
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.text_edit_singleline(&mut self.search);
            });

            ui.separator();

            let chars: Vec<char> = if self.search.trim().is_empty() {
                CATEGORIES[self.category].1.chars().collect()
            } else {
                search(&self.search)
            };

            let btn_size = Vec2::splat(36.0);
            let available_w = ui.available_width();
            let cols = ((available_w / (btn_size.x + 4.0)) as usize).max(1);

            egui::Grid::new("char_grid")
                .spacing([2.0, 2.0])
                .show(ui, |ui| {
                    for (i, &ch) in chars.iter().enumerate() {
                        if i > 0 && i % cols == 0 {
                            ui.end_row();
                        }

                        let text = RichText::new(ch.to_string())
                            .size(18.0)
                            .monospace();

                        let mut btn = Button::new(text)
                            .min_size(btn_size)
                            .fill(Color32::from_gray(60));

                        if ch == *primary_char {
                            btn = btn.fill(Color32::from_rgb(0, 100, 200));
                        } else if ch == *secondary_char {
                            btn = btn.fill(Color32::from_rgb(80, 80, 80));
                        }

                        let resp = ui.add(btn);

                        if resp.clicked() {
                            *primary_char = ch;
                        }
                        if resp.secondary_clicked() {
                            *secondary_char = ch;
                        }
                        if resp.hovered() {
                            resp.on_hover_text(format!(
                                "Left click: primary, Right click: secondary\nU+{:04X}",
                                ch as u32
                            ));
                        }
                    }
                });
        });
    }
}