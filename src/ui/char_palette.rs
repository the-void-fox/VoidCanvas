use egui::*;

pub const CATEGORIES: &[(&str, &str)] = &[
    ("Basic",
     " !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
    ("Letters",
     "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"),
    ("Numbers",
     "0123456789"),
    ("Lines",
     "─│┌┐└┘├┤┬┴┼━┃┏┓┗┛┣┫┳┻╋═║╔╗╚╝╠╣╦╩╬╒╕╖╗╘╙╚╛╜╝╞╟╠╡╢╣╤╥╦╧╨╩╪╫╬╭╮╯╰"),
    ("Blocks",
     "█▉▊▋▌▍▎▏▀▁▂▃▄▅▆▇▐░▒▓■□▪▫▬▭▮▯"),
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
     "$¢£¤¥€₠₡₢₣₤₥₦₧₨₩₪₫€₭₮₯₰₱₲₳₴₵₶₷₸₹₺₻₼₽₾"),
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

            let chars: Vec<char> = if self.search.is_empty() {
                CATEGORIES[self.category].1.chars().collect()
            } else {
                CATEGORIES.iter()
                    .flat_map(|(_, chars)| chars.chars())
                    .filter(|c| c.to_string().contains(&self.search))
                    .take(64)
                    .collect()
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