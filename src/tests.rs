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
        channels: crate::tools::Channels::default(),
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
fn history_lists_steps_and_jumps_between_them() {
    let mut doc = Document::new(4, 1);
    let mut history = History::new(10);
    for (i, label) in ["Brush", "Fill", "Typing"].iter().enumerate() {
        let cell = glyph(char::from(b'a' + i as u8));
        doc.layers[0].set(i, 0, cell.clone());
        let changes = Changes::from([((i, 0), (Cell::empty(), cell))]);
        history.push_as(Command::Draw { layer_idx: 0, changes }, *label);
    }
    let row = |doc: &Document| (0..3).map(|x| doc.layers[0].get(x, 0).unwrap().ch).collect::<String>();

    history.jump_to(1, &mut doc);
    assert_eq!(row(&doc), "a  ");
    assert_eq!(history.labels(), (vec!["Brush"], vec!["Fill", "Typing"]));

    history.jump_to(3, &mut doc);
    assert_eq!(row(&doc), "abc");
    history.jump_to(0, &mut doc);
    assert_eq!(row(&doc), "   ");
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
            .build_eframe(|cc| AsciiArtEditor::new(cc));
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
    fn alt_click_picks_glyph_and_colours() {
        let mut harness = harness();
        harness.state_mut().doc.layers[0].set(4, 3, crate::document::Cell::new(
            '@', crate::document::Color::new(10, 20, 30, 255), crate::document::Color::TRANSPARENT,
        ));
        let pos = cell_pos(&harness, 4, 3);
        harness.hover_at(pos);
        harness.step();
        harness.input_mut().modifiers = Modifiers::ALT;
        harness.event(button(pos, true));
        harness.step();
        harness.event(button(pos, false));
        harness.step();

        let app = harness.state();
        assert_eq!(app.primary_char, '@');
        assert_eq!(app.color_panel.primary_fg, crate::document::Color::new(10, 20, 30, 255));
        assert_eq!(glyph_at(&harness, 4, 3), '@', "nothing was painted");
    }

    #[test]
    fn space_drag_pans_instead_of_painting() {
        let mut harness = harness();
        let pos = cell_pos(&harness, 3, 3);
        let offset = harness.state().canvas_state.offset;
        harness.hover_at(pos);
        harness.event(Event::Key { key: egui::Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        harness.step();
        harness.event(button(pos, true));
        harness.step();
        harness.hover_at(pos + egui::vec2(40.0, 30.0));
        harness.step();
        harness.event(button(pos + egui::vec2(40.0, 30.0), false));
        harness.step();

        assert_ne!(harness.state().canvas_state.offset, offset);
        assert_eq!(glyph_at(&harness, 3, 3), ' ');
    }

    #[test]
    fn select_all_and_bracket_keys() {
        let mut harness = harness();
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
        harness.step();
        let (w, h) = (harness.state().doc.width, harness.state().doc.height);
        assert_eq!(harness.state().tools.selection.selection, Some((0, 0, w - 1, h - 1)));

        harness.key_press(egui::Key::CloseBracket);
        harness.step();
        harness.key_press(egui::Key::CloseBracket);
        harness.step();
        assert_eq!(harness.state().tool_options.brush_size, 3);
        harness.key_press(egui::Key::OpenBracket);
        harness.step();
        assert_eq!(harness.state().tool_options.brush_size, 2);
    }

    #[test]
    fn typing_writes_text_and_undoes_in_one_step() {
        let mut harness = harness();
        harness.state_mut().active_tool = ToolType::Text;
        let pos = cell_pos(&harness, 2, 2);
        harness.hover_at(pos);
        harness.step();
        harness.event(button(pos, true));
        harness.event(button(pos, false));
        harness.step();

        harness.event(Event::Text("He".into()));
        harness.key_press(egui::Key::E);
        harness.step();
        harness.key_press(egui::Key::Enter);
        harness.event(Event::Text("x".into()));
        harness.step();

        assert_eq!(harness.state().active_tool, ToolType::Text, "letters don't switch tools while typing");
        let row: String = (2..4).map(|x| glyph_at(&harness, x, 2)).collect();
        assert_eq!(row, "He");
        assert_eq!(glyph_at(&harness, 2, 3), 'x', "Enter returns to the starting column");

        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
        harness.step();
        assert!([(2, 2), (3, 2), (2, 3)].iter().all(|&(x, y)| glyph_at(&harness, x, y) == ' '));
    }

    fn set_row(harness: &mut Harness<AsciiArtEditor>, x: usize, y: usize, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            harness.state_mut().doc.layers[0].set(x + i, y, crate::document::Cell::new(
                ch, crate::document::Color::WHITE, crate::document::Color::TRANSPARENT,
            ));
        }
    }

    fn row(harness: &Harness<AsciiArtEditor>, x: usize, y: usize, len: usize) -> String {
        (x..x + len).map(|x| glyph_at(harness, x, y)).collect()
    }

    #[test]
    fn flip_turns_glyphs_and_undoes() {
        let mut harness = harness();
        set_row(&mut harness, 1, 1, "(>-┌");
        harness.state_mut().active_tool = ToolType::Select;
        harness.state_mut().tools.selection.selection = Some((1, 1, 4, 1));
        harness.key_press(egui::Key::H);
        harness.step();
        assert_eq!(row(&harness, 1, 1, 4), "┐-<)");
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
        harness.step();
        assert_eq!(row(&harness, 1, 1, 4), "(>-┌");
    }

    #[test]
    fn arrows_move_the_selection_as_one_undo_step() {
        let mut harness = harness();
        set_row(&mut harness, 2, 2, "ab");
        harness.state_mut().active_tool = ToolType::Select;
        harness.state_mut().tools.selection.selection = Some((2, 2, 3, 2));
        harness.key_press(egui::Key::ArrowRight);
        harness.step();
        harness.key_press_modifiers(Modifiers::SHIFT, egui::Key::ArrowDown);
        harness.step();
        assert_eq!(row(&harness, 3, 7, 2), "ab");
        assert_eq!(row(&harness, 2, 2, 2), "  ");
        assert_eq!(harness.state().tools.selection.selection, Some((3, 7, 4, 7)));

        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
        harness.step();
        assert_eq!(row(&harness, 2, 2, 2), "ab");
        assert_eq!(row(&harness, 3, 7, 2), "  ");
    }

    #[test]
    fn dialogs_and_dock_tabs_render() {
        let mut harness = harness();
        harness.state_mut().show_about_dialog = true;
        harness.step();
        harness.state_mut().about_tab = 1;
        harness.step();
        harness.state_mut().show_new_dialog = true;
        harness.state_mut().show_resize_dialog = true;
        harness.state_mut().dock_tab = crate::app::DockTab::Colors;
        harness.step();
        harness.state_mut().dock_tab = crate::app::DockTab::History;
        harness.step();
        harness.step();
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

mod stage2_tools {
    use super::{NONE, blue, ctx, glyph, red};
    use crate::document::{Cell, Document};
    use crate::history::{Command, History};
    use crate::tools::{Channels, apply_stamps};
    use crate::tools::shapes::{Shape, ShapeMods, shape_box};

    const RECT: Shape = Shape::Rect { filled: false };

    #[test]
    fn shift_makes_a_visually_square_box() {
        let doc = Document::new(80, 40);
        let square = ShapeMods { constrain: true, from_center: false };
        // 10 columns × 2 rows dragged → 10 × 5 cells (cells are twice as tall as wide).
        assert_eq!(shape_box(RECT, (5, 5), (14, 6), square, &doc), ((5, 5), (14, 9)));
        // Dragging up-left keeps the direction.
        assert_eq!(shape_box(RECT, (20, 20), (19, 15), square, &doc), ((20, 20), (9, 15)));
    }

    #[test]
    fn alt_draws_from_the_centre_and_shift_straightens_lines() {
        let doc = Document::new(80, 40);
        let centre = ShapeMods { constrain: false, from_center: true };
        assert_eq!(shape_box(RECT, (10, 10), (13, 12), centre, &doc), ((7, 8), (13, 12)));
        let straight = ShapeMods { constrain: true, from_center: false };
        assert_eq!(shape_box(Shape::Line, (10, 10), (20, 12), straight, &doc), ((10, 10), (20, 10)));
        assert_eq!(shape_box(Shape::Line, (10, 10), (13, 22), straight, &doc), ((10, 10), (10, 22)));
        assert_eq!(shape_box(Shape::Line, (10, 10), (16, 14), straight, &doc), ((10, 10), (15, 15)));
    }

    #[test]
    fn channels_limit_what_tools_change() {
        let old = Cell::new('#', red(), blue());
        let mut c = ctx('@', Cell::empty().fg, NONE);
        c.channels = Channels { glyph: false, fg: true, bg: false };
        let painted = c.paint(&old, '@');
        assert_eq!((painted.ch, painted.bg), ('#', blue()), "only the foreground changes");
        assert_eq!(painted.fg, Cell::empty().fg);

        c.channels = Channels { glyph: false, fg: false, bg: true };
        let erased = c.erase(&old);
        assert_eq!((erased.ch, erased.fg, erased.bg.a), ('#', red(), 0), "only the background is erased");
    }

    #[test]
    fn repeated_cells_in_one_shape_still_undo() {
        // A one-row rectangle lists each cell twice (top and bottom edge).
        let mut doc = Document::new(6, 2);
        doc.layers[0].set(1, 0, glyph('a'));
        let original = doc.layers.clone();
        let stamps: Vec<_> = (0..4).flat_map(|x| [(x, 0, '#'), (x, 0, '#')]).collect();
        let changes = apply_stamps(&mut doc, &stamps, &ctx('#', red(), NONE));
        let mut history = History::new(5);
        history.push(Command::Draw { layer_idx: 0, changes });
        history.undo(&mut doc);
        assert_eq!(doc.layers, original);
    }
}

mod glyph_transforms {
    use super::glyph;
    use crate::glyphs::{Transform::*, box_arms, from_arms, transform_block, transform_glyph};
    use crate::ui::char_palette::CATEGORIES;

    fn every_glyph() -> impl Iterator<Item = char> {
        CATEGORIES.iter().flat_map(|(_, chars)| chars.chars()).chain((0x2500..0x2580).filter_map(char::from_u32))
    }

    #[test]
    fn box_arms_identify_one_glyph_each() {
        for c in (0x2500..0x2580).filter_map(char::from_u32) {
            if let Some(arms) = box_arms(c) {
                assert_eq!(from_arms(arms), Some(c), "{c}");
            }
        }
    }

    #[test]
    fn mirrors_and_rotations_undo_themselves() {
        for c in every_glyph() {
            for flip in [FlipHorizontal, FlipVertical] {
                assert_eq!(transform_glyph(transform_glyph(c, flip), flip), c, "{c} {flip:?}");
            }
            assert_eq!(transform_glyph(transform_glyph(c, RotateCw), RotateCcw), c, "{c}");
            let turned = (0..4).fold(c, |c, _| transform_glyph(c, RotateCw));
            assert_eq!(turned, c, "{c} four quarter turns");
        }
    }

    #[test]
    fn glyphs_turn_with_the_art() {
        assert_eq!(transform_glyph('┌', FlipHorizontal), '┐');
        assert_eq!(transform_glyph('┌', FlipVertical), '└');
        assert_eq!(transform_glyph('╒', RotateCw), '╖');
        assert_eq!(transform_glyph('─', RotateCw), '│');
        assert_eq!(transform_glyph('/', FlipHorizontal), '\\');
        assert_eq!(transform_glyph('<', RotateCw), '^');
        assert_eq!(transform_glyph('▌', FlipHorizontal), '▐');
        assert_eq!(transform_glyph('A', FlipHorizontal), 'A');
    }

    #[test]
    fn rotating_a_block_swaps_its_sides() {
        // a b c        d a
        // d e f   →    e b
        //              f c
        let block: Vec<Vec<_>> = ["abc", "def"].iter().map(|r| r.chars().map(glyph).collect()).collect();
        let turned = transform_block(&block, RotateCw);
        let rows: Vec<String> = turned.iter().map(|r| r.iter().map(|c| c.ch).collect()).collect();
        assert_eq!(rows, ["da", "eb", "fc"]);
        let back = transform_block(&turned, RotateCcw);
        assert_eq!(back, block);
    }
}

mod minor_fixes {
    use crate::document::{Cell, Color, Document};
    use crate::tools::{brush_positions, shapes::ellipse_points};
    use crate::ui::char_palette::{CATEGORIES, search};

    #[test]
    fn every_brush_size_is_distinct() {
        for size in 1..=6 {
            let cells = brush_positions(10, 10, size);
            assert_eq!(cells.len(), size * size, "size {size}");
        }
    }

    #[test]
    fn ellipse_fits_its_box() {
        for w in 1..=9 {
            for h in 1..=9 {
                let (x2, y2) = (2 + w - 1, 3 + h - 1);
                let outline = ellipse_points(2, 3, x2, y2, false);
                let filled = ellipse_points(2, 3, x2, y2, true);
                let xs = || outline.iter().map(|p| p.0);
                let ys = || outline.iter().map(|p| p.1);
                assert_eq!((xs().min(), xs().max()), (Some(2), Some(x2)), "{w}x{h}");
                assert_eq!((ys().min(), ys().max()), (Some(3), Some(y2)), "{w}x{h}");
                assert!(outline.iter().all(|p| filled.contains(p)), "{w}x{h}");
            }
        }
    }

    #[test]
    fn char_search_finds_codes_categories_and_any_char() {
        assert_eq!(search("2588").first(), Some(&'█'));
        assert_eq!(search("U+2588").first(), Some(&'█'));
        assert!(search("arrow").contains(&'→'));
        assert_eq!(search("ж"), vec!['ж']);
        assert!(!search("face").contains(&'\u{face}'), "plain words are not code points");
    }

    #[test]
    fn palettes_have_no_duplicates() {
        for (name, chars) in CATEGORIES {
            let mut seen = std::collections::HashSet::new();
            for c in chars.chars() {
                assert!(seen.insert(c), "{name} has {c} twice");
            }
        }
    }

    #[test]
    fn png_export_blends_alpha_and_can_be_transparent() {
        let mut doc = Document::new(2, 1);
        doc.layers[0].set(1, 0, Cell::new(' ', Color::WHITE, Color::new(255, 0, 0, 128)));

        let img = crate::export::render_png(&doc, true).unwrap();
        assert_eq!(img.get_pixel(0, 0)[3], 0, "empty cell stays transparent");
        let red = img.get_pixel(crate::export::CELL_W as u32 + 1, 1);
        assert_eq!((red[0], red[3]), (255, 128));

        let img = crate::export::render_png(&doc, false).unwrap();
        let red = img.get_pixel(crate::export::CELL_W as u32 + 1, 1);
        assert_eq!((red[0], red[1], red[3]), (128, 0, 255), "half-transparent red over black");
    }

    #[test]
    fn png_full_blocks_leave_no_seams() {
        let mut doc = Document::new(3, 3);
        for y in 0..3 {
            for x in 0..3 {
                doc.layers[0].set(x, y, Cell::new('█', Color::WHITE, Color::TRANSPARENT));
            }
        }
        let img = crate::export::render_png(&doc, false).unwrap();
        let darkest = img.pixels().map(|p| p[0]).min().unwrap();
        assert!(darkest > 200, "a seam between blocks: darkest pixel {darkest}");
    }
}

/// Renders the whole window offscreen, e.g. for the README screenshot:
/// `SHOT_DOC=art.vc SHOT_OUT=shot.png cargo test render_window -- --ignored`
#[test]
#[ignore = "needs a GPU; run by hand"]
fn render_window() {
    use eframe::egui;
    let out = std::env::var("SHOT_OUT").unwrap_or_else(|_| "screenshot.png".into());
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .wgpu()
        .build_eframe(|cc| {
            let mut app = crate::app::AsciiArtEditor::new(cc);
            if let Ok(doc) = std::env::var("SHOT_DOC") {
                app.open_path(std::path::Path::new(&doc));
            }
            app
        });
    for _ in 0..5 {
        harness.step();
    }
    harness.render().expect("render").save(out).expect("save");
}
