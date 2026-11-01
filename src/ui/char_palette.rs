use egui::*;
use crate::icons;
use crate::theme;
use crate::app::storage::GlyphSet;
use crate::ui::{glyph_render, widgets::section_label};

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
    ("Box",
     "─━│┃┄┅┆┇┈┉┊┋┌┍┎┏┐┑┒┓└┕┖┗┘┙┚┛├┝┞┟┠┡┢┣┤┥┦┧┨┩┪┫┬┭┮┯┰┱┲┳┴┵┶┷┸┹┺┻┼┽┾┿╀╁╂╃╄╅╆╇╈╉╊╋╌╍╎╏═║╒╓╔╕╖╗╘╙╚╛╜╝╞╟╠╡╢╣╤╥╦╧╨╩╪╫╬╭╮╯╰╱╲╳╴╵╶╷╸╹╺╻╼╽╾╿"),
    ("Braille",
     "⠀⠁⠂⠃⠄⠅⠆⠇⠈⠉⠊⠋⠌⠍⠎⠏⠐⠑⠒⠓⠔⠕⠖⠗⠘⠙⠚⠛⠜⠝⠞⠟⠠⠡⠢⠣⠤⠥⠦⠧⠨⠩⠪⠫⠬⠭⠮⠯⠰⠱⠲⠳⠴⠵⠶⠷⠸⠹⠺⠻⠼⠽⠾⠿⡀⡁⡂⡃⡄⡅⡆⡇⡈⡉⡊⡋⡌⡍⡎⡏⡐⡑⡒⡓⡔⡕⡖⡗⡘⡙⡚⡛⡜⡝⡞⡟⡠⡡⡢⡣⡤⡥⡦⡧⡨⡩⡪⡫⡬⡭⡮⡯⡰⡱⡲⡳⡴⡵⡶⡷⡸⡹⡺⡻⡼⡽⡾⡿⢀⢁⢂⢃⢄⢅⢆⢇⢈⢉⢊⢋⢌⢍⢎⢏⢐⢑⢒⢓⢔⢕⢖⢗⢘⢙⢚⢛⢜⢝⢞⢟⢠⢡⢢⢣⢤⢥⢦⢧⢨⢩⢪⢫⢬⢭⢮⢯⢰⢱⢲⢳⢴⢵⢶⢷⢸⢹⢺⢻⢼⢽⢾⢿⣀⣁⣂⣃⣄⣅⣆⣇⣈⣉⣊⣋⣌⣍⣎⣏⣐⣑⣒⣓⣔⣕⣖⣗⣘⣙⣚⣛⣜⣝⣞⣟⣠⣡⣢⣣⣤⣥⣦⣧⣨⣩⣪⣫⣬⣭⣮⣯⣰⣱⣲⣳⣴⣵⣶⣷⣸⣹⣺⣻⣼⣽⣾⣿"),
    ("Sextants",
     "🬀🬁🬂🬃🬄🬅🬆🬇🬈🬉🬊🬋🬌🬍🬎🬏🬐🬑🬒🬓🬔🬕🬖🬗🬘🬙🬚🬛🬜🬝🬞🬟🬠🬡🬢🬣🬤🬥🬦🬧🬨🬩🬪🬫🬬🬭🬮🬯🬰🬱🬲🬳🬴🬵🬶🬷🬸🬹🬺🬻"),
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

#[derive(Default)]
pub struct CharPalette {
    /// A built-in set (below `CATEGORIES.len()`) or one of the user's, after them.
    pub category: usize,
    pub search: String,
    /// Recently used glyphs, newest first.
    pub recent: Vec<char>,
    /// Glyphs typed or pasted to add to the user's set shown.
    adding: String,
    /// Clicking a glyph of the user's set removes it.
    pub editing: bool,
    rename: String,
}

/// What the user did to their own glyph sets (indices into them).
#[derive(Clone, Debug, PartialEq)]
pub enum SetAction {
    New,
    Rename(usize, String),
    Delete(usize),
    /// Adds the glyphs of the text that aren't in the set yet.
    Add(usize, String),
    Remove(usize, char),
    /// Adds the glyphs used in the selection (or the active layer).
    FromSelection(usize),
    /// Copies the set's glyphs as text.
    Copy(usize),
    /// Open the Library on glyph sets.
    Manage,
}

impl CharPalette {
    pub fn remember(&mut self, ch: char) {
        if ch == ' ' { return; }
        self.recent.retain(|&c| c != ch);
        self.recent.insert(0, ch);
        self.recent.truncate(16);
    }

    /// Left click picks the primary glyph, right click the secondary one.
    pub fn show(&mut self, ui: &mut Ui, primary_char: &mut char, secondary_char: &mut char, sets: &[GlyphSet]) -> Option<SetAction> {
        let mut action = None;
        let builtin = CATEGORIES.len();
        if self.category >= builtin + sets.len() {
            self.category = 0;
        }
        let own = self.category.checked_sub(builtin).filter(|&i| i < sets.len());
        let set_name = |i: usize| match i.checked_sub(builtin) {
            Some(own) => sets[own].name.clone(),
            None => CATEGORIES[i].0.to_owned(),
        };
        ui.add(
            TextEdit::singleline(&mut self.search)
                .hint_text(format!("{}  Search: arrow, 2588, or type a glyph", icons::SEARCH))
                .desired_width(f32::INFINITY),
        );

        let searching = !self.search.trim().is_empty();
        if !searching {
            ComboBox::from_id_salt("glyph_category")
                .width(ui.available_width())
                .selected_text(set_name(self.category))
                .show_ui(ui, |ui| {
                    for (i, (name, chars)) in CATEGORIES.iter().enumerate() {
                        let sample: String = chars.chars().take(6).collect();
                        ui.selectable_value(&mut self.category, i, format!("{name}   {sample}"));
                    }
                    ui.separator();
                    for (i, set) in sets.iter().enumerate() {
                        let sample: String = set.glyphs.chars().take(6).collect();
                        ui.selectable_value(&mut self.category, builtin + i, format!("{}   {sample}", set.name));
                    }
                    if ui.selectable_label(false, format!("{}  New Set…", icons::PLUS)).clicked() {
                        action = Some(SetAction::New);
                    }
                    if ui.selectable_label(false, "Manage…").clicked() {
                        action = Some(SetAction::Manage);
                    }
                });
            if let Some(i) = own {
                action = self.own_set_tools(ui, i, &sets[i], *primary_char).or(action);
            }
        }

        const CELL: f32 = 30.0;
        const GAP: f32 = 4.0;
        let cols = (((ui.available_width() + GAP) / (CELL + GAP)) as usize).max(1);
        let removing = own.filter(|_| self.editing && !searching);
        let mut removed = None;
        let mut tiles = |ui: &mut Ui, chars: &[char], editable: bool| {
            for row in chars.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                    for &ch in row {
                        let resp = glyph_cell(ui, ch, CELL, ch == *primary_char, ch == *secondary_char);
                        if editable && removing.is_some() {
                            // Edit mode: a cross over each glyph, a click takes it out.
                            ui.painter().text(resp.rect.right_top() + vec2(-4.0, 3.0), Align2::RIGHT_TOP, icons::CLOSE, theme::mono(10.0), theme::WARNING);
                            if resp.clicked() {
                                removed = Some(ch);
                            }
                            continue;
                        }
                        if resp.clicked() {
                            *primary_char = ch;
                        }
                        if resp.secondary_clicked() {
                            *secondary_char = ch;
                        }
                    }
                });
            }
        };

        if !searching && !self.recent.is_empty() {
            ui.add_space(2.0);
            section_label(ui, "Recent");
            let recent: Vec<char> = self.recent.iter().take(cols).copied().collect();
            tiles(ui, &recent, false);
            ui.add_space(2.0);
            section_label(ui, &set_name(self.category));
        }

        let chars: Vec<char> = if searching {
            search(&self.search)
        } else {
            match own {
                Some(i) => sets[i].glyphs.chars().collect(),
                None => CATEGORIES[self.category].1.chars().collect(),
            }
        };
        if chars.is_empty() {
            let empty = if own.is_some() && !searching {
                "Empty: type or paste glyphs above, or add the primary glyph or the selection's glyphs"
            } else {
                "Nothing found"
            };
            ui.label(RichText::new(empty).color(theme::TEXT_FAINT));
            return action;
        }

        ui.add_space(4.0);
        tiles(ui, &chars, true);
        if let (Some(i), Some(ch)) = (removing, removed) {
            action = Some(SetAction::Remove(i, ch));
        }
        action
    }

    /// Adding, editing and managing the user's set `i`.
    fn own_set_tools(&mut self, ui: &mut Ui, i: usize, set: &GlyphSet, primary: char) -> Option<SetAction> {
        let mut action = None;
        ui.add_space(2.0);
        let add = ui.add(
            TextEdit::singleline(&mut self.adding)
                .hint_text(format!("{}  Type or paste glyphs, Enter adds them", icons::PLUS))
                .desired_width(f32::INFINITY),
        );
        if add.lost_focus() && ui.input(|inp| inp.key_pressed(Key::Enter)) && !self.adding.is_empty() {
            action = Some(SetAction::Add(i, std::mem::take(&mut self.adding)));
        }
        ui.horizontal(|ui| {
            if ui.button(format!("Add {primary}")).on_hover_text("Add the primary glyph").clicked() {
                action = Some(SetAction::Add(i, primary.to_string()));
            }
            if ui.button("From Selection").on_hover_text("Add every glyph used in the selection (or the active layer)").clicked() {
                action = Some(SetAction::FromSelection(i));
            }
            ui.toggle_value(&mut self.editing, "Edit").on_hover_text("Click glyphs to take them out of the set");
            ui.menu_button(icons::MORE, |ui| {
                if self.rename.is_empty() {
                    self.rename = set.name.clone();
                }
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let field = ui.add(TextEdit::singleline(&mut self.rename).desired_width(140.0));
                    if field.lost_focus() && ui.input(|inp| inp.key_pressed(Key::Enter)) {
                        action = Some(SetAction::Rename(i, std::mem::take(&mut self.rename)));
                        ui.close();
                    }
                });
                if ui.button("Copy Glyphs as Text").clicked() {
                    action = Some(SetAction::Copy(i));
                    ui.close();
                }
                if ui.button("Delete Set").clicked() {
                    action = Some(SetAction::Delete(i));
                    ui.close();
                }
            });
        });
        action
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
        // Box, block and braille glyphs as on the canvas (the font lacks some of them).
        let ppp = ui.ctx().pixels_per_point();
        let glyph = glyph_render::snap_rect(Rect::from_center_size(rect.center(), vec2(size * 0.4, size * 0.8)), ppp);
        if !glyph_render::draw(ui.painter(), ch, glyph, fg, glyph_render::Strokes::new(glyph.width(), ppp)) {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, ch, theme::mono(17.0), fg);
        }
    }
    resp
}
