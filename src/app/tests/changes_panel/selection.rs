use super::every_project::{Collapsed, assert_agent_got_the_reference, collapsed_owner, row};
use super::*;
use crate::changes::selection::{At, Spot};
use crate::ui::changes::{LinesAction, lines_actions};

pub(super) fn at(line: usize) -> Spot {
    Spot { hunk: 0, at: At::Line(line) }
}

pub(super) fn line(app: &App, file: usize, l: usize) -> Position {
    row(app, panel::Row::Line(file, 0, l))
}

fn click_line(app: &mut App, file: usize, l: usize) {
    let pos = line(app, file, l);
    click(app, pos);
}

fn press_line(app: &mut App, file: usize, l: usize) {
    let pos = line(app, file, l);
    press(app, pos);
}

pub(super) fn selected(app: &App) -> Option<(Spot, Spot)> {
    app.panel_view().and_then(|v| v.selection).map(|s| (s.from, s.to))
}

pub(super) fn footer(app: &App, action: LinesAction) -> Position {
    let view = app.panel_view().expect("panel");
    let (_, r) = lines_actions(panel_area(app), &view).into_iter().find(|(a, _)| *a == action).expect("a button");
    Position::new(r.x + 1, r.y)
}

fn select(app: &mut App, from: usize, to: usize) {
    press_line(app, 0, from);
    let to = line(app, 0, to);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to);
}

pub(super) fn tabbed() -> (TempDir, App, Receiver<AppEvent>) {
    let repo = git_repo(&[("a.txt", "one\n\ttwo\nthree\n")]);
    std::fs::write(repo.path().join("a.txt"), "one\n\tTWO\nthree\n").expect("edit");
    opened(repo)
}

pub(super) fn long() -> (TempDir, App, Receiver<AppEvent>) {
    let repo = git_repo(&[("a.txt", "one\n")]);
    let text = (1..=200).map(|n| format!("let line_{n} = {n};")).collect::<Vec<_>>().join("\n");
    std::fs::write(repo.path().join("big.txt"), text).expect("new file");
    opened(repo)
}

pub(super) fn big_file(app: &App) -> usize {
    let changes::Body::Ready(diff) = app.panel_view().expect("panel").body else { panic!("ready") };
    diff.files.iter().position(|f| f.path == "big.txt").expect("big.txt")
}

#[test]
fn pressing_and_dragging_selects_lines_of_one_file_and_the_release_ends_the_drag() {
    let (_repo, mut app, _rx) = tabbed();
    press_line(&mut app, 0, 1);
    assert_eq!((selected(&app), app.screen.drag.is_some()), (Some((at(1), at(1))), true));
    let third = line(&app, 0, 3);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), third);
    assert_eq!(selected(&app), Some((at(1), at(3))));
    let body = panel::parts(panel_area(&app), &app.panel_view().expect("panel")).body;
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(body.x + 2, body.bottom() - 1));
    assert_eq!(selected(&app), Some((at(1), at(3))), "the spacer below counts as the file's last line");
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), third);
    assert!(app.screen.drag.is_none());
}

#[test]
fn copy_takes_the_lines_as_they_are_in_the_file() {
    let (_repo, mut app, _rx) = tabbed();
    select(&mut app, 1, 2);
    let copy = footer(&app, LinesAction::Copy);
    click(&mut app, copy);
    assert_eq!((app.take_host_writes(), toast(&app)), (vec![clipboard::osc52("\ttwo\n\tTWO")], Some(COPIED)));
    assert!(selected(&app).is_some());
}

#[test]
fn ask_agent_without_an_agent_copies_the_new_side_reference() {
    let (_repo, mut app, _rx) = tabbed();
    select(&mut app, 0, 3);
    let ask = footer(&app, LinesAction::Ask);
    click(&mut app, ask);
    assert_eq!((app.take_host_writes(), toast(&app)), (vec![clipboard::osc52("a.txt:1-3")], Some(NO_AGENT)));
}

#[test]
fn only_removed_lines_ask_with_their_hunks_reference() {
    let repo = git_repo(&[("a.txt", "one\ntwo\nthree\n")]);
    std::fs::write(repo.path().join("a.txt"), "one\nthree\n").expect("edit");
    let (_repo, mut app, _rx) = opened(repo);
    click_line(&mut app, 0, 1);
    let ask = footer(&app, LinesAction::Ask);
    click(&mut app, ask);
    let (target, file) = first_changed_file(&app);
    app.hunk_action(&target, &file, Hunk { index: 0, action: HunkAction::Ask }, AREA).expect("ask");
    let writes = app.take_host_writes();
    assert_eq!(writes, vec![clipboard::osc52("a.txt:2"), clipboard::osc52("a.txt:2")]);
}

#[test]
fn ask_agent_types_the_reference_into_the_agent() {
    let (repo, _config, mut app) = with_an_agent();
    app.screen.side = Some(Side::Changes);
    click_line(&mut app, 0, 2);
    let ask = footer(&app, LinesAction::Ask);
    click(&mut app, ask);
    app.term_mut().expect("pane").write(b"\r");
    let got = repo.path().join("got");
    wait_until("the agent reads the reference", || std::fs::read_to_string(&got).is_ok_and(|t| !t.is_empty()));
    assert_eq!(std::fs::read_to_string(&got).expect("got"), "a.txt:2 ");
}

#[test]
fn a_selection_held_at_an_edge_scrolls_the_panel_and_moves_its_end() {
    let (_repo, mut app, _rx) = long();
    let (file, body) = (big_file(&app), panel::parts(panel_area(&app), &app.panel_view().expect("panel")).body);
    press_line(&mut app, file, 0);
    hold(&mut app, Instant::now(), 3);
    assert_eq!(app.panels.changes.scroll(), 0, "a press alone does not scroll");
    let bottom = Position::new(body.x + 2, body.bottom() - 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), bottom);
    assert!(app.tick(Instant::now()).is_some_and(|wait| wait <= LINES_SCROLL_EVERY));
    let held = hold(&mut app, Instant::now(), 9);
    assert_eq!(app.panels.changes.scroll(), 10);
    let end = selected(&app).expect("selection").1;
    assert_eq!(end, at(usize::from(body.height) + 10 - 3));
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), bottom);
    hold(&mut app, held, 3);
    assert_eq!((app.panels.changes.scroll(), app.lines_edge()), (10, None));
}

#[test]
fn the_wheel_scrolls_a_selection_without_ending_it() {
    let (_repo, mut app, _rx) = long();
    let first = line(&app, big_file(&app), 0);
    press(&mut app, first);
    mouse(&mut app, MouseEventKind::ScrollDown, first);
    mouse(&mut app, MouseEventKind::ScrollDown, first);
    assert_eq!(app.panels.changes.scroll(), 2 * WHEEL_ROWS.unsigned_abs());
    assert!(app.screen.drag.is_some());
    let end = selected(&app).expect("selection").1;
    assert!(end > at(0));
}

#[test]
fn a_press_on_a_hunk_header_clears_the_selection_and_brings_back_the_fold_button() {
    let (_repo, mut app, _rx) = tabbed();
    click_line(&mut app, 0, 1);
    assert!(selected(&app).is_some());
    assert_eq!(panel::fold_all(panel_area(&app), &app.panel_view().expect("panel")), Rect::default());
    let header = row(&app, panel::Row::Hunk(0, 0));
    click(&mut app, Position::new(header.x - 6, header.y));
    assert!(selected(&app).is_none() && app.panels.changes.selection().is_none());
    assert_ne!(panel::fold_all(panel_area(&app), &app.panel_view().expect("panel")), Rect::default());
}

#[test]
fn folding_the_file_drops_the_selection_for_good() {
    let (_repo, mut app, _rx) = tabbed();
    click_line(&mut app, 0, 1);
    let file = row(&app, panel::Row::File(0));
    click(&mut app, file);
    click(&mut app, file);
    assert!(app.panels.changes.selection().is_none());
}

#[test]
fn a_file_that_changes_hides_the_selection() {
    let (repo, mut app, rx) = tabbed();
    click_line(&mut app, 0, 1);
    std::fs::write(repo.path().join("a.txt"), "one\n\tTWO\nthree\nfour\n").expect("edit");
    let workspace = app.projects[0].workspaces[0].id;
    app.panels.changes.rescan(workspace);
    app.refresh(Instant::now());
    pump_until(&mut app, &rx, "the new patch", |a| {
        let Some(view) = a.panel_view() else { return false };
        matches!(&view.body, changes::Body::Ready(d) if d.files[0].added == 2)
    });
    assert!(selected(&app).is_none());
    assert_ne!(panel::fold_all(panel_area(&app), &app.panel_view().expect("panel")), Rect::default());
}

#[test]
fn another_mode_hides_it_and_coming_back_shows_it_again() {
    let (_repo, mut app, _rx) = tabbed();
    click_line(&mut app, 0, 1);
    app.panels.changes.set_mode(changes::Mode::Commits);
    assert!(selected(&app).is_none());
    app.panels.changes.set_mode(changes::Mode::Uncommitted);
    assert_eq!(selected(&app), Some((at(1), at(1))));
}

#[test]
fn every_project_asks_the_workspace_that_owns_the_file() {
    let Collapsed { owners, mut app, rx: _rx, agent_pane, file, .. } = collapsed_owner();
    click_line(&mut app, file, 1);
    let ask = footer(&app, LinesAction::Ask);
    click(&mut app, ask);
    assert_agent_got_the_reference(&mut app, &owners, agent_pane);
}
