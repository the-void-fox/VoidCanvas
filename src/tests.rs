//! Regression tests for the document model, history and tools.

use crate::document::{Cell, Changes, Color, Document, Layer};
use crate::history::{Command, History};
use crate::tools::{
    DrawContext, merge_changes,
    fill::FillTool,
    selection::{SelectionTool, content_to_text, text_to_content},
};

const NONE: Color = Color::TRANSPARENT;

fn red() -> Color { Color::new(255, 0, 0, 255) }
fn blue() -> Color { Color::new(0, 0, 255, 255) }

fn glyph(ch: char) -> Cell { Cell::new(ch, Color::WHITE, NONE) }

fn ctx(ch: char, fg: Color, bg: Color) -> DrawContext {
    DrawContext {
        primary_char: ch,
        secondary_char: ' ',
        primary_fg: fg,
        primary_bg: bg,
        secondary_fg: Color::WHITE,
        secondary_bg: NONE,
        brush_size: 1,
        is_primary: true,
    }
}

/// "Background" at index 0, "Top" above it at index 1.
fn two_layer_doc() -> Document {
    let mut doc = Document::new(4, 3);
    doc.insert_layer(1, Layer::new(4, 3, "Top"));
    doc
}

#[test]
fn space_with_background_is_visible() {
    let mut doc = Document::new(2, 1);
    doc.layers[0].set(0, 0, Cell::new(' ', Color::WHITE, red()));
    assert_eq!(doc.composite_cell(0, 0).bg, red());
}

#[test]
fn glyph_on_upper_layer_keeps_background_below() {
    let mut doc = two_layer_doc();
    doc.layers[0].set(0, 0, Cell::new('.', Color::WHITE, red()));
    doc.layers[1].set(0, 0, Cell::new('#', blue(), NONE));
    let c = doc.composite_cell(0, 0);
    assert_eq!((c.ch, c.fg, c.bg), ('#', blue(), red()));
}

#[test]
fn merge_down_bakes_into_the_layer_below() {
    let mut doc = two_layer_doc();
    doc.layers[0].set(0, 0, glyph('a'));
    doc.layers[0].set(1, 0, glyph('b'));
    doc.layers[1].set(1, 0, glyph('X'));
    doc.active_layer = 1;

    let mut history = History::new(10);
    let cmd = Command::LayerMergeDown {
        upper:   1,
        layer:   doc.layers[1].clone(),
        changes: doc.merge_down_changes(1),
    };
    history.execute(cmd, &mut doc);
    assert_eq!(doc.layers.len(), 1);
    assert_eq!(doc.layers[0].name, "Background");
    assert_eq!(doc.layers[0].get(0, 0).unwrap().ch, 'a');
    assert_eq!(doc.layers[0].get(1, 0).unwrap().ch, 'X');

    history.undo(&mut doc);
    assert_eq!(doc.layers.len(), 2);
    assert_eq!(doc.layers[0].get(1, 0).unwrap().ch, 'b');
    assert_eq!(doc.layers[1].get(1, 0).unwrap().ch, 'X');
    assert_eq!(doc.active_layer, 1);
}

#[test]
fn layer_commands_undo_and_redo_exactly() {
    let mut doc = two_layer_doc();
    doc.layers[1].set(2, 1, Cell::new('@', red(), NONE));
    let original = doc.layers.clone();

    let mut history = History::new(100);
    history.execute(Command::LayerAdd { index: 2, layer: Layer::new(4, 3, "New") }, &mut doc);
    history.execute(Command::LayerMove { from: 2, to: 0 }, &mut doc);
    history.execute(Command::LayerRename { index: 0, old: "New".into(), new: "Renamed".into() }, &mut doc);
    let removed = doc.layers[1].clone();
    history.execute(Command::LayerRemove { index: 1, layer: removed }, &mut doc);
    let old_layers = doc.layers.clone();
    history.execute(Command::Resize { old_size: (4, 3), new_size: (2, 2), old_layers }, &mut doc);
    let after = (doc.layers.clone(), doc.width, doc.height);

    while history.undo(&mut doc) {}
    assert_eq!(doc.layers, original);
    assert_eq!((doc.width, doc.height), (4, 3));

    while history.redo(&mut doc) {}
    assert_eq!((doc.layers.clone(), doc.width, doc.height), after);
}

#[test]
fn drawing_is_undone_on_its_own_layer_after_reordering() {
    let mut doc = two_layer_doc();
    let mut history = History::new(10);

    let cell = Cell::new('#', red(), NONE);
    doc.layers[1].set(0, 0, cell.clone());
    let mut changes = Changes::new();
    changes.insert((0, 0), (Cell::empty(), cell));
    history.push(Command::Draw { layer_idx: 1, changes });
    history.execute(Command::LayerMove { from: 1, to: 0 }, &mut doc);

    history.undo(&mut doc);
    history.undo(&mut doc);
    assert!(doc.layers.iter().all(|l| l.get(0, 0).unwrap().is_empty()));
    assert_eq!(doc.layers[1].name, "Top");
}

#[test]
fn undo_works_on_a_locked_layer() {
    let mut doc = Document::new(2, 1);
    let mut history = History::new(10);
    let cell = glyph('#');
    doc.layers[0].set(0, 0, cell.clone());
    let mut changes = Changes::new();
    changes.insert((0, 0), (Cell::empty(), cell));
    history.push(Command::Draw { layer_idx: 0, changes });

    doc.layers[0].locked = true;
    history.undo(&mut doc);
    assert!(doc.layers[0].get(0, 0).unwrap().is_empty());
}

#[test]
fn fill_recolors_a_region_with_the_same_glyph() {
    let mut doc = Document::new(3, 1);
    doc.layers[0].set(0, 0, glyph('#'));
    doc.layers[0].set(1, 0, glyph('#'));
    doc.layers[0].set(2, 0, Cell::new('#', blue(), NONE));

    let changes = FillTool.flood_fill(&mut doc, 0, 0, &ctx('#', red(), NONE));
    assert_eq!(changes.len(), 2);
    assert_eq!(doc.layers[0].get(1, 0).unwrap().fg, red());
    assert_eq!(doc.layers[0].get(2, 0).unwrap().fg, blue(), "a differently coloured cell is a border");
}

#[test]
fn undoing_a_selection_move_restores_the_original() {
    let mut doc = Document::new(6, 3);
    doc.layers[0].set(0, 0, glyph('A'));
    doc.layers[0].set(1, 0, glyph('B'));
    let original = doc.layers.clone();

    // Select A and B …
    let mut sel = SelectionTool::default();
    sel.on_press(&doc, 0, 0);
    sel.on_drag(&mut doc, 1, 0);
    sel.on_release(&mut doc);

    // … and drag them one cell to the right, overlapping the source.
    let mut stroke = Changes::new();
    sel.on_press(&doc, 0, 0);
    merge_changes(&mut stroke, sel.on_drag(&mut doc, 1, 0));
    merge_changes(&mut stroke, sel.on_release(&mut doc));
    let row: String = (0..3).map(|x| doc.layers[0].get(x, 0).unwrap().ch).collect();
    assert_eq!(row, " AB");

    let mut history = History::new(10);
    history.push(Command::Draw { layer_idx: 0, changes: stroke });
    history.undo(&mut doc);
    assert_eq!(doc.layers, original);
}

#[test]
fn clipboard_text_round_trip() {
    let content = text_to_content("ab\n\tc", &red(), &NONE);
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].len(), 5, "a tab expands to the next multiple of 4");
    assert_eq!(content[0][0].fg, red());
    assert!(content[0][4].is_empty(), "short lines are padded with empty cells");
    assert_eq!(content_to_text(&content), "ab\n    c");
}

/// Drives the whole app headlessly with synthetic input.
mod ui {
    use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, accesskit::Role};
    use egui_kittest::{Harness, kittest::Queryable};

    use crate::app::AsciiArtEditor;
    use crate::tools::ToolType;

    fn harness() -> Harness<'static, AsciiArtEditor> {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_eframe(|_| AsciiArtEditor::default());
        harness.step();
        harness.step();
        harness
    }

    fn cell_pos(harness: &Harness<AsciiArtEditor>, x: usize, y: usize) -> Pos2 {
        harness.state().canvas_state.screen_cell_rect(x, y).center()
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    fn glyph_at(harness: &Harness<AsciiArtEditor>, x: usize, y: usize) -> char {
        harness.state().doc.layers[0].get(x, y).unwrap().ch
    }

    #[test]
    fn a_single_click_paints() {
        let mut harness = harness();
        let pos = cell_pos(&harness, 3, 2);
        harness.hover_at(pos);
        harness.step();
        harness.event(button(pos, true));
        harness.step();
        harness.event(button(pos, false));
        harness.step();

        assert_eq!(glyph_at(&harness, 3, 2), '#');
        assert!(harness.state().history.can_undo(), "the click is undoable");
    }

    #[test]
    fn a_tap_within_one_frame_paints() {
        let mut harness = harness();
        let pos = cell_pos(&harness, 5, 4);
        harness.hover_at(pos);
        harness.step();
        harness.event(button(pos, true));
        harness.event(button(pos, false));
        harness.step();

        assert_eq!(glyph_at(&harness, 5, 4), '#');
    }

    #[test]
    fn copy_and_paste_events_reach_the_canvas() {
        let mut harness = harness();
        harness.state_mut().doc.layers[0].set(0, 0, crate::document::Cell::new(
            'A', crate::document::Color::WHITE, crate::document::Color::TRANSPARENT,
        ));
        harness.state_mut().tools.selection.selection = Some((0, 0, 0, 0));
        harness.event(Event::Copy);
        harness.step();
        assert_eq!(harness.state().clipboard_text, "A");

        // Text from another program is pasted at the cursor.
        harness.hover_at(cell_pos(&harness, 6, 3));
        harness.step();
        harness.event(Event::Paste("hi".into()));
        harness.step();
        assert_eq!((glyph_at(&harness, 6, 3), glyph_at(&harness, 7, 3)), ('h', 'i'));
    }

    #[test]
    fn tool_hotkeys_are_ignored_while_typing() {
        let mut harness = harness();
        harness.get_by_role(Role::TextInput).focus();
        harness.step();
        harness.key_press(egui::Key::E);
        harness.get_by_role(Role::TextInput).type_text("e");
        harness.step();

        assert_eq!(harness.state().active_tool, ToolType::Brush);
        assert_eq!(harness.state().char_palette.search, "e");
    }
}
