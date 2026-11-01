//! Scripts in Rhai (https://rhai.rs) that draw on the active layer.
//!
//! A script works on a copy of the document; afterwards the differences become one undo step.

use std::cell::RefCell;
use std::rc::Rc;

use rhai::{Dynamic, Engine, FLOAT, INT, Map};

use crate::document::{Cell, Changes, Color, Document};
use crate::history::Command;
use crate::tools::{bresenham, gradient::mix};
use crate::ui::widgets::parse_hex;

/// What a script may read about the editor besides the document.
#[derive(Clone)]
pub struct ScriptContext {
    pub layer: usize,
    pub primary_char: char,
    pub primary_fg: Color,
    pub secondary_fg: Color,
    /// The selection (or the whole canvas): `x0..x1`, `y0..y1`.
    pub area: (usize, usize, usize, usize),
}

pub struct ScriptOutput {
    pub doc: Document,
    /// What the script printed.
    pub log: Vec<String>,
}

/// Steps a script may take before it is stopped (endless loops).
const MAX_OPERATIONS: u64 = 50_000_000;

/// The API, in the order the Script window lists it.
pub const REFERENCE: &[(&str, &str)] = &[
    ("width()  height()", "canvas size"),
    ("area()", "#{x, y, w, h} of the selection, or the canvas"),
    ("get(x, y)", "the glyph at (x, y), as a one-letter string"),
    ("set(x, y, glyph [, fg [, bg]])", "write a cell; colours like \"#ff8800\" or \"none\""),
    ("set_fg(x, y, c)  set_bg(x, y, c)", "change only a colour"),
    ("erase(x, y)", "empty a cell"),
    ("text(x, y, str [, fg])", "write text; \\n starts a new line"),
    ("line(x0, y0, x1, y1, glyph [, fg])", "a straight line"),
    ("fill(glyph [, fg [, bg]])  clear()", "the whole area"),
    ("rgb(r, g, b)  hsv(h, s, v)  mix(a, b, t)", "colours (h in degrees, s v t in 0..1)"),
    ("primary()  secondary()  glyph()", "the colour slots and the primary glyph"),
    ("rand()  rand(a, b)  seed(n)", "random 0..1, or a whole number a..=b"),
    ("noise(x, y)", "smooth noise in 0..1"),
    ("frame()  frames()  go_to(n)", "animation frames (from 0)"),
    ("add_frame()", "a copy of the frame shown, at the end; shows it"),
    ("set_duration(ms)", "how long the frame shown lasts"),
    ("print(x)", "write to the output below"),
];

/// Ready-made scripts for the Examples menu.
pub const EXAMPLES: &[(&str, &str)] = &[
    ("Starfield", r##"// Scatters stars over the empty cells of the selection (or the whole layer).
let a = area();
for i in 0..(a.w * a.h / 30) {
    let x = rand(a.x, a.x + a.w - 1);
    let y = rand(a.y, a.y + a.h - 1);
    if get(x, y) == " " {
        let star = [".", "·", "*", "+", "°"][rand(0, 4)];
        set(x, y, star, hsv(rand(190, 290), 0.35, 0.5 + rand() * 0.5));
    }
}
"##),
    ("Plasma", r##"// A colourful background made of sine waves.
let a = area();
for y in a.y..a.y + a.h {
    for x in a.x..a.x + a.w {
        let v = sin(x / 7.0) + sin(y / 3.5) + sin((x + y * 2) / 11.0);
        set_bg(x, y, hsv(250.0 + v * 35.0, 0.65, 0.22 + v * 0.06));
    }
}
"##),
    ("Mountains", r##"// Layered hills from noise, darker towards the front.
let a = area();
for layer in 0..3 {
    let colour = mix("#3a2668", "#a78bfa", 1.0 - layer / 2.0);
    for x in a.x..a.x + a.w {
        let top = a.h * (0.35 + layer * 0.18) - noise(x / 9.0, layer * 10.0) * a.h * 0.3;
        for y in a.y + top.to_int()..a.y + a.h {
            set(x, y, "█", colour);
        }
    }
}
"##),
    ("Rainbow text", r##"// Writes the text with a colour per letter.
let message = "Void Canvas";
let a = area();
let x0 = a.x + (a.w - message.len()) / 2;
let y = a.y + a.h / 2;
for i in 0..message.len() {
    text(x0 + i, y, message[i], hsv(i * 360.0 / message.len(), 0.6, 1.0));
}
"##),
    ("Bouncing ball", r##"// Builds a 16-frame animation of a ball bouncing across the canvas.
let w = width();
let h = height();
let count = 16;
for f in 0..count {
    if f > 0 { add_frame(); }
    clear();
    let t = f / (count - 1.0);
    let x = (t * (w - 1)).to_int();
    let y = h - 1 - (abs(sin(t * PI() * 3.0)) * (h - 2)).to_int();
    set(x, y, "●", "#fbbf24");
    line(0, h - 1, w - 1, h - 1, "▀", "#3a2668");
    set_duration(60);
}
"##),
];

struct State {
    doc: Document,
    ctx: ScriptContext,
    rng: u64,
    noise_seed: u64,
}

impl State {
    fn put(&mut self, x: i64, y: i64, f: impl FnOnce(&Cell) -> Cell) {
        if x < 0 || y < 0 { return; }
        let layer = &mut self.doc.layers[self.ctx.layer];
        if let Some(old) = layer.get(x as usize, y as usize) {
            let new = f(old);
            layer.put(x as usize, y as usize, new);
        }
    }

    fn random(&mut self) -> f64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn number(d: &Dynamic) -> Option<f64> {
    d.as_int().map(|i| i as f64).or_else(|_| d.as_float()).ok()
}

fn int(d: &Dynamic) -> i64 {
    number(d).map_or(i64::MIN, |f| f.floor() as i64)
}

fn glyph(d: &Dynamic) -> char {
    d.as_char().ok().or_else(|| d.clone().into_string().ok()?.chars().next()).unwrap_or(' ')
}

fn color(d: &Dynamic) -> Option<Color> {
    let text = d.clone().into_string().ok()?;
    if text.eq_ignore_ascii_case("none") || text.is_empty() {
        return Some(Color::TRANSPARENT);
    }
    parse_hex(&text).map(Color::from_egui)
}

fn hex(c: &Color) -> String {
    if c.a == 255 { format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b) } else { format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a) }
}

fn hsv(h: f64, s: f64, v: f64) -> Color {
    let h = h.rem_euclid(360.0) / 60.0;
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |f: f64| ((f + v - c) * 255.0).round() as u8;
    Color::new(byte(r), byte(g), byte(b), 255)
}

/// Smooth value noise in 0..1.
fn noise(x: f64, y: f64, seed: u64) -> f64 {
    let hash = |x: i64, y: i64| {
        let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed;
        h ^= h >> 33;
        h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        h ^= h >> 33;
        (h >> 11) as f64 / (1u64 << 53) as f64
    };
    let (x0, y0) = (x.floor(), y.floor());
    let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
    let (tx, ty) = (smooth(x - x0), smooth(y - y0));
    let (ix, iy) = (x0 as i64, y0 as i64);
    let top = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * tx;
    let bottom = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * tx;
    top + (bottom - top) * ty
}

fn engine(state: &Rc<RefCell<State>>, log: &Rc<RefCell<Vec<String>>>) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(MAX_OPERATIONS);
    let out = log.clone();
    engine.on_print(move |text| out.borrow_mut().push(text.to_owned()));
    let out = log.clone();
    engine.on_debug(move |text, _, _| out.borrow_mut().push(text.to_owned()));

    macro_rules! api {
        ($name:literal, |$s:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let state = state.clone();
            engine.register_fn($name, move |$($arg: $ty),*| {
                #[allow(unused_mut)]
                let mut $s = state.borrow_mut();
                $body
            });
        }};
    }

    api!("width", |s| s.doc.width as INT);
    api!("height", |s| s.doc.height as INT);
    api!("area", |s| {
        let (x0, y0, x1, y1) = s.ctx.area;
        let mut map = Map::new();
        for (key, value) in [("x", x0), ("y", y0), ("w", x1 - x0), ("h", y1 - y0)] {
            map.insert(key.into(), Dynamic::from(value as INT));
        }
        map
    });
    api!("get", |s, x: Dynamic, y: Dynamic| {
        let (x, y) = (int(&x), int(&y));
        let cell = (x >= 0 && y >= 0).then(|| s.doc.layers[s.ctx.layer].get(x as usize, y as usize)).flatten();
        // A one-letter string compares naturally: `get(x, y) == "#"`.
        cell.map_or(' ', |c| c.ch).to_string()
    });
    api!("set", |s, x: Dynamic, y: Dynamic, g: Dynamic| {
        let ch = glyph(&g);
        let fg = s.ctx.primary_fg.clone();
        s.put(int(&x), int(&y), |old| Cell { ch, fg, bg: old.bg.clone() });
    });
    api!("set", |s, x: Dynamic, y: Dynamic, g: Dynamic, fg: Dynamic| {
        let (ch, fg) = (glyph(&g), color(&fg));
        s.put(int(&x), int(&y), |old| Cell { ch, fg: fg.unwrap_or_else(|| old.fg.clone()), bg: old.bg.clone() });
    });
    api!("set", |s, x: Dynamic, y: Dynamic, g: Dynamic, fg: Dynamic, bg: Dynamic| {
        let (ch, fg, bg) = (glyph(&g), color(&fg), color(&bg));
        s.put(int(&x), int(&y), |old| Cell {
            ch,
            fg: fg.unwrap_or_else(|| old.fg.clone()),
            bg: bg.unwrap_or_else(|| old.bg.clone()),
        });
    });
    api!("set_fg", |s, x: Dynamic, y: Dynamic, c: Dynamic| {
        if let Some(fg) = color(&c) {
            s.put(int(&x), int(&y), |old| Cell { fg, ..old.clone() });
        }
    });
    api!("set_bg", |s, x: Dynamic, y: Dynamic, c: Dynamic| {
        if let Some(bg) = color(&c) {
            s.put(int(&x), int(&y), |old| Cell { bg, ..old.clone() });
        }
    });
    api!("erase", |s, x: Dynamic, y: Dynamic| s.put(int(&x), int(&y), |_| Cell::empty()));
    let text = |s: &mut State, x: i64, y: i64, text: &str, fg: Option<Color>| {
        let (mut cx, mut cy) = (x, y);
        for ch in text.chars() {
            if ch == '\n' {
                (cx, cy) = (x, cy + 1);
                continue;
            }
            let fg = fg.clone().unwrap_or_else(|| s.ctx.primary_fg.clone());
            s.put(cx, cy, |old| Cell { ch, fg, bg: old.bg.clone() });
            cx += 1;
        }
    };
    {
        let state = state.clone();
        engine.register_fn("text", move |x: Dynamic, y: Dynamic, t: Dynamic| {
            text(&mut state.borrow_mut(), int(&x), int(&y), &t.to_string(), None);
        });
    }
    {
        let state = state.clone();
        engine.register_fn("text", move |x: Dynamic, y: Dynamic, t: Dynamic, fg: Dynamic| {
            text(&mut state.borrow_mut(), int(&x), int(&y), &t.to_string(), color(&fg));
        });
    }
    let line = |s: &mut State, from: (i64, i64), to: (i64, i64), ch: char, fg: Option<Color>| {
        let fg = fg.unwrap_or_else(|| s.ctx.primary_fg.clone());
        let clamp = |v: i64| v.clamp(-1, 100_000) as isize;
        for (x, y) in bresenham(clamp(from.0), clamp(from.1), clamp(to.0), clamp(to.1)) {
            s.put(x as i64, y as i64, |old| Cell { ch, fg: fg.clone(), bg: old.bg.clone() });
        }
    };
    {
        let state = state.clone();
        engine.register_fn("line", move |x0: Dynamic, y0: Dynamic, x1: Dynamic, y1: Dynamic, g: Dynamic| {
            line(&mut state.borrow_mut(), (int(&x0), int(&y0)), (int(&x1), int(&y1)), glyph(&g), None);
        });
    }
    {
        let state = state.clone();
        engine.register_fn("line", move |x0: Dynamic, y0: Dynamic, x1: Dynamic, y1: Dynamic, g: Dynamic, fg: Dynamic| {
            line(&mut state.borrow_mut(), (int(&x0), int(&y0)), (int(&x1), int(&y1)), glyph(&g), color(&fg));
        });
    }
    let fill = |s: &mut State, cell: &dyn Fn(&Cell) -> Cell| {
        let (x0, y0, x1, y1) = s.ctx.area;
        for y in y0..y1 {
            for x in x0..x1 {
                s.put(x as i64, y as i64, cell);
            }
        }
    };
    {
        let state = state.clone();
        engine.register_fn("clear", move || fill(&mut state.borrow_mut(), &|_| Cell::empty()));
    }
    {
        let state = state.clone();
        engine.register_fn("fill", move |g: Dynamic| {
            let mut s = state.borrow_mut();
            let (ch, fg) = (glyph(&g), s.ctx.primary_fg.clone());
            fill(&mut s, &|old| Cell { ch, fg: fg.clone(), bg: old.bg.clone() });
        });
    }
    {
        let state = state.clone();
        engine.register_fn("fill", move |g: Dynamic, fg: Dynamic| {
            let (ch, fg) = (glyph(&g), color(&fg));
            fill(&mut state.borrow_mut(), &|old| Cell { ch, fg: fg.clone().unwrap_or_else(|| old.fg.clone()), bg: old.bg.clone() });
        });
    }
    {
        let state = state.clone();
        engine.register_fn("fill", move |g: Dynamic, fg: Dynamic, bg: Dynamic| {
            let (ch, fg, bg) = (glyph(&g), color(&fg), color(&bg));
            fill(&mut state.borrow_mut(), &|old| Cell {
                ch,
                fg: fg.clone().unwrap_or_else(|| old.fg.clone()),
                bg: bg.clone().unwrap_or_else(|| old.bg.clone()),
            });
        });
    }

    engine.register_fn("rgb", |r: INT, g: INT, b: INT| hex(&Color::new(r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8, 255)));
    engine.register_fn("hsv", |h: Dynamic, s: Dynamic, v: Dynamic| {
        hex(&hsv(number(&h).unwrap_or(0.0), number(&s).unwrap_or(0.0), number(&v).unwrap_or(0.0)))
    });
    engine.register_fn("mix", |a: Dynamic, b: Dynamic, t: Dynamic| {
        let (a, b) = (color(&a).unwrap_or(Color::TRANSPARENT), color(&b).unwrap_or(Color::TRANSPARENT));
        hex(&mix(&a, &b, number(&t).unwrap_or(0.0) as f32))
    });
    api!("primary", |s| hex(&s.ctx.primary_fg));
    api!("secondary", |s| hex(&s.ctx.secondary_fg));
    api!("glyph", |s| s.ctx.primary_char.to_string());

    api!("rand", |s| s.random() as FLOAT);
    api!("rand", |s, a: Dynamic, b: Dynamic| {
        let (a, b) = (int(&a), int(&b));
        let (lo, hi) = (a.min(b), a.max(b));
        lo + (s.random() * (hi - lo + 1) as f64).floor() as INT
    });
    api!("seed", |s, n: INT| {
        s.rng = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        s.noise_seed = n as u64;
    });
    api!("noise", |s, x: Dynamic, y: Dynamic| noise(number(&x).unwrap_or(0.0), number(&y).unwrap_or(0.0), s.noise_seed) as FLOAT);

    api!("frame", |s| s.doc.frame as INT);
    api!("frames", |s| s.doc.frame_count() as INT);
    api!("go_to", |s, n: INT| {
        let n = n.clamp(0, s.doc.frame_count() as INT - 1) as usize;
        s.doc.show_frame(n);
    });
    api!("add_frame", |s| {
        let data = s.doc.frame_data(s.doc.frame);
        let index = s.doc.frame_count();
        s.doc.insert_frame(index, data);
        index as INT
    });
    api!("set_duration", |s, ms: INT| {
        let frame = s.doc.frame;
        s.doc.durations[frame] = ms.clamp(10, 10_000) as u32;
    });
    engine
}

/// Runs `code` on a copy of `doc`. An error comes with the output printed before it.
pub fn run(code: &str, doc: &Document, ctx: ScriptContext) -> Result<ScriptOutput, (String, Vec<String>)> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        | 1;
    let state = Rc::new(RefCell::new(State { doc: doc.clone(), ctx, rng: seed, noise_seed: 0 }));
    let log = Rc::new(RefCell::new(Vec::new()));
    let result = engine(&state, &log).run(code);
    let log = log.take();
    match result {
        Ok(()) => {
            let doc = Rc::try_unwrap(state).ok().expect("the engine is gone").into_inner().doc;
            Ok(ScriptOutput { doc, log })
        }
        Err(mut e) => {
            // Shown as "Line 3: …" rather than Rhai's "… (line 3, position 7)".
            let message = match e.take_position().line() {
                Some(line) => format!("Line {line}: {e}"),
                None => e.to_string(),
            };
            Err((message, log))
        }
    }
}

/// The commands that turn `before` into `after` (the result of a script on `layer`),
/// or `None` if the script changed nothing.
pub fn changes(before: &Document, after: &Document, layer: usize) -> Option<Command> {
    let mut cmds = Vec::new();
    let kept = before.frame_count();
    for frame in 0..kept {
        let (old, new) = (before.layers[layer].cells_in(frame), after.layers[layer].cells_in(frame));
        let changes: Changes = old
            .iter()
            .zip(new)
            .enumerate()
            .filter(|(_, (o, n))| o != n)
            .map(|(i, (o, n))| ((i % before.width, i / before.width), (o.clone(), n.clone())))
            .collect();
        if !changes.is_empty() {
            cmds.push(Command::Draw { layer_idx: layer, frame, changes });
        }
    }
    for index in kept..after.frame_count() {
        cmds.push(Command::FrameAdd { index, data: after.frame_data(index) });
    }
    let mut old_timing = before.durations.clone();
    old_timing.extend_from_slice(&after.durations[kept..]);
    if old_timing != after.durations {
        cmds.push(Command::FrameTiming { old: old_timing, new: after.durations.clone() });
    }
    (!cmds.is_empty()).then_some(Command::Group(cmds))
}
