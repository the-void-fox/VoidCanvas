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
        mirror: crate::tools::Mirror::default(),
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
fn background_on_an_upper_layer_goes_behind_the_glyph_below() {
    // The demo case: mountains on a lower layer, the background painted on the layer above.
    let mut doc = two_layer_doc();
    doc.layers[0].set(0, 0, Cell::new('▄', blue(), NONE));
    doc.active_layer = 1;
    let mut back_only = ctx('#', Color::WHITE, red());
    back_only.channels = crate::tools::Channels { glyph: false, fg: false, bg: true };
    crate::tools::apply_stamps(&mut doc, &[(0, 0, '#')], &back_only);

    let c = doc.composite_cell(0, 0);
    assert_eq!((c.ch, c.fg, c.bg), ('▄', blue(), red()));
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
        upper:  1,
        layer:  Box::new(doc.layers[1].clone()),
        below:  Box::new(doc.layers[0].clone()),
        merged: Box::new(doc.merged_down(1)),
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
    history.push(Command::Draw { layer_idx: 1, frame: 0, changes });
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
        history.push_as(Command::Draw { layer_idx: 0, frame: 0, changes }, *label);
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
    history.push(Command::Draw { layer_idx: 0, frame: 0, changes });

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
    history.push(Command::Draw { layer_idx: 0, frame: 0, changes: stroke });
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
    fn painting_remembers_glyph_and_colour() {
        let mut harness = harness();
        harness.state_mut().primary_char = '@';
        harness.state_mut().color_panel.primary_fg = crate::document::Color::new(1, 2, 3, 255);
        let pos = cell_pos(&harness, 1, 1);
        harness.hover_at(pos);
        harness.step();
        harness.event(button(pos, true));
        harness.event(button(pos, false));
        harness.step();
        let app = harness.state();
        assert_eq!(app.char_palette.recent.first(), Some(&'@'));
        assert_eq!(app.color_panel.recent.first(), Some(&crate::document::Color::new(1, 2, 3, 255)));
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
    fn gradient_drag_fills_the_layer_in_one_undo_step() {
        let mut harness = harness();
        harness.key_press(egui::Key::U);
        harness.step();
        assert_eq!(harness.state().active_tool, ToolType::Gradient);

        let (from, to) = (cell_pos(&harness, 0, 0), cell_pos(&harness, 20, 0));
        harness.hover_at(from);
        harness.step();
        harness.event(button(from, true));
        harness.step();
        harness.hover_at(to);
        harness.step();
        assert!(harness.state().doc.layers[0].get(0, 5).unwrap().is_empty(), "only a preview while dragging");
        harness.event(button(to, false));
        harness.step();

        let doc = &harness.state().doc;
        assert_eq!(doc.layers[0].get(0, 5).unwrap().ch, '█', "the whole layer, not just the dragged row");
        assert_eq!(doc.layers[0].get(30, 5).unwrap().ch, ' ', "past the end it's empty");
        assert_eq!(harness.state().history.labels().0, vec!["Gradient"]);
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
        harness.step();
        assert_eq!(glyph_at(&harness, 0, 5), ' ');
    }

    #[test]
    fn command_palette_runs_and_remembers_commands() {
        let mut harness = harness();
        harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::P);
        harness.step();
        assert!(harness.state().command_palette.is_some());
        harness.state_mut().command_palette.as_mut().unwrap().query = "grad".into();
        harness.step();
        harness.key_press(egui::Key::Enter);
        harness.step();

        let app = harness.state();
        assert!(app.command_palette.is_none());
        assert_eq!(app.active_tool, ToolType::Gradient);
        let entries = app.actions();
        let first = &entries[app.palette_matches(&entries, "")[0]];
        assert_eq!((first.group, first.name.as_str()), ("Tool", "Gradient"), "recently used commands come first");

        // Esc closes it without running anything.
        harness.key_press(egui::Key::F1);
        harness.step();
        harness.key_press(egui::Key::Escape);
        harness.step();
        assert!(harness.state().command_palette.is_none());
        assert_eq!(harness.state().active_tool, ToolType::Gradient);
    }

    #[test]
    fn enter_plays_the_animation_and_drawing_stops_it() {
        let mut harness = harness();
        let ctx = harness.ctx.clone();
        harness.state_mut().run(crate::app::commands::Action::NewFrame, &ctx);
        // The second frame stays long enough not to wrap around while the test clicks.
        harness.state_mut().doc.durations = vec![1000, 9000];
        harness.key_press(egui::Key::Comma);
        harness.step();
        assert_eq!(harness.state().doc.frame, 0, ", goes to the previous frame");

        harness.key_press(egui::Key::Enter);
        harness.step();
        assert!(harness.state().playback.playing);
        // Each test step is a quarter of a second.
        for _ in 0..4 { harness.step(); }
        assert_eq!(harness.state().doc.frame, 1);

        let pos = cell_pos(&harness, 2, 2);
        harness.hover_at(pos);
        harness.event(button(pos, true));
        harness.event(button(pos, false));
        harness.step();
        let app = harness.state();
        assert!(!app.playback.playing, "drawing stops playback");
        assert_eq!((app.doc.frame, glyph_at(&harness, 2, 2)), (1, '#'), "and paints on the frame in view");
    }

    #[test]
    fn the_script_window_runs_with_ctrl_enter() {
        let mut harness = harness();
        let ctx = harness.ctx.clone();
        harness.state_mut().run(crate::app::commands::Action::Script, &ctx);
        harness.state_mut().script.as_mut().unwrap().code = "text(0, 0, \"ok\");".into();
        harness.step();
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Enter);
        harness.step();
        assert_eq!(row(&harness, 0, 0, 2), "ok");
        assert_eq!(harness.state().history.labels().0, vec!["Script"]);
    }

    #[test]
    fn the_layers_panel_sets_the_blend_mode_in_one_undo_step() {
        let mut harness = harness();
        harness.get_by_value("Normal").click();
        harness.step();
        // A popup item takes a click once the pointer has rested on it for a frame, as a real one does.
        harness.get_by_label("Overlay").hover();
        harness.step();
        harness.get_by_label("Overlay").click();
        harness.step();
        harness.step();
        let app = harness.state();
        assert_eq!(app.doc.active_layer().blend, crate::document::Blend::Overlay);
        assert_eq!(app.history.labels().0, vec!["Blend Mode"]);
    }

    #[test]
    fn users_make_their_own_glyph_sets() {
        use crate::ui::char_palette::{CATEGORIES, SetAction};
        let mut app = AsciiArtEditor::default();
        let ctx = egui::Context::default();
        app.apply_set_action(SetAction::New, &ctx);
        assert_eq!(app.settings.glyph_sets[0].name, "My Glyphs 1");
        assert_eq!(app.char_palette.category, CATEGORIES.len(), "the new set is shown");
        app.apply_set_action(SetAction::Add(0, "★a★ b\n".into()), &ctx);
        assert_eq!(app.settings.glyph_sets[0].glyphs, "★ab", "no repeats, no spaces");
        app.apply_set_action(SetAction::Remove(0, 'a'), &ctx);
        for (x, ch) in "xyzx".chars().enumerate() {
            app.doc.layers[0].set(x, 0, crate::document::Cell::new(ch, crate::document::Color::WHITE, crate::document::Color::TRANSPARENT));
        }
        app.tools.selection.selection = Some((1, 0, 3, 0));
        app.apply_set_action(SetAction::FromSelection(0), &ctx);
        assert_eq!(app.settings.glyph_sets[0].glyphs, "★byzx");
        app.apply_set_action(SetAction::Rename(0, "  Stars ".into()), &ctx);
        assert_eq!(app.settings.glyph_sets[0].name, "Stars");
        app.apply_set_action(SetAction::Delete(0), &ctx);
        assert!(app.settings.glyph_sets.is_empty());
    }

    #[test]
    fn a_script_becomes_the_brush() {
        let mut harness = harness();
        let ctx = harness.ctx.clone();
        harness.state_mut().run(crate::app::commands::Action::Script, &ctx);
        harness.state_mut().script.as_mut().unwrap().code = crate::script::RAINBOW_BRUSH.into();
        harness.step();
        harness.get_by_label("Use as Brush").click();
        harness.step();
        let app = harness.state();
        assert_eq!(app.active_tool, ToolType::Brush);
        assert!(matches!(&app.tool_options.brush, crate::tools::brush::BrushKind::Script { code } if code.contains("fn brush")));
    }

    #[test]
    fn several_frames_go_or_copy_in_one_step() {
        let mut app = AsciiArtEditor::default();
        app.doc = crate::document::Document::new(2, 1);
        let ctx = egui::Context::default();
        for ch in ['b', 'c', 'd', 'e'] {
            app.run(crate::app::commands::Action::NewFrame, &ctx);
            app.doc.layers[0].put(0, 0, crate::document::Cell::new(ch, crate::document::Color::WHITE, crate::document::Color::TRANSPARENT));
        }
        app.doc.show_frame(0);
        app.doc.layers[0].put(0, 0, crate::document::Cell::new('a', crate::document::Color::WHITE, crate::document::Color::TRANSPARENT));
        let frames = |app: &AsciiArtEditor| (0..app.doc.frame_count()).map(|f| app.doc.composite_in(f, 0, 0).ch).collect::<String>();
        assert_eq!(frames(&app), "abcde");

        app.playback.selected = [1, 3].into();
        app.run(crate::app::commands::Action::DeleteFrame, &ctx);
        assert_eq!(frames(&app), "ace");
        assert_eq!(app.history.labels().0.last(), Some(&"Delete Frames"));
        app.run(crate::app::commands::Action::Undo, &ctx);
        assert_eq!(frames(&app), "abcde", "one undo step brings both back");

        app.playback.selected = [0, 1].into();
        app.duplicate_frames();
        assert_eq!(frames(&app), "ababcde");

        app.playback.selected = (0..7).collect();
        app.run(crate::app::commands::Action::DeleteFrame, &ctx);
        assert_eq!(app.doc.frame_count(), 7, "the last frame can't go");
    }

    #[test]
    fn right_clicking_a_layer_opens_its_menu() {
        let mut harness = harness();
        harness.get_all_by_label("Background").next().unwrap().click_secondary();
        harness.step();
        harness.step();
        harness.get_by_label("Duplicate").hover();
        harness.step();
        harness.get_by_label("Duplicate").click();
        harness.step();
        assert_eq!(harness.state().doc.layers.len(), 2);
        harness.get_all_by_label("Background").next().unwrap().click_secondary();
        harness.step();
        harness.step();
        harness.get_by_label("Select Content").hover();
        harness.step();
        harness.get_by_label("Select Content").click();
        harness.step();
        assert_eq!(harness.state().tools.selection.selection, None, "an empty layer has nothing to select");
    }

    #[test]
    fn ctrl_b_makes_the_selection_the_brush() {
        let mut harness = harness();
        set_row(&mut harness, 0, 0, "<>");
        harness.state_mut().tools.selection.selection = Some((0, 0, 1, 0));
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::B);
        harness.step();
        let app = harness.state();
        assert_eq!(app.active_tool, ToolType::Brush);
        assert_eq!(app.tool_options.brush_name, "Stamp 2×1");
        assert!(matches!(&app.tool_options.brush, crate::tools::brush::BrushKind::Stamp { cells, .. } if cells[0][1].ch == '>'));
        let entries = app.actions();
        assert!(entries.iter().any(|e| e.group == "Brush" && e.name == "Stars"), "presets are in the palette");
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
    fn dropping_a_text_file_imports_it() {
        let path = std::env::temp_dir().join(format!("void-canvas-test-{}.txt", std::process::id()));
        std::fs::write(&path, "/\\_/\\\n( o.o )\n > ^ <").unwrap();

        let mut harness = harness();
        harness.input_mut().dropped_files.push(egui::DroppedFile { path: Some(path.clone()), ..Default::default() });
        harness.step();
        harness.step();
        std::fs::remove_file(&path).unwrap();

        let doc = &harness.state().doc;
        assert_eq!((doc.width, doc.height), (7, 3));
        assert_eq!(crate::export::to_text(doc, crate::export::whole(doc)), "/\\_/\\\n( o.o )\n > ^ <");
        assert!(doc.modified && doc.file_path.is_none(), "an import is a new, unsaved document");
    }

    #[test]
    fn recovering_an_autosave_restores_every_document() {
        use crate::app::storage::{Autosave, SavedDocument};
        let mut doc = crate::document::Document::new(5, 2);
        doc.layers[0].set(1, 1, crate::document::Cell::new('R', crate::document::Color::WHITE, crate::document::Color::TRANSPARENT));
        let mut harness = harness();
        harness.state_mut().recovery = Some(Autosave {
            documents: vec![
                SavedDocument { file_path: Some("/art/lost.vc".into()), document: doc },
                SavedDocument { file_path: None, document: crate::document::Document::new(3, 3) },
            ],
            age: None,
        });
        // The modal measures itself in its first frame.
        harness.step();
        harness.step();
        harness.get_by_label("Recover").click();
        harness.step();
        harness.step();

        let app = harness.state();
        assert!(app.recovery.is_none());
        assert_eq!(app.tab_count(), 2, "the empty start-up document gave its tab up");
        assert_eq!((app.doc.width, app.doc.height), (3, 3));
        harness.state_mut().switch_tab(0);
        let app = harness.state();
        assert_eq!((app.doc.width, app.doc.height), (5, 2));
        assert_eq!(glyph_at(&harness, 1, 1), 'R');
        assert!(app.documents().all(|doc| doc.modified), "recovered work is still unsaved");
        assert_eq!(app.doc.file_path.as_deref(), Some("/art/lost.vc"));
    }

    #[test]
    fn dropping_a_picture_imports_it_as_a_layer() {
        let path = std::env::temp_dir().join(format!("void-canvas-test-{}.png", std::process::id()));
        image::RgbaImage::from_pixel(40, 40, image::Rgba([200, 100, 50, 255])).save(&path).unwrap();

        let mut harness = harness();
        harness.input_mut().dropped_files.push(egui::DroppedFile { path: Some(path.clone()), ..Default::default() });
        // The picture is read on another thread; the dialog opens when it's in.
        for _ in 0..400 {
            harness.step();
            if harness.state().image_import.is_some() { break; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        harness.step();
        std::fs::remove_file(&path).unwrap();
        assert!(harness.state().image_import.is_some(), "the import dialog is open");

        harness.get_by_label("Import as layer").click();
        harness.step();
        harness.step();
        let app = harness.state();
        assert!(app.image_import.is_none());
        assert_eq!(app.doc.layers.len(), 2);
        assert!(app.doc.active_layer().name.starts_with("Image: "));
        assert_eq!(app.history.labels().0, vec!["Import Image"]);
        assert_eq!(app.doc.active_layer().get(0, 0).unwrap().ch, '█', "a flat colour becomes full half-block pixels");
    }

    #[test]
    fn replace_all_from_the_dialog() {
        let mut harness = harness();
        set_row(&mut harness, 0, 0, "..x.");
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::H);
        harness.step();
        harness.step();
        harness.get_by_label("Replace All").click();
        harness.step();
        assert_eq!(row(&harness, 0, 0, 4), "##x#", "secondary '.' → primary '#' by default");
        assert_eq!(harness.state().history.labels().0, vec!["Replace"]);
    }

    #[test]
    fn banner_is_inserted_as_a_movable_selection() {
        let mut harness = harness();
        harness.state_mut().banner = Some(crate::app::banner::BannerDialog { text: "Hi".into(), font: 0 });
        harness.step();
        harness.step();
        harness.get_by_label("Insert").click();
        harness.step();
        let app = harness.state();
        assert!(app.banner.is_none());
        assert_eq!(app.active_tool, ToolType::Select);
        assert_eq!(app.tools.selection.selection, Some((0, 0, 9, 4)));
        assert_eq!(row(&harness, 0, 1, 10), "| | | |(_)");
        assert_eq!(harness.state().history.labels().0, vec!["Banner"]);
    }

    #[test]
    fn recent_files_are_unique_newest_first_and_capped() {
        let mut settings = crate::app::storage::Settings::default();
        for i in 0..12 {
            settings.add_recent(&format!("/art/{i}.vc"));
        }
        settings.add_recent("/art/5.vc");
        assert_eq!(settings.recent_files.len(), 10);
        assert_eq!(settings.recent_files[0], "/art/5.vc");
        assert_eq!(settings.recent_files.iter().filter(|p| *p == "/art/5.vc").count(), 1);
    }

    #[test]
    fn pixel_tool_paints_half_cells() {
        let mut harness = harness();
        harness.state_mut().active_tool = ToolType::Pixel;
        harness.state_mut().color_panel.primary_fg = crate::document::Color::new(255, 0, 0, 255);
        harness.state_mut().color_panel.secondary_fg = crate::document::Color::new(0, 0, 255, 255);
        let rect = harness.state().canvas_state.screen_cell_rect(4, 2);
        let top = rect.center() - egui::vec2(0.0, rect.height() / 4.0);
        let bottom = rect.center() + egui::vec2(0.0, rect.height() / 4.0);
        for (pos, primary) in [(top, true), (bottom, false)] {
            let button_id = if primary { PointerButton::Primary } else { PointerButton::Secondary };
            harness.hover_at(pos);
            harness.step();
            harness.event(Event::PointerButton { pos, button: button_id, pressed: true, modifiers: Modifiers::NONE });
            harness.event(Event::PointerButton { pos, button: button_id, pressed: false, modifiers: Modifiers::NONE });
            harness.step();
        }
        let cell = harness.state().doc.layers[0].get(4, 2).unwrap().clone();
        assert_eq!(cell.ch, '▀');
        assert_eq!((cell.fg.r, cell.bg.b), (255, 255), "red on top, blue below");
    }

    /// egui warns (and outlines widgets in red in debug builds) when a widget keeps its
    /// place but changes its id between frames; that also loses hover and click state.
    #[test]
    fn widgets_keep_their_ids_while_the_ui_changes() {
        let warnings = capture_warnings();
        let mut harness = harness();
        let canvas = cell_pos(&harness, 5, 5);
        let status_corner = egui::pos2(1390.0, 892.0);
        for tool in ToolType::groups().iter().flat_map(|g| g.iter()) {
            harness.state_mut().active_tool = *tool;
            for pos in [canvas, status_corner, canvas] {
                harness.hover_at(pos);
                harness.step();
            }
        }
        let changed: Vec<String> = warnings.lock().unwrap().iter().filter(|w| w.contains("changed id")).cloned().collect();
        assert!(changed.is_empty(), "{changed:#?}");
    }

    #[test]
    fn deleting_frames_keeps_timeline_ids() {
        let warnings = capture_warnings();
        let mut harness = harness();
        let ctx = harness.ctx.clone();
        for _ in 0..3 {
            harness.state_mut().run(crate::app::commands::Action::NewFrame, &ctx);
            harness.step();
        }
        for _ in 0..3 {
            harness.state_mut().run(crate::app::commands::Action::DeleteFrame, &ctx);
            harness.step();
            harness.step();
        }
        let changed: Vec<String> = warnings.lock().unwrap().iter().filter(|w| w.contains("changed id")).cloned().collect();
        assert!(changed.is_empty(), "{changed:#?}");
    }

    /// Collects `log` warnings from now on (one logger for the whole test run).
    fn capture_warnings() -> &'static std::sync::Mutex<Vec<String>> {
        struct Capture;
        static LINES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        impl log::Log for Capture {
            fn enabled(&self, metadata: &log::Metadata) -> bool {
                metadata.level() <= log::Level::Warn
            }
            fn log(&self, record: &log::Record) {
                if self.enabled(record.metadata()) {
                    LINES.lock().unwrap().push(record.args().to_string());
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: Capture = Capture;
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Warn);
        }
        &LINES
    }

    /// An empty folder of its own for one test.
    fn temp_folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("void-canvas-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_a_new_document_goes_through_the_built_in_chooser() {
        let dir = temp_folder("save");
        let mut harness = harness();
        harness.state_mut().last_dir = Some(dir.clone());
        set_row(&mut harness, 0, 0, "hi");

        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::S);
        harness.step();
        harness.step();
        let browser = harness.state_mut().file_browser.as_mut().expect("the chooser is open");
        assert_eq!((browser.dir.canonicalize().unwrap(), browser.name.as_str()), (dir.canonicalize().unwrap(), "Untitled.vc"));
        browser.name = "art".into(); // the extension gets added
        harness.get_by_label("Save").click();
        harness.step();

        let path = dir.join("art.vc");
        let app = harness.state();
        assert!(app.file_browser.is_none());
        assert!(path.is_file());
        // Compared as resolved paths: Windows may name the temp folder in its short 8.3 form.
        let saved = std::fs::canonicalize(app.doc.file_path.as_deref().unwrap()).unwrap();
        assert_eq!(saved, std::fs::canonicalize(&path).unwrap());
        assert!(!app.doc.modified);

        // From now on Ctrl+S writes straight to the file.
        set_row(&mut harness, 0, 0, "yo");
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::S);
        harness.step();
        assert!(harness.state().file_browser.is_none());
        let saved = crate::app::file_io::parse_document(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.layers[0].get(0, 0).unwrap().ch, 'y');
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_built_in_chooser_lists_folders_and_matching_files() {
        let dir = temp_folder("open");
        std::fs::create_dir(dir.join("sub")).unwrap();
        std::fs::create_dir(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("notes.md"), "").unwrap();
        let mut doc = crate::document::Document::new(3, 1);
        doc.layers[0].set(1, 0, crate::document::Cell::new('Q', crate::document::Color::WHITE, crate::document::Color::TRANSPARENT));
        std::fs::write(dir.join("pic.vc"), serde_json::to_string(&doc).unwrap()).unwrap();

        let mut harness = harness();
        harness.state_mut().last_dir = Some(dir.clone());
        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::O);
        harness.step();
        harness.step();
        assert!(harness.query_by_label_contains("sub").is_some());
        assert!(harness.query_by_label_contains(".hidden").is_none());
        assert!(harness.query_by_label_contains("notes.md").is_none());

        harness.get_by_label_contains("pic.vc").click();
        harness.step();
        harness.get_by_role_and_label(Role::Button, "Open").click();
        harness.step();
        let app = harness.state();
        assert!(app.file_browser.is_none());
        assert_eq!((app.doc.width, app.doc.height), (3, 1));
        assert_eq!(glyph_at(&harness, 1, 0), 'Q');
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_before_closing_a_tab_goes_through_the_chooser() {
        let dir = temp_folder("save-then-close");
        let mut harness = harness();
        harness.state_mut().last_dir = Some(dir.clone());
        harness.state_mut().open_in_tab(crate::document::Document::new(4, 4));
        set_row(&mut harness, 0, 0, "x");
        harness.state_mut().doc.modified = true;

        harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::W);
        harness.step();
        harness.step();
        harness.get_by_label("Save").click(); // in "Unsaved changes"
        harness.step();
        harness.step();
        harness.get_by_label("Save").click(); // in the chooser
        harness.step();

        assert!(dir.join("Untitled.vc").is_file());
        let app = harness.state();
        assert_eq!(app.tab_count(), 1, "the tab closed once it was saved");
        assert_eq!((app.doc.width, app.doc.height), (80, 40));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn write_doc(path: &std::path::Path, ch: char) {
        let mut doc = crate::document::Document::new(6, 2);
        doc.layers[0].set(0, 0, crate::document::Cell::new(ch, crate::document::Color::WHITE, crate::document::Color::TRANSPARENT));
        std::fs::write(path, serde_json::to_string(&doc).unwrap()).unwrap();
    }

    #[test]
    fn files_open_in_tabs_with_their_own_history() {
        let dir = temp_folder("tabs");
        let (a, b) = (dir.join("a.vc"), dir.join("b.vc"));
        write_doc(&a, 'A');
        write_doc(&b, 'B');
        let mut harness = harness();
        harness.state_mut().open_path(&a);
        harness.state_mut().open_path(&b);
        harness.step();
        assert_eq!(harness.state().tab_count(), 2, "the untouched start-up tab was reused");
        assert_eq!(glyph_at(&harness, 0, 0), 'B');

        let z = crate::document::Cell::new('z', crate::document::Color::WHITE, crate::document::Color::TRANSPARENT);
        let changes = crate::document::Changes::from([((1, 1), (crate::document::Cell::empty(), z))]);
        let app = harness.state_mut();
        app.history.execute(crate::history::Command::Draw { layer_idx: 0, frame: 0, changes }, &mut app.doc);
        harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::Tab);
        harness.step();
        assert_eq!(glyph_at(&harness, 0, 0), 'A');
        assert!(!harness.state().history.can_undo(), "each tab has its own history");

        harness.state_mut().open_path(&b);
        harness.step();
        let app = harness.state();
        assert_eq!((app.tab_count(), app.active_tab()), (2, 1), "an open file isn't opened twice");
        assert!(app.history.can_undo());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn quitting_asks_about_each_unsaved_tab() {
        let mut harness = harness();
        harness.state_mut().doc.modified = true;
        harness.state_mut().open_in_tab(crate::document::Document::new(4, 4));
        harness.state_mut().open_in_tab(crate::document::Document::new(5, 5));
        harness.state_mut().doc.modified = true;
        harness.state_mut().switch_tab(1);

        let ctx = harness.ctx.clone();
        harness.state_mut().run(crate::app::commands::Action::Quit, &ctx);
        for _ in 0..2 {
            harness.step();
            harness.step();
            assert!(harness.state().doc.modified, "an unsaved tab is in front while asked about");
            harness.get_by_label("Don't Save").click();
            harness.step();
        }
        let app = harness.state();
        assert!(app.allow_close, "nothing unsaved is left, so the window closes");
        assert_eq!(app.tab_count(), 1, "the clean tab was never asked about");
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
        history.push(Command::Draw { layer_idx: 0, frame: 0, changes });
        history.undo(&mut doc);
        assert_eq!(doc.layers, original);
    }
}

mod stage3 {
    use super::{NONE, blue, ctx, red};
    use crate::document::{Cell, Document};
    use crate::tools::{Mirror, apply_stamps};
    use crate::tools::pixel::{from_halves, halves};

    #[test]
    fn half_blocks_hold_two_pixels() {
        assert_eq!(from_halves(Some(red()), Some(blue())), Cell::new('▀', red(), NONE).with_bg(blue()));
        assert_eq!(from_halves(Some(red()), Some(red())).ch, '█');
        assert_eq!(from_halves(None, Some(blue())), Cell::new('▄', blue(), NONE));
        assert!(from_halves(None, None).is_empty());
        for (top, bottom) in [(Some(red()), None), (None, Some(red())), (Some(red()), Some(blue())), (Some(blue()), Some(blue()))] {
            assert_eq!(halves(&from_halves(top.clone(), bottom.clone())), (top, bottom));
        }
    }

    #[test]
    fn finer_pixel_grids_build_their_glyphs() {
        use crate::tools::pixel::{PixelGrid, PixelTool};
        let paint = |grid: PixelGrid, dots: &[(usize, usize)], erase: &[(usize, usize)]| {
            let mut doc = Document::new(2, 1);
            doc.layers[0].set(0, 0, super::glyph('#'));
            let mut tool = PixelTool::default();
            let mut c = ctx('#', red(), NONE);
            for &p in dots {
                tool.on_press(&mut doc, p, grid, &c);
                tool.on_release();
            }
            c.is_primary = false;
            for &p in erase {
                tool.on_press(&mut doc, p, grid, &c);
                tool.on_release();
            }
            doc.layers[0].get(0, 0).unwrap().clone()
        };
        // Braille: column 0 holds dots 1, 2, 3, 7 from the top.
        let cell = paint(PixelGrid::Braille, &[(0, 0), (1, 3)], &[]);
        assert_eq!((cell.ch, cell.fg.clone()), ('⢁', red()), "a foreign glyph gives way");
        assert_eq!(paint(PixelGrid::Braille, &[(0, 0), (1, 3)], &[(1, 3)]).ch, '⠁', "the right button clears a dot");
        assert_eq!(paint(PixelGrid::Quarters, &[(0, 0), (1, 1)], &[]).ch, '▚');
        assert_eq!(paint(PixelGrid::Quarters, &[(0, 0), (0, 1)], &[]).ch, '▌');
        assert_eq!(paint(PixelGrid::Sixths, &[(0, 0), (0, 1), (0, 2)], &[]).ch, '▌', "the left column is the half block");
        assert_eq!(paint(PixelGrid::Sixths, &[(1, 2)], &[]).ch, '🬞');
        assert_eq!(paint(PixelGrid::Sixths, &[(0, 0)], &[(0, 0)]).ch, ' ');
        // The pixel under the pointer.
        assert_eq!(PixelGrid::Braille.pixel_at(3, 2, (0.9, 0.3)), (7, 9));
        assert_eq!(PixelGrid::Halves.pixel_at(3, 2, (0.9, 0.99)), (3, 5));
    }

    #[test]
    fn mirrored_brush_turns_glyphs() {
        let mut doc = Document::new(10, 4);
        let mut c = ctx('/', red(), NONE);
        c.mirror = Mirror { x: true, y: true };
        let stamps = c.mirror.stamps(vec![(1, 0, '/')], doc.width, doc.height);
        apply_stamps(&mut doc, &stamps, &c);
        let at = |x, y| doc.layers[0].get(x, y).unwrap().ch;
        assert_eq!((at(1, 0), at(8, 0), at(1, 3), at(8, 3)), ('/', '\\', '\\', '/'));
    }
}

mod glyph_sets {
    use crate::document::{Cell, Color, Document};
    use crate::export::render_png;

    /// Bright pixels of a one-cell PNG as (left half, right half, top third, rest).
    fn lit(ch: char) -> (usize, usize, usize, usize) {
        let mut doc = Document::new(1, 1);
        doc.layers[0].set(0, 0, Cell::new(ch, Color::WHITE, Color::TRANSPARENT));
        let img = render_png(&doc, false).unwrap();
        let (w, h) = img.dimensions();
        let mut counts = (0, 0, 0, 0);
        for (x, y, p) in img.enumerate_pixels() {
            if p[0] < 128 { continue; }
            if x < w / 2 { counts.0 += 1 } else { counts.1 += 1 }
            // A sextant row is a rounded third of the cell.
            if (y as f32) < (h as f32 / 3.0).round() { counts.2 += 1 } else { counts.3 += 1 }
        }
        counts
    }

    #[test]
    fn braille_and_sextants_export_as_shapes() {
        // The font has neither; drawn from the font they'd be "missing glyph" boxes.
        let (left, right, top, rest) = lit('⠁');
        assert!(left > 0 && right == 0 && top > 0 && rest == 0, "{:?}", (left, right, top, rest));
        let (left, right, top, rest) = lit('\u{1FB00}');
        assert!(left > 0 && right == 0 && top > 0 && rest == 0, "{:?}", (left, right, top, rest));
    }

    #[test]
    fn palettes_have_their_sizes() {
        let sizes: Vec<usize> = crate::library::builtin_palettes().iter().map(|p| p.colors.len()).collect();
        assert_eq!(&sizes[..4], &[16, 256, 16, 4]);
    }
}

#[test]
fn neighbouring_cells_share_their_edges_at_any_zoom() {
    use crate::ui::{canvas::CanvasState, glyph_render::snap_rect};
    use eframe::egui::{pos2, vec2};
    for step in 0..100 {
        let mut state = CanvasState::default();
        state.zoom = 0.2 + step as f32 * 0.0731;
        state.offset = vec2(step as f32 * 3.7, step as f32 * 1.3);
        let origin = pos2(20.0, 20.0);
        for ppp in [1.0, 1.25, 2.0] {
            for x in 0..200 {
                let a = snap_rect(state.cell_rect(x, x, origin), ppp);
                let b = snap_rect(state.cell_rect(x + 1, x + 1, origin), ppp);
                assert_eq!((a.max.x, a.max.y), (b.min.x, b.min.y), "zoom {} cell {x}", state.zoom);
            }
        }
    }
}

mod banners {
    use crate::figlet::fonts;

    #[test]
    fn every_bundled_font_loads() {
        let names: Vec<&str> = fonts().iter().map(|f| f.name).collect();
        assert_eq!(names, ["Standard", "Small", "Slant", "Big", "Shadow", "Mini", "Banner"]);
    }

    /// Expected rows come from  (fitted letters) with the same font files.
    #[test]
    fn matches_figlet() {
        let cases: &[(&str, &str, &[&str])] = &[
        ("Standard", "Hi", &[" _   _  _", "| | | |(_)", "| |_| || |", "|  _  || |", "|_| |_||_|"]),
        ("Standard", "Void 42!", &["__     __      _      _   _  _   ____   _", "\\ \\   / /___  (_)  __| | | || | |___ \\ | |", " \\ \\ / // _ \\ | | / _` | | || |_  __) || |", "  \\ V /| (_) || || (_| | |__   _|/ __/ |_|", "   \\_/  \\___/ |_| \\__,_|    |_| |_____|(_)"]),
        ("Slant", "Hi", &["    __  __ _", "   / / / /(_)", "  / /_/ // /", " / __  // /", "/_/ /_//_/"]),
        ("Slant", "Void 42!", &[" _    __        _      __   __ __  ___    __", "| |  / /____   (_)____/ /  / // / |__ \\  / /", "| | / // __ \\ / // __  /  / // /_ __/ / / /", "| |/ // /_/ // // /_/ /  /__  __// __/ /_/", "|___/ \\____//_/ \\__,_/     /_/  /____/(_)"]),
        ("Small", "Hi", &[" _  _  _", "| || |(_)", "| __ || |", "|_||_||_|"]),
        ("Small", "Void 42!", &["__   __     _     _   _ _  ___  _", "\\ \\ / /___ (_) __| | | | ||_  )| |", " \\ V // _ \\| |/ _` | |_  _|/ / |_|", "  \\_/ \\___/|_|\\__,_|   |_|/___|(_)"]),
        ("Big", "Hi", &[" _    _  _", "| |  | |(_)", "| |__| | _", "|  __  || |", "| |  | || |", "|_|  |_||_|"]),
        ("Big", "Void 42!", &["__      __     _      _   _  _  ___   _", "\\ \\    / /    (_)    | | | || ||__ \\ | |", " \\ \\  / /___   _   __| | | || |_  ) || |", "  \\ \\/ // _ \\ | | / _` | |__   _|/ / | |", "   \\  /| (_) || || (_| |    | | / /_ |_|", "    \\/  \\___/ |_| \\__,_|    |_||____|(_)"]),
        ("Mini", "Hi", &["", "|_| o", "| | |"]),
        ("Mini", "Void 42!", &["                    _", "\\  / _  o  _|   |_|_ ) |", " \\/ (_) | (_|     | /_ o"]),
        ];
        for (font, text, expected) in cases {
            let font = fonts().iter().find(|f| f.name == *font).unwrap();
            assert_eq!(font.render(text), *expected, "{} {text:?}", font.name);
        }
    }
}

mod replacing {
    use super::{NONE, blue, red};
    use crate::app::replace::{CellPattern, replace_changes};
    use crate::document::{Cell, Layer};

    fn layer() -> Layer {
        let mut layer = Layer::new(4, 1, "L");
        layer.set(0, 0, Cell::new('#', red(), NONE));
        layer.set(1, 0, Cell::new('#', blue(), NONE));
        layer.set(2, 0, Cell::new('.', red(), NONE));
        layer.set(3, 0, Cell::new(' ', red(), NONE));
        layer
    }

    const ALL: (usize, usize, usize, usize) = (0, 0, 4, 1);

    #[test]
    fn finds_by_glyph_or_colour() {
        let by_glyph = CellPattern { glyph: Some('#'), fg: None, bg: None };
        let to_block = CellPattern { glyph: Some('█'), fg: None, bg: None };
        assert_eq!(replace_changes(&layer(), ALL, &by_glyph, &to_block).len(), 2);

        let by_red = CellPattern { glyph: None, fg: Some(red()), bg: None };
        let blue_back = CellPattern { glyph: None, fg: None, bg: Some(blue()) };
        let changes = replace_changes(&layer(), ALL, &by_red, &blue_back);
        let mut cells: Vec<_> = changes.keys().copied().collect();
        cells.sort();
        assert_eq!(cells, [(0, 0), (2, 0)], "an empty cell's foreground doesn't count");
    }

    #[test]
    fn stays_inside_the_region() {
        let by_glyph = CellPattern { glyph: Some('#'), fg: None, bg: None };
        let to_block = CellPattern { glyph: Some('█'), fg: None, bg: None };
        assert_eq!(replace_changes(&layer(), (1, 0, 4, 1), &by_glyph, &to_block).len(), 1);
    }
}

mod sharing {
    use super::{NONE, blue, red};
    use crate::document::{Cell, Document};
    use crate::export::{to_ansi, to_html, whole};

    #[test]
    fn ansi_emits_colours_only_when_they_change() {
        let mut doc = Document::new(4, 2);
        doc.layers[0].set(0, 0, Cell::new('A', red(), NONE));
        doc.layers[0].set(1, 0, Cell::new('B', red(), NONE));
        doc.layers[0].set(2, 0, Cell::new(' ', red(), blue()));
        assert_eq!(to_ansi(&doc, whole(&doc)), "\x1b[38;2;255;0;0mAB\x1b[48;2;0;0;255m \x1b[0m\n");
    }

    #[test]
    fn html_escapes_and_colours_runs() {
        let mut doc = Document::new(3, 1);
        doc.layers[0].set(0, 0, Cell::new('<', red(), NONE));
        doc.layers[0].set(1, 0, Cell::new('&', red(), NONE));
        let html = to_html(&doc, "a<b");
        assert!(html.contains("<title>a&lt;b</title>"));
        assert!(html.contains("<span style=\"color:#ff0000;\">&lt;&amp;</span> \n"), "{html}");
    }
}

mod image_convert {
    use image::{Rgba, RgbaImage};
    use crate::convert::{ConvertMode, ConvertSettings, convert};

    fn settings(mode: ConvertMode, width: usize) -> ConvertSettings {
        ConvertSettings { width, mode, dither: false, ..ConvertSettings::default() }
    }

    #[test]
    fn ascii_follows_brightness() {
        let img = RgbaImage::from_fn(64, 32, |x, _| { let v = (x * 255 / 63) as u8; Rgba([v, v, v, 255]) });
        let cells = convert(&img, &settings(ConvertMode::Ascii, 16));
        assert_eq!((cells[0].len(), cells.len()), (16, 4), "rows keep the 2:1 cell shape");
        assert!(cells[2][0].is_empty());
        assert_eq!(cells[2][15].ch, '@');
    }

    #[test]
    fn half_blocks_keep_two_colours_per_cell() {
        let img = RgbaImage::from_fn(2, 2, |_, y| if y == 0 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) });
        let cell = &convert(&img, &settings(ConvertMode::HalfBlocks, 1))[0][0];
        assert_eq!((cell.ch, cell.fg.r, cell.bg.b), ('▀', 255, 255));
    }

    #[test]
    fn braille_and_transparency() {
        let white = RgbaImage::from_pixel(4, 8, Rgba([255, 255, 255, 255]));
        assert!(convert(&white, &settings(ConvertMode::Braille, 2)).iter().flatten().all(|c| c.ch == '⣿'));
        let clear = RgbaImage::from_pixel(8, 8, Rgba([255, 255, 255, 0]));
        assert!(convert(&clear, &settings(ConvertMode::Ascii, 4)).iter().flatten().all(|c| c.is_empty()));
    }

    #[test]
    fn dithering_keeps_mid_grey_half_lit() {
        let grey = RgbaImage::from_pixel(80, 160, Rgba([128, 128, 128, 255]));
        let s = ConvertSettings { dither: true, ..settings(ConvertMode::Braille, 40) };
        let dots: u32 = convert(&grey, &s).iter().flatten().map(|c| (c.ch as u32).saturating_sub(0x2800).count_ones()).sum();
        let total = 40 * 40 * 8;
        assert!((dots as f32 / total as f32 - 0.5).abs() < 0.05, "{dots} of {total} dots lit");
    }
}

mod shade_brush {
    use super::{NONE, ctx, red};
    use crate::document::Document;
    use crate::tools::shade::ShadeTool;

    #[test]
    fn strokes_step_density_once_per_stroke() {
        let blocks = " ░▒▓█";
        let mut doc = Document::new(5, 1);
        let mut tool = ShadeTool::default();
        let at = |doc: &Document| doc.layers[0].get(2, 0).unwrap().ch;
        let darker = ctx('#', red(), NONE);

        tool.on_press(&mut doc, 2, 0, &darker, blocks);
        tool.on_drag(&mut doc, 3, 0, &darker, blocks);
        tool.on_drag(&mut doc, 2, 0, &darker, blocks);
        tool.on_release();
        assert_eq!(at(&doc), '░', "going back over a cell in the same stroke doesn't stack");

        tool.on_press(&mut doc, 2, 0, &darker, blocks);
        tool.on_release();
        assert_eq!(at(&doc), '▒');

        let mut lighter = darker.clone();
        lighter.is_primary = false;
        tool.on_press(&mut doc, 2, 0, &lighter, blocks);
        tool.on_release();
        assert_eq!(at(&doc), '░');
    }

    #[test]
    fn ascii_ramp_tops_out() {
        let mut doc = Document::new(1, 1);
        doc.layers[0].set(0, 0, super::glyph('%'));
        let mut tool = ShadeTool::default();
        for _ in 0..3 {
            tool.on_press(&mut doc, 0, 0, &ctx('#', red(), NONE), " .:-=+*#%@");
            tool.on_release();
        }
        assert_eq!(doc.layers[0].get(0, 0).unwrap().ch, '@');
    }
}

mod command_search {
    use crate::app::commands::fuzzy_score;

    #[test]
    fn letters_must_appear_in_order() {
        assert!(fuzzy_score("grd", "Tool: Gradient").is_some());
        assert!(fuzzy_score("dg", "Tool: Gradient").is_none());
        assert!(fuzzy_score("GRAD", "tool: gradient").is_some(), "case doesn't matter");
    }

    #[test]
    fn word_starts_and_runs_rank_higher() {
        let score = |q, t| fuzzy_score(q, t).unwrap();
        assert!(score("md", "Layer: Merge Down") > score("md", "Edit: Command Palette"));
        assert!(score("pal", "Palette: VGA") > score("pal", "Edit: Copy As Plain Text"));
    }

    #[test]
    fn every_entry_is_unique() {
        let app = crate::app::AsciiArtEditor::default();
        let entries = app.actions();
        let mut keys: Vec<String> = entries.iter().map(|e| format!("{}: {}", e.group, e.name)).collect();
        let count = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), count, "the palette remembers commands by their text");
        let matches = app.palette_matches(&entries, "exp png");
        assert_eq!(entries[matches[0]].name, "Export PNG…");
    }
}

mod animation {
    use super::{glyph, red, NONE};
    use crate::document::{Cell, Document, Layer};
    use crate::history::{Command, History};

    fn at(doc: &Document, frame: usize, x: usize) -> char {
        doc.composite_in(frame, x, 0).ch
    }

    /// Three frames of a 3×1 picture: "a", "b", "c" in the first cell.
    fn three_frames(history: &mut History) -> Document {
        let mut doc = Document::new(3, 1);
        doc.layers[0].set(0, 0, glyph('a'));
        for ch in ['b', 'c'] {
            let index = doc.frame + 1;
            history.execute(Command::FrameAdd { index, data: doc.blank_frame() }, &mut doc);
            doc.layers[0].set(0, 0, glyph(ch));
        }
        doc
    }

    #[test]
    fn frames_keep_their_own_cells() {
        let mut history = History::new(50);
        let mut doc = three_frames(&mut history);
        assert_eq!((doc.frame_count(), doc.frame), (3, 2), "a new frame is shown");
        assert_eq!([at(&doc, 0, 0), at(&doc, 1, 0), at(&doc, 2, 0)], ['a', 'b', 'c']);
        doc.show_frame(0);
        assert_eq!(doc.layers[0].get(0, 0).unwrap().ch, 'a', "the shown frame is what tools edit");

        let data = doc.frame_data(0);
        history.execute(Command::FrameAdd { index: 1, data }, &mut doc);
        assert_eq!((doc.frame, at(&doc, 1, 0)), (1, 'a'), "a duplicate follows its original");

        history.execute(Command::FrameMove { from: 1, to: 3 }, &mut doc);
        assert_eq!((doc.frame, at(&doc, 3, 0), at(&doc, 1, 0)), (3, 'a', 'b'), "the moved frame stays shown");

        let data = doc.frame_data(3);
        history.execute(Command::FrameRemove { index: 3, data }, &mut doc);
        assert_eq!((doc.frame_count(), doc.frame), (3, 2), "a neighbour is shown instead");

        while history.undo(&mut doc) {}
        assert_eq!(doc.frame_count(), 1);
        assert!(doc.layers.iter().all(Layer::is_valid));
        assert_eq!(at(&doc, 0, 0), 'a');
    }

    #[test]
    fn undo_goes_back_to_the_frame_drawn_on() {
        let mut history = History::new(50);
        let mut doc = three_frames(&mut history);
        doc.show_frame(1);
        let changes = crate::document::Changes::from([((2, 0), (Cell::empty(), glyph('!')))]);
        doc.layers[0].set(2, 0, glyph('!'));
        history.push(Command::Draw { layer_idx: 0, frame: 1, changes });
        doc.show_frame(0);

        history.undo(&mut doc);
        assert_eq!(doc.frame, 1);
        assert_eq!(at(&doc, 1, 2), ' ');
        doc.show_frame(2);
        history.redo(&mut doc);
        assert_eq!((doc.frame, at(&doc, 1, 2)), (1, '!'));
    }

    #[test]
    fn layer_changes_reach_every_frame() {
        let mut history = History::new(50);
        let mut doc = three_frames(&mut history);
        history.execute(Command::LayerAdd { index: 1, layer: Layer::new(3, 1, "Top") }, &mut doc);
        doc.layers[1].set(1, 0, Cell::new('T', red(), NONE));
        assert!(doc.layers[1].is_valid());
        assert_eq!((at(&doc, 2, 1), at(&doc, 0, 1)), ('T', ' '), "a new layer starts blank in the other frames");

        // Merge down bakes every frame.
        doc.show_frame(0);
        doc.layers[1].set(2, 0, glyph('m'));
        let cmd = Command::LayerMergeDown { upper: 1, layer: Box::new(doc.layers[1].clone()), below: Box::new(doc.layers[0].clone()), merged: Box::new(doc.merged_down(1)) };
        history.execute(cmd, &mut doc);
        assert_eq!(doc.layers.len(), 1);
        assert_eq!((at(&doc, 0, 2), at(&doc, 2, 1), at(&doc, 1, 0)), ('m', 'T', 'b'));

        // Deleting the layer while on one frame and undoing it on another restores every frame.
        doc.show_frame(2);
        history.execute(Command::LayerAdd { index: 1, layer: doc.layers[0].clone() }, &mut doc);
        let layer = doc.layers[0].clone();
        history.execute(Command::LayerRemove { index: 0, layer }, &mut doc);
        doc.show_frame(0);
        history.undo(&mut doc);
        assert_eq!(doc.frame, 0);
        assert_eq!(doc.layers[0].get(2, 0).unwrap().ch, 'm', "the shown frame's cells came back, not frame 2's");
        assert_eq!(doc.layers[0].cells_in(2)[1].ch, 'T');

        // Resizing too, and undo puts the cells back.
        let old_layers = doc.layers.clone();
        history.execute(Command::Resize { old_size: (3, 1), new_size: (2, 2), old_layers }, &mut doc);
        assert!(doc.layers.iter().all(Layer::is_valid));
        assert_eq!(doc.composite_in(2, 1, 0).ch, 'T');
        doc.show_frame(1);
        history.undo(&mut doc);
        assert!(doc.layers.iter().all(Layer::is_valid));
        assert_eq!((doc.width, at(&doc, 2, 2), at(&doc, 0, 2)), (3, ' ', 'm'));
    }

    #[test]
    fn files_keep_frames_and_still_pictures_keep_the_old_format() {
        let mut history = History::new(50);
        let mut doc = three_frames(&mut history);
        doc.durations[1] = 250;
        doc.show_frame(1);
        let json = serde_json::to_string(&doc).unwrap();
        let mut loaded: Document = serde_json::from_str(&json).unwrap();
        loaded.validate().unwrap();
        assert_eq!((loaded.frame_count(), loaded.frame, loaded.durations[1]), (3, 1, 250));
        assert_eq!([at(&loaded, 0, 0), at(&loaded, 1, 0), at(&loaded, 2, 0)], ['a', 'b', 'c']);

        let still = serde_json::to_string(&Document::new(2, 1)).unwrap();
        assert!(!still.contains("durations") && !still.contains("frames"), "{still}");
        let mut old: Document = serde_json::from_str(&still).unwrap();
        old.validate().unwrap();
        assert_eq!(old.frame_count(), 1);
    }

    #[test]
    fn gif_export_has_every_frame() {
        use image::AnimationDecoder;
        let mut history = History::new(50);
        let mut doc = three_frames(&mut history);
        doc.durations = vec![100, 200, 300];
        let path = std::env::temp_dir().join(format!("void-canvas-test-{}.gif", std::process::id()));
        crate::export::write_gif(&doc, &path).unwrap();
        let file = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
        let frames = image::codecs::gif::GifDecoder::new(file).unwrap().into_frames().collect_frames().unwrap();
        std::fs::remove_file(&path).unwrap();
        let delays: Vec<u32> = frames.iter().map(|f| f.delay().numer_denom_ms().0 / f.delay().numer_denom_ms().1.max(1)).collect();
        assert_eq!(delays, vec![100, 200, 300]);
    }
}

mod scripts {
    use crate::document::{Color, Document};
    use crate::history::History;
    use crate::script::{EXAMPLES, ScriptContext, changes, run};

    fn context(doc: &Document) -> ScriptContext {
        ScriptContext {
            layer: 0,
            primary_char: '#',
            primary_fg: Color::WHITE,
            secondary_fg: Color::new(128, 128, 128, 255),
            area: (0, 0, doc.width, doc.height),
        }
    }

    fn row(doc: &Document, y: usize) -> String {
        (0..doc.width).map(|x| doc.layers[0].get(x, y).unwrap().ch).collect()
    }

    #[test]
    fn every_example_runs_and_draws() {
        for (name, code) in EXAMPLES {
            if code.contains("fn brush") {
                assert!(crate::script::check_brush(code).is_ok(), "{name}");
                continue;
            }
            let doc = Document::new(80, 40);
            let result = run(code, &doc, context(&doc)).unwrap_or_else(|(e, _)| panic!("{name}: {e}"));
            assert!(changes(&doc, &result.doc, 0).is_some(), "{name} changed nothing");
        }
    }

    #[test]
    fn the_api_writes_cells_inside_the_area() {
        let doc = Document::new(6, 3);
        let code = r##"
            fill(".", "#ff0000");
            set(0, 0, "A");
            set(1.7, 0, 'B', "#00ff00", "#0000ff");
            text(2, 1, "hi\nyo");
            set(-1, 0, "X"); set(99, 99, "X");
            print(get(0, 0) + width() + height());
        "##;
        let mut ctx = context(&doc);
        ctx.area = (1, 0, 5, 3);
        let result = run(code, &doc, ctx).unwrap_or_else(|(e, _)| panic!("{e}"));
        assert_eq!(row(&result.doc, 0), "AB... ");
        assert_eq!(row(&result.doc, 1), " .hi. ");
        assert_eq!(row(&result.doc, 2), " .yo. ");
        let b = result.doc.layers[0].get(1, 0).unwrap();
        assert_eq!((b.fg.g, b.bg.b), (255, 255));
        assert_eq!(result.doc.layers[0].get(4, 0).unwrap().fg, Color::new(255, 0, 0, 255));
        assert_eq!(result.log, vec!["A63"]);
    }

    #[test]
    fn a_seed_makes_random_output_repeatable() {
        let doc = Document::new(1, 1);
        let code = "seed(7); print(rand(1, 6)); print(rand()); print(noise(1.5, 2.5));";
        let first = run(code, &doc, context(&doc)).ok().unwrap().log;
        assert_eq!(run(code, &doc, context(&doc)).ok().unwrap().log, first);
    }

    #[test]
    fn errors_name_the_line_and_endless_loops_stop() {
        let doc = Document::new(2, 2);
        let (error, log) = run("print(\"before\");\nlet x = 1;\nnope(x);", &doc, context(&doc)).err().unwrap();
        assert!(error.starts_with("Line 3:"), "{error}");
        assert_eq!(log, vec!["before"]);
        assert!(run("loop { }", &doc, context(&doc)).is_err());
    }

    #[test]
    fn a_script_is_one_undo_step_even_across_frames() {
        let mut doc = Document::new(4, 1);
        let code = "set(0, 0, \"a\"); add_frame(); set(1, 0, \"b\"); set_duration(40); go_to(0); set(3, 0, \"z\");";
        let result = run(code, &doc, context(&doc)).unwrap_or_else(|(e, _)| panic!("{e}"));
        let mut history = History::new(10);
        history.execute(changes(&doc, &result.doc, 0).unwrap(), &mut doc);
        assert_eq!(doc.frame_count(), 2);
        assert_eq!((doc.durations[1], doc.composite_in(0, 3, 0).ch, doc.composite_in(1, 1, 0).ch), (40, 'z', 'b'));
        history.undo(&mut doc);
        assert_eq!(doc.frame_count(), 1);
        assert_eq!(row(&doc, 0), "    ");
        history.redo(&mut doc);
        assert_eq!((doc.frame_count(), doc.composite_in(1, 0, 0).ch), (2, 'a'));
    }
}

mod file_format {
    use crate::app::file_io::{document_bytes, parse_document};
    use crate::document::{Cell, Color, Document, FrameData, Layer, codec};

    /// A small deterministic random source for building test pictures.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// Patches and runs of a few glyphs and colours, like real art, plus noise.
    fn random_doc(rng: &mut Rng, w: usize, h: usize, layers: usize, frames: usize) -> Document {
        const GLYPHS: [char; 8] = [' ', '#', '█', '░', '╬', '⣿', '😀', '\u{10FFFF}'];
        let colors = [Color::WHITE, Color::TRANSPARENT, Color::new(255, 0, 0, 255), Color::new(1, 2, 3, 0), Color::new(9, 200, 77, 128)];
        let mut doc = Document::new(w, h);
        for l in 1..layers {
            doc.insert_layer(l, Layer::new(w, h, format!("Layer ü {l}")));
        }
        for _ in 1..frames {
            let data = doc.frame_data(doc.frame);
            doc.insert_frame(doc.frame_count(), data);
        }
        for f in 0..frames {
            doc.show_frame(f);
            for layer in &mut doc.layers {
                for _ in 0..(w * h / 3).max(1) {
                    let (x, y) = (rng.below(w), rng.below(h));
                    let run = 1 + rng.below(6);
                    let cell = Cell::new(GLYPHS[rng.below(8)], colors[rng.below(5)].clone(), colors[rng.below(5)].clone());
                    for dx in 0..run {
                        layer.set((x + dx).min(w - 1), y, cell.clone());
                    }
                }
            }
        }
        doc.layers[0].visible = false;
        doc.layers[0].blend = crate::document::Blend::Overlay;
        doc.layers[layers - 1].blend = crate::document::Blend::Combine;
        doc.layers[layers - 1].locked = true;
        doc.layers[layers - 1].opacity = 0.37;
        doc.durations = (0..frames).map(|f| [100, 100, 40, 2500][f % 4]).collect();
        doc.show_frame(frames / 2);
        doc.active_layer = layers / 2;
        doc
    }

    fn round_trip(doc: &Document) -> Document {
        // Both ways of writing colours must decode, whichever the encoder would pick.
        for predicted in [false, true] {
            let back = codec::decode(&codec::encode_with(doc, predicted)).unwrap_or_else(|e| panic!("{e}"));
            assert!(codec::same_content(doc, &back), "predicted colours: {predicted}");
        }
        let bytes = codec::encode(doc);
        let back = codec::decode(&bytes).unwrap_or_else(|e| panic!("{e}"));
        assert!(codec::same_content(doc, &back), "the decoded document differs");
        back
    }

    #[test]
    fn the_demo_round_trips_and_shrinks() {
        let doc = parse_document(&std::fs::read("examples/demo.vc").unwrap()).unwrap();
        round_trip(&doc);
        let json = serde_json::to_vec_pretty(&doc).unwrap().len();
        let (bytes, compressed) = document_bytes(&doc).unwrap();
        println!("demo.vc: JSON {json} bytes, compressed {} bytes", bytes.len());
        assert!(compressed);
        assert!(bytes.len() < 2328, "beats xz -9e on the JSON ({} bytes)", bytes.len());
    }

    #[test]
    fn random_pictures_round_trip() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for (w, h, layers, frames) in [(1, 1, 1, 1), (1, 9, 1, 2), (9, 1, 2, 1), (13, 7, 3, 4), (40, 20, 2, 3)] {
            let doc = random_doc(&mut rng, w, h, layers, frames);
            let back = round_trip(&doc);
            assert_eq!((back.frame, back.active_layer, back.layers[2.min(layers - 1)].name.clone()), (doc.frame, doc.active_layer, doc.layers[2.min(layers - 1)].name.clone()));
        }
        // Frames that mostly repeat the one before cost little.
        let mut doc = random_doc(&mut rng, 40, 20, 1, 1);
        let still = codec::encode(&doc).len();
        for _ in 0..9 {
            let mut data: FrameData = doc.frame_data(doc.frame);
            data.cells[0][rng.below(800)] = Cell::new('*', Color::WHITE, Color::TRANSPARENT);
            doc.insert_frame(doc.frame_count(), data);
        }
        round_trip(&doc);
        let animated = codec::encode(&doc).len();
        assert!(animated < still + 9 * 12, "ten nearly equal frames: {animated} bytes vs {still} for one");
    }

    #[test]
    fn damage_is_reported_not_trusted() {
        let mut rng = Rng(42);
        let doc = random_doc(&mut rng, 12, 6, 2, 2);
        let bytes = codec::encode(&doc);
        let mut flipped = bytes.clone();
        flipped[bytes.len() / 2] ^= 0x10;
        assert!(codec::decode(&flipped).unwrap_err().contains("damaged"));
        assert!(codec::decode(&bytes[..bytes.len() - 7]).is_err());
        assert!(codec::decode(b"VOIDCANV").is_err());
        let mut newer = bytes.clone();
        newer[8] = 99;
        assert!(codec::decode(&newer).unwrap_err().contains("newer"));
        assert!(parse_document(b"\xff\xfe nonsense").is_err());

        // Garbage that passes the checksum must still fail cleanly, never panic.
        for round in 0..300 {
            let mut junk = bytes[..bytes.len() - 4].to_vec();
            for _ in 0..1 + round % 5 {
                let i = 9 + rng.below(junk.len() - 9);
                junk[i] = rng.next() as u8;
            }
            if round % 7 == 0 {
                junk.truncate(9 + rng.below(junk.len() - 9));
            }
            let crc = crc32fast::hash(&junk);
            junk.extend(crc.to_le_bytes());
            let _ = codec::decode(&junk);
        }
    }

    #[test]
    fn format_2_files_still_open() {
        let bytes = std::fs::read("examples/demo.vc").unwrap();
        assert_eq!(bytes[8], 2, "the demo was saved by 2.0");
        let doc = parse_document(&bytes).unwrap();
        assert!(doc.layers.iter().all(|l| l.blend == crate::document::Blend::Normal));
    }

    #[test]
    fn json_files_from_before_still_open() {
        let mut doc = Document::new(3, 2);
        doc.layers[0].set(1, 1, Cell::new('J', Color::WHITE, Color::TRANSPARENT));
        let json = serde_json::to_vec_pretty(&doc).unwrap();
        let loaded = parse_document(&json).unwrap();
        assert!(codec::same_content(&doc, &loaded));
    }
}

mod blending {
    use super::{NONE, blue, glyph, red};
    use crate::document::blend::combine;
    use crate::document::{Blend, Cell, Color, Document, Layer, Stack};
    use crate::history::{Command, History};

    #[test]
    fn glyphs_combine_into_real_characters() {
        let pairs = [
            ('│', '─', Some('┼')), ('┌', '┘', Some('┼')), ('│', '═', Some('╪')), ('╴', '╶', Some('─')),
            ('⠁', '⠈', Some('⠉')), ('▀', '▄', Some('█')), ('▌', '▐', Some('█')), ('▘', '▗', Some('▚')),
            ('🬀', '🬁', Some('🬂')), ('🬐', '🬃', Some('▌')), ('-', '|', Some('+')), ('\\', '/', Some('X')),
            ('#', '#', Some('#')), ('▀', '🬭', None), ('a', 'b', None), ('│', '⠁', None),
        ];
        for (below, top, want) in pairs {
            assert_eq!(combine(below, top), want, "{below} + {top}");
        }
    }

    fn stack(cells: &[(&Cell, Blend, f32)]) -> Stack {
        let mut stack = Stack::new();
        for (cell, blend, opacity) in cells {
            stack.push(cell, *blend, *opacity);
        }
        stack
    }

    #[test]
    fn blend_modes_decide_what_a_glyph_does_to_the_one_below() {
        let (line, cross) = (Cell::new('│', red(), NONE), Cell::new('─', blue(), NONE));
        let normal = stack(&[(&line, Blend::Normal, 1.0), (&cross, Blend::Normal, 1.0)]);
        assert_eq!(normal.glyphs, vec![('─', blue())]);
        let combined = stack(&[(&line, Blend::Normal, 1.0), (&cross, Blend::Combine, 1.0)]);
        assert_eq!(combined.glyphs, vec![('┼', blue())], "one real glyph, in the top colour");
        let letter = Cell::new('a', blue(), NONE);
        let fallback = stack(&[(&line, Blend::Normal, 1.0), (&letter, Blend::Combine, 1.0)]);
        assert_eq!(fallback.glyphs, vec![('a', blue())], "nothing to combine: it replaces");
        let overlaid = stack(&[(&line, Blend::Normal, 1.0), (&letter, Blend::Overlay, 1.0)]);
        assert_eq!(overlaid.glyphs, vec![('│', red()), ('a', blue())]);
        assert_eq!(overlaid.top().ch, 'a', "text keeps the top glyph");
    }

    #[test]
    fn opacity_fades_and_backgrounds_blend() {
        let below = Cell::new(' ', Color::WHITE, Color::new(0, 0, 255, 255));
        let above = Cell::new('x', Color::WHITE, Color::new(255, 0, 0, 255));
        let half = stack(&[(&below, Blend::Normal, 1.0), (&above, Blend::Normal, 0.5)]);
        assert_eq!(half.bg, Color::new(128, 0, 127, 255));
        assert_eq!(half.glyphs, vec![('x', Color::new(255, 255, 255, 128))]);
        let none = stack(&[(&glyph('o'), Blend::Normal, 1.0), (&above, Blend::Normal, 0.0)]);
        assert_eq!(none.top(), glyph('o'), "a layer at 0 % changes nothing");
        // A see-through background lets the one below show through.
        let tint = Cell::new(' ', Color::WHITE, Color::new(255, 0, 0, 128));
        assert_eq!(stack(&[(&below, Blend::Normal, 1.0), (&tint, Blend::Normal, 1.0)]).bg, Color::new(128, 0, 127, 255));
    }

    fn two_layers(blend: Blend) -> Document {
        let mut doc = Document::new(2, 1);
        doc.layers[0].set(0, 0, glyph('│'));
        doc.layers[0].set(1, 0, glyph('o'));
        let mut top = Layer::new(2, 1, "Top");
        top.blend = blend;
        top.set(0, 0, glyph('─'));
        top.set(1, 0, glyph('x'));
        doc.insert_layer(1, top);
        doc
    }

    #[test]
    fn merging_down_bakes_the_blend_mode() {
        for (blend, want, overlaid) in [(Blend::Normal, "─x", false), (Blend::Combine, "┼x", false), (Blend::Overlay, "─x", true)] {
            let doc = two_layers(blend);
            assert_eq!(doc.has_overlaid_glyphs(0), overlaid);
            let merged = doc.merged_down(1);
            let row: String = (0..2).map(|x| merged.get(x, 0).unwrap().ch).collect();
            assert_eq!(row, want, "{blend:?}");
        }
        let doc = two_layers(Blend::Combine);
        assert_eq!(crate::export::to_text(&doc, crate::export::whole(&doc)), "┼x", "combined glyphs are real text");
    }

    #[test]
    fn style_changes_undo() {
        let mut doc = two_layers(Blend::Normal);
        let mut history = History::new(10);
        history.execute(Command::LayerStyle { index: 1, old: (Blend::Normal, 1.0), new: (Blend::Overlay, 0.4) }, &mut doc);
        assert_eq!((doc.layers[1].blend, doc.layers[1].opacity), (Blend::Overlay, 0.4));
        history.undo(&mut doc);
        assert_eq!((doc.layers[1].blend, doc.layers[1].opacity), (Blend::Normal, 1.0));
    }
}

mod brushes {
    use super::{NONE, ctx, glyph, red};
    use crate::document::{Cell, Document};
    use crate::tools::brush::{BrushKind, BrushTool, direction};

    fn row(doc: &Document, y: usize) -> String {
        (0..doc.width).map(|x| doc.layers[0].get(x, y).unwrap().ch).collect()
    }

    #[test]
    fn directions_follow_the_stroke_as_it_looks() {
        // Cells are twice as tall as wide: two across and one down is a 45° diagonal.
        for (way, want) in [((1, 0), 0), ((-3, 0), 0), ((0, 1), 1), ((1, -3), 1), ((2, -1), 2), ((-2, 1), 2), ((2, 1), 3), ((-2, -1), 3), ((5, 1), 0)] {
            assert_eq!(direction(way), want, "{way:?}");
        }
    }

    #[test]
    fn scatter_is_stable_within_a_stroke_and_previews_truly() {
        let mut doc = Document::new(12, 6);
        let brush = BrushKind::Scatter { glyphs: "*+".into(), density: 0.5 };
        let mut c = ctx('#', red(), NONE);
        c.brush_size = 5;
        let mut tool = BrushTool::default();
        let preview = tool.preview(&doc, 5, 3, &brush, &c);
        tool.on_press(&mut doc, 5, 3, &brush, &c);
        let first = (0..6).map(|y| row(&doc, y)).collect::<Vec<_>>();
        assert!(preview.iter().all(|(x, y, cell)| doc.layers[0].get(*x, *y) == Some(cell)), "the preview showed this stroke");
        tool.on_drag(&mut doc, 6, 3, &brush, &c);
        tool.on_drag(&mut doc, 5, 3, &brush, &c);
        let changes = tool.on_release();
        assert_eq!((0..6).map(|y| row(&doc, y)).collect::<Vec<_>>()[1..5].iter().map(|r| &r[3..8]).collect::<Vec<_>>(),
                   first[1..5].iter().map(|r| &r[3..8]).collect::<Vec<_>>(), "going back over a cell doesn't redraw it");
        let placed = changes.len();
        assert!(placed > 3 && placed < 30, "about half of the cells: {placed}");
        // Full density covers the square.
        let mut full = Document::new(5, 5);
        let mut tool = BrushTool::default();
        tool.on_press(&mut full, 2, 2, &BrushKind::Scatter { glyphs: "o".into(), density: 1.0 }, &{ let mut c = c.clone(); c.brush_size = 3; c });
        assert_eq!((0..5).map(|y| row(&full, y)).collect::<String>(), "      ooo  ooo  ooo      ");
    }

    #[test]
    fn direction_brush_draws_the_line_it_follows() {
        let mut doc = Document::new(8, 6);
        let brush = BrushKind::Direction { glyphs: "-|/\\".into() };
        let c = ctx('#', red(), NONE);
        let mut tool = BrushTool::default();
        tool.on_press(&mut doc, 1, 1, &brush, &c);
        tool.on_drag(&mut doc, 4, 1, &brush, &c);
        tool.on_drag(&mut doc, 4, 4, &brush, &c);
        tool.on_release();
        assert_eq!(row(&doc, 1), " ----   ");
        assert_eq!((2..5).map(|y| doc.layers[0].get(4, y).unwrap().ch).collect::<String>(), "|||");
    }

    #[test]
    fn stamps_go_down_centred_without_their_empty_cells() {
        let mut doc = Document::new(7, 5);
        doc.layers[0].set(3, 2, glyph('.'));
        let block = vec![vec![Cell::new('A', red(), NONE), Cell::empty()], vec![Cell::new('B', red(), NONE), Cell::new('C', red(), NONE)]];
        let mut tool = BrushTool::default();
        let c = ctx('#', crate::document::Color::WHITE, NONE);
        tool.on_press(&mut doc, 3, 2, &BrushKind::Stamp { cells: block.clone(), recolor: false }, &c);
        tool.on_release();
        assert_eq!(row(&doc, 1), "  A    ");
        assert_eq!(row(&doc, 2), "  BC   ", "the stamp's cells replace what was under them");
        assert_eq!(doc.layers[0].get(2, 1).unwrap().fg, red(), "own colours");
        let mut doc = Document::new(7, 5);
        tool.on_press(&mut doc, 3, 2, &BrushKind::Stamp { cells: block, recolor: true }, &c);
        tool.on_release();
        assert_eq!(doc.layers[0].get(2, 1).unwrap().fg, crate::document::Color::WHITE, "slot colours");
        assert_eq!(doc.layers[0].get(3, 1).unwrap().ch, ' ', "empty stamp cells leave the canvas alone");
    }
}

mod script_brushes {
    use super::{NONE, ctx, glyph, red};
    use crate::document::Document;
    use crate::script::{GRASS_BRUSH, RAINBOW_BRUSH, check_brush};
    use crate::tools::brush::{BrushKind, BrushTool};

    fn stroke(doc: &mut Document, code: &str, points: &[(usize, usize)]) -> (BrushTool, usize) {
        let brush = BrushKind::Script { code: code.into() };
        let c = ctx('#', red(), NONE);
        let mut tool = BrushTool::default();
        tool.on_press(doc, points[0].0, points[0].1, &brush, &c);
        for &(x, y) in &points[1..] {
            tool.on_drag(doc, x, y, &brush, &c);
        }
        let changed = tool.on_release().len();
        (tool, changed)
    }

    fn row(doc: &Document, y: usize) -> String {
        (0..doc.width).map(|x| doc.layers[0].get(x, y).unwrap().ch).collect()
    }

    #[test]
    fn script_brushes_draw_along_the_stroke() {
        let mut doc = Document::new(8, 3);
        let (_, changed) = stroke(&mut doc, RAINBOW_BRUSH, &[(1, 1), (5, 1)]);
        assert_eq!(row(&doc, 1), " █████  ");
        assert_eq!(changed, 5, "one undo step with every cell");
        assert_ne!(doc.layers[0].get(1, 1).unwrap().fg, doc.layers[0].get(2, 1).unwrap().fg, "a rainbow");

        let mut doc = Document::new(6, 6);
        let way = r#"fn brush(x, y, dx, dy) { set(x, y, if dx != 0 { "-" } else if dy != 0 { "|" } else { "o" }); }"#;
        stroke(&mut doc, way, &[(1, 1), (4, 1), (4, 4)]);
        assert_eq!(row(&doc, 1), " o--- ");
        assert_eq!((2..5).map(|y| doc.layers[0].get(4, y).unwrap().ch).collect::<String>(), "|||");
    }

    #[test]
    fn grass_grows_up_and_leaves_art_alone() {
        let mut doc = Document::new(10, 6);
        doc.layers[0].set(3, 3, glyph('@'));
        stroke(&mut doc, GRASS_BRUSH, &[(0, 4), (9, 4)]);
        assert!(row(&doc, 4).chars().all(|c| "|/\\'".contains(c)), "a blade on every cell: {}", row(&doc, 4));
        assert_eq!(doc.layers[0].get(3, 3).unwrap().ch, '@');
        assert_eq!(row(&doc, 0), "          ", "at most three tall");
    }

    #[test]
    fn broken_brushes_say_why() {
        let mut doc = Document::new(6, 2);
        let failing = r##"fn brush(x, y, dx, dy) { if x > 2 { nope(); } set(x, y, "#"); }"##;
        let (mut tool, changed) = stroke(&mut doc, failing, &[(0, 0), (5, 0)]);
        assert_eq!(row(&doc, 0), "###   ", "it stops at the error and keeps what it drew");
        assert_eq!(changed, 3);
        assert!(tool.error.take().unwrap().contains("nope"));
        assert!(check_brush("set(0, 0, \"x\");").unwrap_err().contains("brush(x, y, dx, dy)"));
        assert!(check_brush("fn brush(x, y, dx, dy) {").unwrap_err().starts_with("Line"));
        assert!(check_brush(RAINBOW_BRUSH).is_ok());
    }
}

mod three_d {
    use crate::document::scene::{Object3D, Scene3D, Style3D};
    use crate::document::{Cell, Color, Document, Layer, codec};
    use crate::history::{Command, History};

    fn scene(object: Object3D, style: Style3D) -> Scene3D {
        Scene3D { object, style, ..Scene3D::default() }
    }

    fn glyphs(cells: &[Cell]) -> String {
        cells.iter().map(|c| c.ch).collect()
    }

    #[test]
    fn objects_render_in_every_style() {
        for object in [Object3D::Cube, Object3D::Sphere, Object3D::Torus, Object3D::Cylinder, Object3D::Cone] {
            for style in Style3D::ALL {
                let s = scene(object.clone(), style);
                let cells = s.render(24, 12, 0);
                assert_eq!(cells, s.render(24, 12, 0), "the same scene renders the same");
                let drawn: Vec<&Cell> = cells.iter().filter(|c| !c.is_empty()).collect();
                assert!(drawn.len() > 10, "{object:?} {style:?}: {}", glyphs(&cells));
                assert!(cells[0].is_empty(), "the corner stays empty");
                let fits = |c: &&Cell| match style {
                    Style3D::Shade => " .:-=+*#%@".contains(c.ch),
                    Style3D::Braille => ('\u{2800}'..='\u{28FF}').contains(&c.ch),
                    Style3D::Blocks => "▀▄█".contains(c.ch),
                };
                assert!(drawn.iter().all(fits), "{object:?} {style:?}");
            }
        }
        let cube = scene(Object3D::Cube, Style3D::Shade);
        let turned = Scene3D { turn: [0.0, 45.0, 0.0], ..cube.clone() };
        assert_ne!(cube.render(24, 12, 0), turned.render(24, 12, 0));
    }

    #[test]
    fn art_seen_face_on_covers_itself_exactly() {
        let mut app = crate::app::AsciiArtEditor::default();
        app.doc = Document::new(20, 10);
        for (x, y) in [(5, 3), (6, 3), (7, 3), (6, 4)] {
            app.doc.layers[0].set(x, y, Cell::new('#', Color::new(255, 0, 0, 255), Color::TRANSPARENT));
        }
        app.tools.selection.selection = Some((5, 3, 7, 4));
        let ctx = eframe::egui::Context::default();
        app.run(crate::app::commands::Action::New3DFromSelection, &ctx);
        assert!(app.three_d.is_some(), "the 3D window opens on the new layer");
        let layer = app.doc.layers.len() - 1;
        let mut s = app.doc.layers[layer].scene.as_deref().unwrap().clone();
        s.turn = [0.0; 3];
        let cells = s.render(20, 10, 0);
        let covered: Vec<(usize, usize)> = (0..200).filter(|i| !cells[*i].is_empty()).map(|i| (i % 20, i / 20)).collect();
        assert_eq!(covered, vec![(5, 3), (6, 3), (7, 3), (6, 4)]);
        assert_eq!(cells[3 * 20 + 6].ch, '█');
    }

    #[test]
    fn frames_turn_a_spinning_scene() {
        let mut doc = Document::new(24, 12);
        let mut layer = Layer::new(24, 12, "3D");
        layer.scene = Some(Box::new(Scene3D { spin: [0.0, 30.0, 0.0], ..scene(Object3D::Cube, Style3D::Shade) }));
        doc.insert_layer(1, layer);
        let data = doc.frame_data(0);
        doc.insert_frame(1, data);
        assert_eq!(doc.stale_3d(), vec![(1, 1), (1, 0)], "both frames wait for a render, the one in view first");
        doc.render_3d_now();
        let (a, b) = (doc.layers[1].cells_in(0).to_vec(), doc.layers[1].cells_in(1).to_vec());
        assert_ne!(a, b, "a new frame is rendered at its own angle, not copied");
        assert_eq!(b, doc.layers[1].scene.as_ref().unwrap().render(24, 12, 1));
        // Tools leave a 3D layer alone.
        assert!(!doc.layers[1].set(0, 0, Cell::new('x', Color::WHITE, Color::TRANSPARENT)));
    }

    #[test]
    fn scene_edits_undo_and_rasterizing_keeps_the_cells() {
        let mut doc = Document::new(24, 12);
        let mut layer = Layer::new(24, 12, "3D");
        layer.scene = Some(Box::new(scene(Object3D::Sphere, Style3D::Blocks)));
        doc.insert_layer(1, layer);
        doc.render_3d_now();
        let before = doc.layers[1].cells_in(0).to_vec();
        let mut history = History::new(10);
        let old = doc.layers[1].scene.clone();
        let new = Some(Box::new(scene(Object3D::Torus, Style3D::Blocks)));
        history.execute(Command::Scene { index: 1, old: old.clone(), new }, &mut doc);
        doc.render_3d_now();
        assert_ne!(doc.layers[1].cells_in(0), before.as_slice());
        history.undo(&mut doc);
        doc.render_3d_now();
        assert_eq!(doc.layers[1].cells_in(0), before.as_slice(), "the sphere is back");

        history.execute(Command::Scene { index: 1, old, new: None }, &mut doc);
        assert!(doc.layers[1].scene.is_none());
        assert_eq!(doc.layers[1].cells_in(0), before.as_slice(), "rasterized, it looks the same");
        assert!(doc.layers[1].set(0, 0, Cell::new('x', Color::WHITE, Color::TRANSPARENT)), "and can be painted");
    }

    #[test]
    fn merging_into_a_3d_layer_flattens_it_and_files_keep_scenes() {
        let mut doc = Document::new(24, 12);
        doc.layers[0].scene = Some(Box::new(scene(Object3D::Cone, Style3D::Braille)));
        doc.invalidate_3d();
        doc.render_3d_now();
        let mut top = Layer::new(24, 12, "Top");
        top.set(0, 0, Cell::new('x', Color::WHITE, Color::TRANSPARENT));
        doc.insert_layer(1, top.clone());
        let back = codec::decode(&codec::encode(&doc)).unwrap();
        assert!(codec::same_content(&doc, &back));
        assert_eq!(back.layers[0].scene, doc.layers[0].scene);

        let merged = doc.merged_down(1);
        assert!(merged.scene.is_none());
        assert_eq!(merged.get(0, 0).unwrap().ch, 'x');
    }

    #[test]
    fn the_background_catches_3d_layers_up() {
        let mut app = crate::app::AsciiArtEditor::default();
        let ctx = eframe::egui::Context::default();
        app.run(crate::app::commands::Action::New3D, &ctx);
        app.turntable(8);
        let layer = app.doc.active_layer;
        assert_eq!(app.doc.stale_3d().len(), 8);
        app.run_render_job(&ctx);
        assert!(app.render_job.is_some(), "a render job starts");
        // The scene changes while it runs: what it renders for the old one must not land.
        app.doc.layers[layer].scene.as_mut().unwrap().turn[0] = 70.0;
        app.doc.invalidate_3d();
        for _ in 0..4000 {
            app.run_render_job(&ctx);
            if app.render_job.is_none() && app.doc.stale_3d().is_empty() { break; }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(app.doc.stale_3d().is_empty(), "everything caught up");
        let scene = app.doc.layers[layer].scene.clone().unwrap();
        for frame in 0..8 {
            assert_eq!(app.doc.layers[layer].cells_in(frame), scene.render(app.doc.width, app.doc.height, frame).as_slice(), "frame {frame}");
        }
    }

    #[test]
    fn turntable_adds_frames_in_one_step() {
        let mut app = crate::app::AsciiArtEditor::default();
        let ctx = eframe::egui::Context::default();
        app.run(crate::app::commands::Action::New3D, &ctx);
        assert_eq!(app.history.labels().0, vec!["New 3D Layer"]);
        let layer = app.doc.active_layer;
        assert!(app.doc.layers[layer].scene.is_some());
        app.turntable(12);
        assert_eq!(app.doc.frame_count(), 12);
        assert_eq!(app.doc.layers[layer].scene.as_ref().unwrap().spin, [0.0, 30.0, 0.0]);
        assert_eq!(app.history.labels().0.last(), Some(&"Turntable"));
        app.history.undo(&mut app.doc);
        assert_eq!(app.doc.frame_count(), 1);
    }
}

mod library_pools {
    use crate::app::AsciiArtEditor;
    use crate::app::library::LibraryTab;
    use crate::library::builtin_ramps;

    #[test]
    fn built_in_ramps_are_all_different() {
        let ramps = builtin_ramps();
        for (i, a) in ramps.iter().enumerate() {
            assert!(a.glyphs.chars().count() > 1, "{}", a.name);
            assert!(ramps[i + 1..].iter().all(|b| b.glyphs != a.glyphs && b.name != a.name), "{} twice", a.name);
        }
    }

    #[test]
    fn entries_are_made_copied_moved_and_shared() {
        let mut app = AsciiArtEditor::default();
        let builtin = builtin_ramps().len();
        app.open_library(LibraryTab::Ramps);
        app.library_new(LibraryTab::Ramps, Some(1));
        assert_eq!(app.settings.ramps[0].name, "ASCII copy");
        assert_eq!(app.settings.ramps[0].glyphs, builtin_ramps()[1].glyphs);
        assert_eq!(app.library.as_ref().unwrap().selected, builtin, "the new entry is selected");
        app.library_new(LibraryTab::Ramps, Some(builtin));
        assert_eq!(app.settings.ramps[1].name, "ASCII copy copy");

        // Shared as text, added under a name of its own.
        let text = r#"{"ramp":{"name":"ASCII copy","glyphs":" -=#"}}"#;
        assert_eq!(app.library_add_shared(text), Ok(LibraryTab::Ramps));
        assert_eq!(app.settings.ramps[2].name, "ASCII copy 2");
        assert!(app.library_add_shared("hello").is_err());
        let palette = r##"{"palette":{"name":"Two","colors":[{"r":1,"g":2,"b":3,"a":255},{"r":0,"g":0,"b":0,"a":255}]}}"##;
        assert_eq!(app.library_add_shared(palette), Ok(LibraryTab::Palettes));
        assert_eq!(app.settings.all_palettes().last().unwrap().colors.len(), 2);
    }
}

mod obj_models {
    use crate::document::scene::{Object3D, Scene3D, Style3D, parse_obj};
    use crate::document::{Document, codec};

    const CUBE: &str = "# a cube\nv -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nvt 0 0\nvn 0 0 1\nf 1/1/1 2/1/1 3/1/1 4/1/1\nf 5 8 7 6\nf 1 5 6 2\nf -5 -1 -2 -6\nf 2 6 7 3\nf 1 4 8 5\n";

    #[test]
    fn obj_files_parse_into_triangles() {
        let (vertices, faces) = parse_obj(CUBE).unwrap();
        assert_eq!((vertices.len(), faces.len()), (8, 12), "six quads become twelve triangles");
        let reach = vertices.iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0f32, f32::max);
        assert!((reach - 0.98).abs() < 1e-4, "fits the unit sphere: {reach}");
        assert_eq!(faces[6], [3, 7, 6], "negative indices count back from the last vertex");

        assert!(parse_obj("v 0 0 0\n").unwrap_err().contains("no faces"));
        assert!(parse_obj("v 0 x 0\n").unwrap_err().contains("line 1"));
        assert!(parse_obj("v 0 0 0\nv 1 0 0\nf 1 2 3\n").unwrap_err().contains("no vertex 3"));
    }

    /// A square of `n` × `n` quads in the xy plane, as OBJ.
    fn square(n: usize) -> String {
        let mut obj = String::new();
        for y in 0..=n {
            for x in 0..=n {
                obj += &format!("v {} {} 0\n", x as f32 / n as f32 - 0.5, y as f32 / n as f32 - 0.5);
            }
        }
        for y in 0..n {
            for x in 0..n {
                let i = y * (n + 1) + x + 1;
                obj += &format!("f {} {} {} {}\n", i, i + 1, i + n + 2, i + n + 1);
            }
        }
        obj
    }

    #[test]
    fn the_tree_finds_every_triangle() {
        let scene = |n: usize| {
            let (vertices, faces) = parse_obj(&square(n)).unwrap();
            Scene3D { object: Object3D::Mesh { name: "square".into(), vertices, faces, smooth: false }, style: Style3D::Blocks, turn: [0.0; 3], ..Scene3D::default() }
        };
        let (many, two) = (scene(40).render(40, 20, 0), scene(1).render(40, 20, 0));
        assert!(many.iter().filter(|c| !c.is_empty()).count() > 100);
        assert_eq!(many, two, "3200 triangles in a tree show what 2 show");
    }

    #[test]
    fn opening_an_obj_file_makes_a_3d_layer() {
        let path = std::env::temp_dir().join(format!("void-canvas-test-{}.obj", std::process::id()));
        std::fs::write(&path, CUBE).unwrap();
        let mut app = crate::app::AsciiArtEditor::default();
        app.open_path(&path);
        std::fs::remove_file(&path).unwrap();
        let layer = app.doc.active_layer();
        assert!(matches!(&layer.scene.as_deref().unwrap().object, Object3D::Mesh { faces, .. } if faces.len() == 12));
        assert!(layer.name.starts_with("void-canvas-test"));
        app.history.undo(&mut app.doc);
        assert_eq!(app.doc.layers.len(), 1);

        let mut doc = Document::new(20, 10);
        let (vertices, faces) = parse_obj(CUBE).unwrap();
        doc.layers[0].scene = Some(Box::new(Scene3D { object: Object3D::Mesh { name: "cube".into(), vertices, faces, smooth: true }, ..Scene3D::default() }));
        let back = codec::decode(&codec::encode(&doc)).unwrap();
        assert_eq!(back.layers[0].scene, doc.layers[0].scene, "models are kept in files");
    }
}

mod gradients {
    use super::{NONE, blue, ctx, red};
    use crate::document::Document;
    use crate::tools::gradient::{GradientSettings, GradientShape, gradient};

    fn linear(doc: &Document) -> GradientSettings<'static> {
        GradientSettings { shape: GradientShape::Linear, ramp: " ░▒▓█", dither: false, region: (0, 0, doc.width, doc.height) }
    }

    /// The glyphs of row `y` after a gradient.
    fn glyphs(doc: &Document, cells: &[(usize, usize, crate::document::Cell)], y: usize) -> String {
        (0..doc.width)
            .map(|x| cells.iter().find(|c| (c.0, c.1) == (x, y)).map_or(doc.layers[0].get(x, y).unwrap().ch, |c| c.2.ch))
            .collect()
    }

    #[test]
    fn steps_from_full_to_empty_and_blends_colours() {
        let doc = Document::new(9, 1);
        let mut c = ctx('#', red(), NONE);
        c.secondary_fg = blue();
        let cells = gradient(&doc, (0, 0), (8, 0), &c, linear(&doc));
        assert_eq!(glyphs(&doc, &cells, 0), "██▓▓▒▒░░ ");
        assert_eq!(cells[0].2.fg, red(), "the start gets the primary colour exactly");
        let middle = &cells.iter().find(|c| c.0 == 4).unwrap().2.fg;
        assert!(middle.r > 0 && middle.b > 0 && middle.a == 255, "a blend, not either end: {middle:?}");
        assert!(cells.iter().all(|c| c.2.ch != ' '), "empty cells keep their foreground");
    }

    #[test]
    fn right_button_reverses_and_a_click_does_nothing() {
        let doc = Document::new(9, 1);
        let mut c = ctx('#', red(), NONE);
        c.secondary_fg = blue();
        c.is_primary = false;
        let cells = gradient(&doc, (0, 0), (8, 0), &c, linear(&doc));
        assert_eq!(cells[0].2.fg, blue());
        assert!(gradient(&doc, (3, 0), (3, 0), &c, linear(&doc)).is_empty());
    }

    #[test]
    fn radial_rings_account_for_tall_cells() {
        let doc = Document::new(11, 5);
        let settings = GradientSettings { shape: GradientShape::Radial, ..linear(&doc) };
        let cells = gradient(&doc, (5, 2), (9, 2), &ctx('#', red(), NONE), settings);
        let at = |x: usize, y: usize| cells.iter().find(|c| (c.0, c.1) == (x, y)).map_or(' ', |c| c.2.ch);
        assert_eq!(at(5, 2), '█');
        assert_eq!(at(1, 2), at(9, 2));
        assert_eq!(at(5, 0), at(1, 2), "two rows up is as far as four columns across");
    }

    #[test]
    fn dithering_mixes_neighbouring_glyphs() {
        let doc = Document::new(9, 4);
        let column = |dither: bool| {
            let cells = gradient(&doc, (0, 0), (8, 0), &ctx('#', red(), NONE), GradientSettings { dither, ..linear(&doc) });
            let mut glyphs: Vec<char> = (0..4).map(|y| glyphs(&doc, &cells, y).chars().nth(1).unwrap()).collect();
            glyphs.sort();
            glyphs.into_iter().collect::<String>()
        };
        // Column 1 sits halfway between ▓ and █.
        assert_eq!(column(false), "████");
        assert_eq!(column(true), "██▓▓");
    }

    #[test]
    fn stays_in_the_region_and_fades_a_transparent_end() {
        let doc = Document::new(9, 2);
        let mut c = ctx('#', red(), NONE);
        c.channels.glyph = false;
        c.secondary_bg = blue();
        let settings = GradientSettings { region: (2, 1, 6, 2), ..linear(&doc) };
        let cells = gradient(&doc, (0, 0), (8, 0), &c, settings);
        assert!(cells.iter().all(|c| (2..6).contains(&c.0) && c.1 == 1));
        let bg = &cells.iter().find(|c| c.0 == 4).unwrap().2.bg;
        assert_eq!((bg.r, bg.g, bg.b), (0, 0, 255), "transparent has no colour of its own");
        assert!((100..160).contains(&bg.a), "half faded: {bg:?}");
        assert_eq!(cells[0].2.bg.a, ((2.0f32 / 8.0) * 255.0).round() as u8);
    }
}

mod box_lines {
    use super::{NONE, ctx, red};
    use crate::document::Document;
    use crate::tools::border::BorderStyle;
    use crate::tools::shapes::{Shape, ShapeMods, ShapeTool};

    fn draw(doc: &mut Document, shape: Shape, a: (usize, usize), b: (usize, usize)) {
        let mut tool = ShapeTool::default();
        tool.on_press(a.0, a.1);
        tool.on_release(doc, shape, b, ShapeMods::default(), &ctx('#', red(), NONE));
    }

    fn rows(doc: &Document) -> Vec<String> {
        (0..doc.height)
            .map(|y| (0..doc.width).map(|x| doc.layers[0].get(x, y).unwrap().ch).collect::<String>().trim_end().to_string())
            .collect()
    }

    #[test]
    fn boxes_sharing_an_edge_join() {
        let mut doc = Document::new(9, 3);
        let single = Shape::Border(BorderStyle::Single);
        draw(&mut doc, single, (0, 0), (4, 2));
        draw(&mut doc, single, (4, 0), (8, 2));
        assert_eq!(rows(&doc), ["┌───┬───┐", "│   │   │", "└───┴───┘"]);
    }

    #[test]
    fn box_lines_turn_and_join_boxes() {
        let mut doc = Document::new(10, 5);
        draw(&mut doc, Shape::Border(BorderStyle::Single), (5, 0), (9, 4));
        draw(&mut doc, Shape::BoxLine(BorderStyle::Single), (0, 2), (5, 2));
        draw(&mut doc, Shape::BoxLine(BorderStyle::Single), (0, 0), (3, 4));
        assert_eq!(rows(&doc), [
            "───┐ ┌───┐",
            "   │ │   │",
            "───┼─┤   │",
            "   │ │   │",
            "   │ └───┘",
        ]);
    }

    #[test]
    fn double_boxes_join_too() {
        let mut doc = Document::new(5, 5);
        let double = Shape::Border(BorderStyle::Double);
        draw(&mut doc, double, (0, 0), (4, 2));
        draw(&mut doc, double, (0, 2), (4, 4));
        assert_eq!(rows(&doc), ["╔═══╗", "║   ║", "╠═══╣", "║   ║", "╚═══╝"]);
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
