//! What the editor knows about characters: box-drawing structure and how glyphs
//! change when a block of art is mirrored or rotated.

use crate::document::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    FlipHorizontal,
    FlipVertical,
    RotateCw,
    RotateCcw,
}

impl Transform {
    pub fn label(&self) -> &'static str {
        match self {
            Transform::FlipHorizontal => "Flip Horizontal",
            Transform::FlipVertical   => "Flip Vertical",
            Transform::RotateCw       => "Rotate Right",
            Transform::RotateCcw      => "Rotate Left",
        }
    }
}

/// The block mirrored / rotated, with every glyph turned to match.
/// Rotation swaps width and height.
pub fn transform_block(block: &[Vec<Cell>], t: Transform) -> Vec<Vec<Cell>> {
    let h = block.len();
    let w = block.first().map_or(0, |row| row.len());
    let turned = |cell: &Cell| Cell { ch: transform_glyph(cell.ch, t), ..cell.clone() };
    match t {
        Transform::FlipHorizontal => block.iter().map(|row| row.iter().rev().map(turned).collect()).collect(),
        Transform::FlipVertical => block.iter().rev().map(|row| row.iter().map(turned).collect()).collect(),
        Transform::RotateCw => (0..w).map(|r| (0..h).map(|c| turned(&block[h - 1 - c][r])).collect()).collect(),
        Transform::RotateCcw => (0..w).map(|r| (0..h).map(|c| turned(&block[c][w - 1 - r])).collect()).collect(),
    }
}

/// How `ch` looks after the transform (unchanged if nothing fits).
pub fn transform_glyph(ch: char, t: Transform) -> char {
    if let Some([up, right, down, left]) = box_arms(ch) {
        let arms = match t {
            Transform::FlipHorizontal => [up, left, down, right],
            Transform::FlipVertical   => [down, right, up, left],
            Transform::RotateCw       => [left, up, right, down],
            Transform::RotateCcw      => [right, down, left, up],
        };
        return from_arms(arms).unwrap_or(ch);
    }
    match t {
        Transform::FlipHorizontal => swap(MIRROR_H, ch),
        Transform::FlipVertical => swap(MIRROR_V, ch),
        Transform::RotateCw => step_cycle(ch, 1),
        Transform::RotateCcw => step_cycle(ch, -1),
    }
}

/// Arm weights [up, right, down, left] of a box-drawing line glyph.
pub fn box_arms(ch: char) -> Option<[u8; 4]> {
    let index = (ch as u32).checked_sub(0x2500).filter(|&i| i < 0x80)? as usize;
    Some(BOX_ARMS[index]).filter(|arms| *arms != [0; 4])
}

/// The box-drawing glyph with exactly these arms.
pub fn from_arms(arms: [u8; 4]) -> Option<char> {
    if arms == [0; 4] { return None; }
    let index = BOX_ARMS.iter().position(|a| *a == arms)?;
    char::from_u32(0x2500 + index as u32)
}

fn swap(pairs: &[(char, char)], ch: char) -> char {
    pairs
        .iter()
        .find_map(|&(a, b)| if ch == a { Some(b) } else if ch == b { Some(a) } else { None })
        .unwrap_or(ch)
}

fn step_cycle(ch: char, step: isize) -> char {
    for cycle in ROTATE_CW {
        if let Some(i) = cycle.iter().position(|&c| c == ch) {
            let n = cycle.len() as isize;
            return cycle[(i as isize + step).rem_euclid(n) as usize];
        }
    }
    ch
}

/// Glyph pairs that swap in a left ↔ right mirror (box lines are handled by their arms).
const MIRROR_H: &[(char, char)] = &[
    ('(', ')'), ('[', ']'), ('{', '}'), ('<', '>'), ('/', '\\'),
    ('«', '»'), ('‹', '›'), ('⟨', '⟩'), ('⌈', '⌉'), ('⌊', '⌋'), ('¬', '⌐'),
    ('⎛', '⎞'), ('⎜', '⎟'), ('⎝', '⎠'), ('⎡', '⎤'), ('⎢', '⎥'), ('⎣', '⎦'),
    ('⎧', '⎫'), ('⎨', '⎬'), ('⎩', '⎭'),
    ('❨', '❩'), ('❪', '❫'), ('❬', '❭'), ('❮', '❯'), ('❰', '❱'),
    ('←', '→'), ('↖', '↗'), ('↙', '↘'), ('⇐', '⇒'), ('⇖', '⇗'), ('⇙', '⇘'), ('↩', '↪'), ('↺', '↻'),
    ('◀', '▶'), ('◄', '►'), ('◁', '▷'), ('◢', '◣'), ('◤', '◥'), ('◐', '◑'), ('◖', '◗'), ('◧', '◨'),
    ('▌', '▐'), ('▏', '▕'), ('▖', '▗'), ('▘', '▝'), ('▙', '▟'), ('▛', '▜'), ('▚', '▞'),
    ('╱', '╲'), ('╭', '╮'), ('╰', '╯'),
    ('⊂', '⊃'), ('⊆', '⊇'), ('∈', '∋'), ('≤', '≥'),
];

/// Glyph pairs that swap in a top ↔ bottom mirror.
const MIRROR_V: &[(char, char)] = &[
    ('/', '\\'), ('^', 'v'), ('.', '\''), (',', '`'), ('_', '‾'),
    ('↑', '↓'), ('↖', '↙'), ('↗', '↘'), ('⇑', '⇓'), ('⇖', '⇙'), ('⇗', '⇘'),
    ('▲', '▼'), ('△', '▽'), ('◢', '◥'), ('◣', '◤'), ('◒', '◓'), ('◠', '◡'),
    ('▀', '▄'), ('▔', '▁'), ('▘', '▖'), ('▝', '▗'), ('▛', '▙'), ('▜', '▟'), ('▚', '▞'),
    ('╱', '╲'), ('╭', '╰'), ('╮', '╯'),
    ('∩', '∪'), ('⌈', '⌊'), ('⌉', '⌋'), ('⊓', '⊔'), ('∧', '∨'),
    ('⎛', '⎝'), ('⎞', '⎠'), ('⎡', '⎣'), ('⎤', '⎦'), ('⎧', '⎩'), ('⎫', '⎭'),
];

/// Glyphs that turn into the next one of their cycle when rotated a quarter turn clockwise.
const ROTATE_CW: &[&[char]] = &[
    &['-', '|'],
    &['/', '\\'],
    &['<', '^', '>', 'v'],
    &['←', '↑', '→', '↓'],
    &['↖', '↗', '↘', '↙'],
    &['⇐', '⇑', '⇒', '⇓'],
    &['⇖', '⇗', '⇘', '⇙'],
    &['◀', '▲', '▶', '▼'],
    &['◁', '△', '▷', '▽'],
    &['◢', '◣', '◤', '◥'],
    &['▌', '▀', '▐', '▄'],
    &['▘', '▝', '▗', '▖'],
    &['▛', '▜', '▟', '▙'],
    &['▚', '▞'],
    &['╭', '╮', '╯', '╰'],
    &['╱', '╲'],
    &['┄', '┆'], &['┅', '┇'], &['┈', '┊'], &['┉', '┋'], &['╌', '╎'], &['╍', '╏'],
];

/// Arm weights of U+2500–U+257F as [up, right, down, left]:
/// 0 none, 1 light, 2 heavy, 3 double. All-zero rows are dashes, arcs and diagonals.
/// Generated from the Unicode character names.
#[rustfmt::skip]
pub(crate) const BOX_ARMS: [[u8; 4]; 128] = [
    [0, 1, 0, 1], // 2500 ─ light horizontal
    [0, 2, 0, 2], // 2501 ━ heavy horizontal
    [1, 0, 1, 0], // 2502 │ light vertical
    [2, 0, 2, 0], // 2503 ┃ heavy vertical
    [0, 0, 0, 0], // 2504 ┄ light triple dash horizontal
    [0, 0, 0, 0], // 2505 ┅ heavy triple dash horizontal
    [0, 0, 0, 0], // 2506 ┆ light triple dash vertical
    [0, 0, 0, 0], // 2507 ┇ heavy triple dash vertical
    [0, 0, 0, 0], // 2508 ┈ light quadruple dash horizontal
    [0, 0, 0, 0], // 2509 ┉ heavy quadruple dash horizontal
    [0, 0, 0, 0], // 250A ┊ light quadruple dash vertical
    [0, 0, 0, 0], // 250B ┋ heavy quadruple dash vertical
    [0, 1, 1, 0], // 250C ┌ light down and right
    [0, 2, 1, 0], // 250D ┍ down light and right heavy
    [0, 1, 2, 0], // 250E ┎ down heavy and right light
    [0, 2, 2, 0], // 250F ┏ heavy down and right
    [0, 0, 1, 1], // 2510 ┐ light down and left
    [0, 0, 1, 2], // 2511 ┑ down light and left heavy
    [0, 0, 2, 1], // 2512 ┒ down heavy and left light
    [0, 0, 2, 2], // 2513 ┓ heavy down and left
    [1, 1, 0, 0], // 2514 └ light up and right
    [1, 2, 0, 0], // 2515 ┕ up light and right heavy
    [2, 1, 0, 0], // 2516 ┖ up heavy and right light
    [2, 2, 0, 0], // 2517 ┗ heavy up and right
    [1, 0, 0, 1], // 2518 ┘ light up and left
    [1, 0, 0, 2], // 2519 ┙ up light and left heavy
    [2, 0, 0, 1], // 251A ┚ up heavy and left light
    [2, 0, 0, 2], // 251B ┛ heavy up and left
    [1, 1, 1, 0], // 251C ├ light vertical and right
    [1, 2, 1, 0], // 251D ┝ vertical light and right heavy
    [2, 1, 1, 0], // 251E ┞ up heavy and right down light
    [1, 1, 2, 0], // 251F ┟ down heavy and right up light
    [2, 1, 2, 0], // 2520 ┠ vertical heavy and right light
    [2, 2, 1, 0], // 2521 ┡ down light and right up heavy
    [1, 2, 2, 0], // 2522 ┢ up light and right down heavy
    [2, 2, 2, 0], // 2523 ┣ heavy vertical and right
    [1, 0, 1, 1], // 2524 ┤ light vertical and left
    [1, 0, 1, 2], // 2525 ┥ vertical light and left heavy
    [2, 0, 1, 1], // 2526 ┦ up heavy and left down light
    [1, 0, 2, 1], // 2527 ┧ down heavy and left up light
    [2, 0, 2, 1], // 2528 ┨ vertical heavy and left light
    [2, 0, 1, 2], // 2529 ┩ down light and left up heavy
    [1, 0, 2, 2], // 252A ┪ up light and left down heavy
    [2, 0, 2, 2], // 252B ┫ heavy vertical and left
    [0, 1, 1, 1], // 252C ┬ light down and horizontal
    [0, 1, 1, 2], // 252D ┭ left heavy and right down light
    [0, 2, 1, 1], // 252E ┮ right heavy and left down light
    [0, 2, 1, 2], // 252F ┯ down light and horizontal heavy
    [0, 1, 2, 1], // 2530 ┰ down heavy and horizontal light
    [0, 1, 2, 2], // 2531 ┱ right light and left down heavy
    [0, 2, 2, 1], // 2532 ┲ left light and right down heavy
    [0, 2, 2, 2], // 2533 ┳ heavy down and horizontal
    [1, 1, 0, 1], // 2534 ┴ light up and horizontal
    [1, 1, 0, 2], // 2535 ┵ left heavy and right up light
    [1, 2, 0, 1], // 2536 ┶ right heavy and left up light
    [1, 2, 0, 2], // 2537 ┷ up light and horizontal heavy
    [2, 1, 0, 1], // 2538 ┸ up heavy and horizontal light
    [2, 1, 0, 2], // 2539 ┹ right light and left up heavy
    [2, 2, 0, 1], // 253A ┺ left light and right up heavy
    [2, 2, 0, 2], // 253B ┻ heavy up and horizontal
    [1, 1, 1, 1], // 253C ┼ light vertical and horizontal
    [1, 1, 1, 2], // 253D ┽ left heavy and right vertical light
    [1, 2, 1, 1], // 253E ┾ right heavy and left vertical light
    [1, 2, 1, 2], // 253F ┿ vertical light and horizontal heavy
    [2, 1, 1, 1], // 2540 ╀ up heavy and down horizontal light
    [1, 1, 2, 1], // 2541 ╁ down heavy and up horizontal light
    [2, 1, 2, 1], // 2542 ╂ vertical heavy and horizontal light
    [2, 1, 1, 2], // 2543 ╃ left up heavy and right down light
    [2, 2, 1, 1], // 2544 ╄ right up heavy and left down light
    [1, 1, 2, 2], // 2545 ╅ left down heavy and right up light
    [1, 2, 2, 1], // 2546 ╆ right down heavy and left up light
    [2, 2, 1, 2], // 2547 ╇ down light and up horizontal heavy
    [1, 2, 2, 2], // 2548 ╈ up light and down horizontal heavy
    [2, 1, 2, 2], // 2549 ╉ right light and left vertical heavy
    [2, 2, 2, 1], // 254A ╊ left light and right vertical heavy
    [2, 2, 2, 2], // 254B ╋ heavy vertical and horizontal
    [0, 0, 0, 0], // 254C ╌ light double dash horizontal
    [0, 0, 0, 0], // 254D ╍ heavy double dash horizontal
    [0, 0, 0, 0], // 254E ╎ light double dash vertical
    [0, 0, 0, 0], // 254F ╏ heavy double dash vertical
    [0, 3, 0, 3], // 2550 ═ double horizontal
    [3, 0, 3, 0], // 2551 ║ double vertical
    [0, 3, 1, 0], // 2552 ╒ down single and right double
    [0, 1, 3, 0], // 2553 ╓ down double and right single
    [0, 3, 3, 0], // 2554 ╔ double down and right
    [0, 0, 1, 3], // 2555 ╕ down single and left double
    [0, 0, 3, 1], // 2556 ╖ down double and left single
    [0, 0, 3, 3], // 2557 ╗ double down and left
    [1, 3, 0, 0], // 2558 ╘ up single and right double
    [3, 1, 0, 0], // 2559 ╙ up double and right single
    [3, 3, 0, 0], // 255A ╚ double up and right
    [1, 0, 0, 3], // 255B ╛ up single and left double
    [3, 0, 0, 1], // 255C ╜ up double and left single
    [3, 0, 0, 3], // 255D ╝ double up and left
    [1, 3, 1, 0], // 255E ╞ vertical single and right double
    [3, 1, 3, 0], // 255F ╟ vertical double and right single
    [3, 3, 3, 0], // 2560 ╠ double vertical and right
    [1, 0, 1, 3], // 2561 ╡ vertical single and left double
    [3, 0, 3, 1], // 2562 ╢ vertical double and left single
    [3, 0, 3, 3], // 2563 ╣ double vertical and left
    [0, 3, 1, 3], // 2564 ╤ down single and horizontal double
    [0, 1, 3, 1], // 2565 ╥ down double and horizontal single
    [0, 3, 3, 3], // 2566 ╦ double down and horizontal
    [1, 3, 0, 3], // 2567 ╧ up single and horizontal double
    [3, 1, 0, 1], // 2568 ╨ up double and horizontal single
    [3, 3, 0, 3], // 2569 ╩ double up and horizontal
    [1, 3, 1, 3], // 256A ╪ vertical single and horizontal double
    [3, 1, 3, 1], // 256B ╫ vertical double and horizontal single
    [3, 3, 3, 3], // 256C ╬ double vertical and horizontal
    [0, 0, 0, 0], // 256D ╭ light arc down and right
    [0, 0, 0, 0], // 256E ╮ light arc down and left
    [0, 0, 0, 0], // 256F ╯ light arc up and left
    [0, 0, 0, 0], // 2570 ╰ light arc up and right
    [0, 0, 0, 0], // 2571 ╱ light diagonal upper right to lower left
    [0, 0, 0, 0], // 2572 ╲ light diagonal upper left to lower right
    [0, 0, 0, 0], // 2573 ╳ light diagonal cross
    [0, 0, 0, 1], // 2574 ╴ light left
    [1, 0, 0, 0], // 2575 ╵ light up
    [0, 1, 0, 0], // 2576 ╶ light right
    [0, 0, 1, 0], // 2577 ╷ light down
    [0, 0, 0, 2], // 2578 ╸ heavy left
    [2, 0, 0, 0], // 2579 ╹ heavy up
    [0, 2, 0, 0], // 257A ╺ heavy right
    [0, 0, 2, 0], // 257B ╻ heavy down
    [0, 2, 0, 1], // 257C ╼ light left and heavy right
    [1, 0, 2, 0], // 257D ╽ light up and heavy down
    [0, 1, 0, 2], // 257E ╾ heavy left and light right
    [2, 0, 1, 0], // 257F ╿ heavy up and light down
];
