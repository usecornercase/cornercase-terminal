use super::*;
use ratatui::style::Modifier;

use crate::text::stretch::{Point, Stretch};

fn text_pos(app: &App, line: u32, column: u16) -> Position {
    let at = line_pos(app, line);
    Position::new(at.x + 5 + column, at.y)
}

fn copied(app: &mut App) -> Option<Vec<u8>> {
    let copy = action_pos(app, Action::Copy);
    click(app, copy);
    app.take_host_writes().last().cloned()
}

fn osc(text: &str) -> Vec<u8> {
    clipboard::osc52(text)
}

fn tabbed() -> (TempDir, App, Receiver<AppEvent>) {
    file_with("fn main() {\n\trun();\n}\n")
}

#[test]
fn a_click_on_the_text_selects_its_line() {
    let (_repo, mut app, _rx) = file_with("fn main() {\n    run();\n}\n");
    let at = text_pos(&app, 2, 6);
    click(&mut app, at);
    assert_eq!((file(&app).selection, file(&app).stretch), (Some((2, 2)), None));
    assert_eq!(copied(&mut app), Some(osc("    run();")));
}

#[test]
fn dragging_on_the_text_selects_characters_and_copy_takes_them() {
    let (_repo, mut app, _rx) = file_with("fn main() {\n    run();\n}\n");
    let (from, to) = (text_pos(&app, 2, 4), text_pos(&app, 2, 8));
    drag_between(&mut app, from, to);
    assert_eq!(file(&app).stretch, Some(Stretch::within(2, 4..9)));
    assert_eq!(copied(&mut app), Some(osc("run()")));
}

#[test]
fn a_stretch_across_lines_copies_the_file_s_own_text() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_pos(&app, 1, 3), text_pos(&app, 2, 4));
    drag_between(&mut app, from, to);
    assert_eq!(copied(&mut app), Some(osc("main() {\n\tr")));
}

#[test]
fn a_cell_inside_a_tab_stands_for_the_tab() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_pos(&app, 2, 2), text_pos(&app, 2, 5));
    drag_between(&mut app, from, to);
    assert_eq!(file(&app).stretch, Some(Stretch::within(2, 0..3)));
    assert_eq!(copied(&mut app), Some(osc("\tru")));
}

#[test]
fn the_hidden_end_of_a_cut_line_is_copied() {
    let line = format!("let long = \"{}\";", "x".repeat(150));
    let (_repo, mut app, _rx) = file_with(&format!("{line}\nlet short = 1;\n"));
    let code = text_pos(&app, 1, 0);
    let cut = Position::new(panel_area(&app).right() - 2, code.y);
    drag_between(&mut app, code, cut);
    assert_eq!(copied(&mut app), Some(osc(&line)));
    let next = text_pos(&app, 2, 2);
    drag_between(&mut app, code, next);
    assert_eq!(copied(&mut app), Some(osc(&format!("{line}\nlet"))));
}

#[test]
fn ask_agent_names_the_lines_the_characters_cover() {
    let (_repo, mut app, _rx) = file_with("fn main() {\n    run();\n}\n");
    let (from, gutter) = (text_pos(&app, 1, 3), line_pos(&app, 3));
    drag_between(&mut app, from, gutter);
    assert_eq!(file(&app).stretch.map(|s| s.to), Some(Point { line: 3, offset: 0 }));
    let ask = action_pos(&app, Action::Ask);
    click(&mut app, ask);
    assert_eq!(app.take_host_writes().last(), Some(&clipboard::osc52("src/main.rs:1-2")));
    assert_eq!(toast(&app), Some(NO_AGENT));
}

#[test]
fn the_header_names_the_covered_lines() {
    let (_repo, mut app, _rx) = file_with("fn main() {\n    run();\n}\n");
    let (from, to) = (text_pos(&app, 1, 3), line_pos(&app, 3));
    drag_between(&mut app, from, to);
    let info = panel::parts(panel_area(&app)).info;
    let terminal = render(&mut app, AREA);
    let buffer = terminal.backend().buffer();
    let row: String = (info.x..info.right()).map(|x| buffer[(x, info.y)].symbol().to_string()).collect();
    assert!(row.contains("1-2") && !row.contains("1-3"), "{row}");
}

#[test]
fn the_selected_characters_are_reversed_and_a_line_break_runs_to_the_edge() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_pos(&app, 1, 3), text_pos(&app, 2, 4));
    drag_between(&mut app, from, to);
    let terminal = render(&mut app, AREA);
    let buffer = terminal.backend().buffer();
    let reversed = |p: Position| buffer[(p.x, p.y)].modifier.contains(Modifier::REVERSED);
    let at = |line, column| text_pos(&app, line, column);
    assert!(!reversed(at(1, 2)) && reversed(at(1, 3)) && reversed(at(1, 12)));
    assert!(reversed(at(2, 0)) && reversed(at(2, 4)) && !reversed(at(2, 5)));
    assert!(!reversed(line_pos(&app, 1)), "the numbers keep their look");
}

#[test]
fn a_press_on_the_numbers_replaces_characters_with_lines() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_pos(&app, 1, 3), text_pos(&app, 2, 4));
    drag_between(&mut app, from, to);
    assert!(file(&app).stretch.is_some());
    let numbers = line_pos(&app, 2);
    click(&mut app, numbers);
    assert_eq!((file(&app).selection, file(&app).stretch), (Some((2, 2)), None));
    assert_eq!(copied(&mut app), Some(osc("\trun();")));
}

#[test]
fn held_at_an_edge_the_file_scrolls_and_the_end_follows_the_pointer() {
    let (_repo, mut app, _rx) = long_file();
    let body = panel::parts(panel_area(&app)).body;
    let (first, bottom) = (text_pos(&app, 1, 0), Position::new(body.x + 8, body.bottom() - 1));
    press(&mut app, first);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), bottom);
    hold(&mut app, Instant::now(), 9);
    assert_eq!(file(&app).scroll, 10);
    let end = file(&app).stretch.expect("characters are selected").to;
    assert_eq!(end.line, u32::from(body.height) + 10);
}

#[test]
fn the_wheel_scrolls_a_character_selection_without_ending_it() {
    let (_repo, mut app, _rx) = long_file();
    let first = text_pos(&app, 1, 0);
    press(&mut app, first);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(first.x + 3, first.y));
    mouse(&mut app, MouseEventKind::ScrollDown, first);
    mouse(&mut app, MouseEventKind::ScrollDown, first);
    assert_eq!(file(&app).scroll, 6);
    assert_eq!(file(&app).stretch.map(|s| s.to.line), Some(7));
    let below = Position::new(first.x + 3, first.y + 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), below);
    assert_eq!(file(&app).stretch.map(|s| s.to.line), Some(8));
}

#[test]
fn reset_interaction_drops_a_character_selection() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_pos(&app, 1, 3), text_pos(&app, 2, 4));
    drag_between(&mut app, from, to);
    assert!(file(&app).stretch.is_some());
    app.reset_interaction();
    assert_eq!(file(&app).stretch, None);
}

#[test]
fn a_file_that_shrinks_clamps_the_selection_to_its_text() {
    let (repo, mut app, rx) = file_with("one\ntwo\nthree\nfour\n");
    let (from, to) = (text_pos(&app, 2, 1), text_pos(&app, 4, 2));
    drag_between(&mut app, from, to);
    std::fs::write(repo.path().join("src/main.rs"), "one\ntwo\n").expect("shrink");
    settle(&mut app, &rx, "the shorter file is read", |v| lines(v) == ["one", "two"]);
    assert_eq!(copied(&mut app), Some(osc("wo")));
}
