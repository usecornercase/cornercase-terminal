use ratatui::style::Modifier;

use super::every_project::row;
use super::selection::{at, big_file, footer, line, long, selected, tabbed};
use super::*;
use crate::text::stretch::Stretch;
use crate::ui::changes::LinesAction;

fn text_at(app: &App, file: usize, l: usize, column: u16) -> Position {
    let at = line(app, file, l);
    Position::new(at.x + 3 + column, at.y)
}

fn copied(app: &mut App) -> Option<Vec<u8>> {
    let copy = footer(app, LinesAction::Copy);
    click(app, copy);
    app.take_host_writes().last().cloned()
}

fn two_files() -> (TempDir, App, Receiver<AppEvent>) {
    let repo = git_repo(&[("a.txt", "one\n\ttwo\nthree\n"), ("b.txt", "x\n")]);
    std::fs::write(repo.path().join("a.txt"), "one\n\tTWO\nthree\n").expect("edit");
    std::fs::write(repo.path().join("b.txt"), "y\n").expect("edit");
    opened(repo)
}

#[test]
fn a_click_on_the_code_still_selects_its_line() {
    let (_repo, mut app, _rx) = tabbed();
    let at_text = text_at(&app, 0, 2, 5);
    click(&mut app, at_text);
    assert_eq!(selected(&app), Some((at(2), at(2))));
    assert_eq!(app.panels.changes.selection().and_then(|s| s.stretch), None);
}

#[test]
fn dragging_on_the_code_selects_characters_and_copy_takes_them_from_the_file_s_text() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_at(&app, 0, 2, 4), text_at(&app, 0, 2, 6));
    drag_between(&mut app, from, to);
    assert_eq!(copied(&mut app), Some(clipboard::osc52("TWO")));
    assert_eq!(toast(&app), Some(COPIED));
    let (from, to) = (text_at(&app, 0, 2, 0), text_at(&app, 0, 2, 6));
    drag_between(&mut app, from, to);
    assert_eq!(copied(&mut app), Some(clipboard::osc52("\tTWO")));
}

#[test]
fn characters_cross_removed_and_added_lines_in_the_order_shown() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_at(&app, 0, 1, 5), text_at(&app, 0, 2, 5));
    drag_between(&mut app, from, to);
    assert_eq!(copied(&mut app), Some(clipboard::osc52("wo\n\tTW")));
    assert_eq!(selected(&app), Some((at(1), at(2))));
}

#[test]
fn over_another_file_the_end_stays_in_the_first_one() {
    let (_repo, mut app, _rx) = two_files();
    let from = text_at(&app, 0, 0, 0);
    let other = row(&app, panel::Row::File(1));
    drag_between(&mut app, from, other);
    assert_eq!(copied(&mut app), Some(clipboard::osc52("one\n\ttwo\n\tTWO\nthree")));
}

#[test]
fn another_mode_hides_the_characters_and_coming_back_shows_them_again() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_at(&app, 0, 1, 5), text_at(&app, 0, 2, 5));
    drag_between(&mut app, from, to);
    app.panels.changes.set_mode(changes::Mode::Commits);
    assert!(selected(&app).is_none());
    app.panels.changes.set_mode(changes::Mode::Uncommitted);
    assert_eq!(selected(&app), Some((at(1), at(2))));
    assert_eq!(copied(&mut app), Some(clipboard::osc52("wo\n\tTW")));
}

#[test]
fn folding_the_file_drops_the_characters_for_good() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_at(&app, 0, 1, 5), text_at(&app, 0, 2, 5));
    drag_between(&mut app, from, to);
    let file = row(&app, panel::Row::File(0));
    click(&mut app, file);
    click(&mut app, file);
    assert!(app.panels.changes.selection().is_none());
}

#[test]
fn ask_agent_sends_the_lines_the_characters_cover() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, gutter) = (text_at(&app, 0, 0, 0), line(&app, 0, 3));
    drag_between(&mut app, from, gutter);
    let stretch = app.panels.changes.selection().and_then(|s| s.stretch).expect("characters");
    assert_eq!((stretch.to.line, stretch.to.offset), (at(3), 0));
    let ask = footer(&app, LinesAction::Ask);
    click(&mut app, ask);
    assert_eq!(app.take_host_writes(), vec![clipboard::osc52("a.txt:1-2")]);
}

#[test]
fn the_selected_characters_are_reversed_and_the_line_look_is_gone() {
    let (_repo, mut app, _rx) = tabbed();
    let (from, to) = (text_at(&app, 0, 1, 5), text_at(&app, 0, 2, 5));
    drag_between(&mut app, from, to);
    let terminal = render(&mut app, AREA);
    let buffer = terminal.backend().buffer();
    let reversed = |p: Position| buffer[(p.x, p.y)].modifier.contains(Modifier::REVERSED);
    let at_text = |l, column| text_at(&app, 0, l, column);
    assert!(!reversed(at_text(1, 4)) && reversed(at_text(1, 5)) && reversed(at_text(1, 20)));
    assert!(reversed(at_text(2, 0)) && reversed(at_text(2, 5)) && !reversed(at_text(2, 6)));
    let marks = line(&app, 0, 1);
    assert_ne!(buffer[(marks.x - 7, marks.y)].symbol(), "▌");
}

#[test]
fn held_at_an_edge_the_panel_scrolls_and_the_end_follows_the_pointer() {
    let (_repo, mut app, _rx) = long();
    let (file, body) = (big_file(&app), panel::parts(panel_area(&app), &app.panel_view().expect("panel")).body);
    let first = text_at(&app, file, 0, 0);
    press(&mut app, first);
    let bottom = Position::new(body.x + 14, body.bottom() - 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), bottom);
    hold(&mut app, Instant::now(), 9);
    assert_eq!(app.panels.changes.scroll(), 10);
    let end = app.panels.changes.selection().and_then(|s| s.stretch).expect("characters").to;
    assert_eq!(end.line, at(usize::from(body.height) + 10 - 3));
}

#[test]
fn the_wheel_scrolls_a_character_selection_without_ending_it() {
    let (_repo, mut app, _rx) = long();
    let first = text_at(&app, big_file(&app), 0, 0);
    press(&mut app, first);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(first.x + 4, first.y));
    mouse(&mut app, MouseEventKind::ScrollDown, first);
    assert!(app.screen.drag.is_some());
    let stretch: Stretch<_> = app.panels.changes.selection().and_then(|s| s.stretch).expect("characters");
    assert!(stretch.to.line > at(0));
}

#[test]
fn a_click_and_a_character_selection_on_a_continuation_row() {
    let line = "word ".repeat(40);
    let repo = git_repo(&[("a.txt", "one\n")]);
    std::fs::write(repo.path().join("a.txt"), format!("{line}\n")).expect("edit");
    let (_repo, mut app, _rx) = opened(repo);
    let second = row(&app, panel::Row::Line(0, 0, 1, 1));
    click(&mut app, Position::new(second.x - 5, second.y));
    assert_eq!(selected(&app).map(|(from, _)| from), Some(at(1)));
    let (from, to) = (Position::new(second.x + 2, second.y), Position::new(second.x + 6, second.y));
    drag_between(&mut app, from, to);
    let copied = copied(&mut app);
    let first_row = row(&app, panel::Row::Line(0, 0, 1, 0));
    assert_ne!(first_row.y, second.y);
    assert!(copied.is_some_and(|c| c.starts_with(b"\x1b]52") && c.len() > 8));
}
