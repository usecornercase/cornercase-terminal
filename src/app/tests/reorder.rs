use rstest::rstest;

use super::*;
use crate::state;

fn drag_entry(app: &mut App, from: SidebarRow, to: SidebarRow) {
    let (from, to) = (sidebar_pos(app, from), sidebar_pos(app, to));
    drag(app, from, to);
}

fn drag_row(app: &mut App, from: WorkspaceRow, to: WorkspaceRow) {
    let (from, to) = (row_pos(app, from), row_pos(app, to));
    drag(app, from, to);
}

fn saved_projects(app: &App) -> Vec<(PathBuf, Option<usize>)> {
    app.state().projects.into_iter().map(|p| (p.path, p.group)).collect()
}

fn grouped(n: usize, in_work: &[usize]) -> (App, Receiver<AppEvent>, Vec<PathBuf>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(n);
    let work = app.add_group("work".into());
    for &p in in_work {
        app.projects[p].group = Some(work);
    }
    let paths = dirs.iter().map(canonical).collect();
    (app, rx, paths, dirs)
}

fn with_workspaces(names: &[&str]) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    for name in names {
        let path = app.projects[0].path.clone();
        let workspace = app.new_workspace(AREA, path, Some((*name).into()), false).expect("workspace");
        app.projects[0].workspaces.push(workspace);
    }
    (app, rx, dirs)
}

fn with_tabs(names: &[&str]) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    for _ in 1..names.len() {
        app.add_tab(0, 0, AREA).expect("add a tab");
    }
    for (tab, name) in app.projects[0].workspaces[0].tabs.iter_mut().zip(names) {
        tab.name = Some((*name).into());
    }
    (app, rx, dirs)
}

fn saved_tabs(app: &App) -> String {
    let state = app.state();
    let names: Vec<String> = state.projects[0].workspaces[0].tabs.iter().filter_map(|t| t.name.clone()).collect();
    names.join(" ")
}

#[rstest]
#[case::loose(&[])]
#[case::out_of_a_group(&[2])]
fn dragging_a_project_among_the_loose_ones_moves_it_there_and_saves_the_order(#[case] in_work: &[usize]) {
    let (mut app, _rx, paths, _dirs) = grouped(3, in_work);

    drag_entry(&mut app, SidebarRow::Project(2), SidebarRow::Project(0));

    let expected = vec![(paths[2].clone(), None), (paths[0].clone(), None), (paths[1].clone(), None)];
    assert_eq!((saved_projects(&app), app.active), (expected, 0));
}

#[test]
fn dragging_a_group_moves_it_with_its_projects() {
    let (mut app, _rx, _paths, _dirs) = grouped(2, &[0]);
    let oss = app.add_group("oss".into());
    app.projects[1].group = Some(oss);

    drag_entry(&mut app, SidebarRow::Group(1), SidebarRow::Group(0));

    let state = app.state();
    let groups: Vec<&str> = state.groups.iter().map(|g| g.name.as_str()).collect();
    let projects: Vec<Option<usize>> = state.projects.iter().map(|p| p.group).collect();
    assert_eq!((groups, projects), (vec!["oss", "work"], vec![Some(1), Some(0)]));
}

#[test]
fn dragging_a_workspace_moves_it_and_keeps_the_active_one() {
    let (mut app, _rx, _dirs) = with_workspaces(&["b", "c"]);

    drag_row(&mut app, WorkspaceRow::Workspace(2), WorkspaceRow::Workspace(0));

    let state = app.state();
    let saved: Vec<Option<String>> = state.projects[0].workspaces.iter().map(|w| w.name.clone()).collect();
    assert_eq!((saved, app.projects[0].active), (vec![Some("c".into()), None, Some("b".into())], 1));
}

#[test]
fn dragging_a_tab_moves_it_and_keeps_the_active_one() {
    let (mut app, _rx, _dirs) = with_tabs(&["one", "two", "three"]);

    drag_row(&mut app, WorkspaceRow::Tab(0, 2), WorkspaceRow::Tab(0, 0));

    assert_eq!((saved_tabs(&app), app.projects[0].workspaces[0].active), ("three one two".into(), 0));
}

#[test]
fn a_tab_dragged_onto_another_workspace_stays_in_its_own() {
    let (mut app, _rx, _dirs) = with_tabs(&["one", "two"]);
    let path = app.projects[0].path.clone();
    let other = app.new_workspace(AREA, path, Some("other".into()), false).expect("workspace");
    app.projects[0].workspaces.push(other);

    drag_row(&mut app, WorkspaceRow::Tab(0, 0), WorkspaceRow::Tab(1, 0));

    assert_eq!((saved_tabs(&app), app.projects[0].workspaces[1].tabs.len()), ("two one".into(), 1));
}

#[test]
fn a_row_is_activated_on_release() {
    let (mut app, _rx, _dirs) = app_with(2);
    let first = sidebar_pos(&app, SidebarRow::Project(0));

    press(&mut app, first);
    let pressed = app.active;
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), first);

    assert_eq!((pressed, app.active), (1, 0));
}

#[test]
fn a_release_on_another_row_moves_it_even_without_motion() {
    let (mut app, _rx, paths, _dirs) = grouped(2, &[]);
    let (from, to) = (sidebar_pos(&app, SidebarRow::Project(1)), sidebar_pos(&app, SidebarRow::Project(0)));

    press(&mut app, from);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), to);

    assert_eq!(saved_projects(&app), vec![(paths[1].clone(), None), (paths[0].clone(), None)]);
}

#[test]
fn moving_within_the_row_is_still_a_click() {
    let (mut app, _rx, _dirs) = app_with(2);
    let before = app.state();
    let first = sidebar_pos(&app, SidebarRow::Project(0));

    drag(&mut app, first, Position::new(first.x + 6, first.y));

    assert_eq!((app.active, app.state().projects), (0, before.projects));
}

#[derive(Debug, Clone, Copy)]
enum Cancel {
    Esc,
    ReleaseOutside,
    ReleaseOnItself,
    RightClick,
}

#[rstest]
#[case::esc(Cancel::Esc)]
#[case::release_outside_the_list(Cancel::ReleaseOutside)]
#[case::release_on_itself(Cancel::ReleaseOnItself)]
#[case::another_button(Cancel::RightClick)]
fn a_cancelled_drag_changes_nothing(#[case] cancel: Cancel) {
    let (mut app, _rx, _dirs) = app_with(3);
    let before = app.state();
    let (from, to) = (sidebar_pos(&app, SidebarRow::Project(2)), sidebar_pos(&app, SidebarRow::Project(0)));
    press(&mut app, from);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), to);

    let end = match cancel {
        Cancel::Esc => {
            send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
            to
        }
        Cancel::ReleaseOutside => Position::new(areas().pane.x + 5, to.y),
        Cancel::ReleaseOnItself => from,
        Cancel::RightClick => {
            right_click(&mut app, to);
            to
        }
    };
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), end);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), end);

    assert_eq!((app.state(), app.active, app.overlay.is_none()), (before, 2, true));
}

#[test]
fn dropping_a_project_on_a_group_header_puts_it_first_in_the_group() {
    let (mut app, _rx, paths, _dirs) = grouped(3, &[2]);

    drag_entry(&mut app, SidebarRow::Project(0), SidebarRow::Group(0));

    let expected = vec![(paths[1].clone(), None), (paths[0].clone(), Some(0)), (paths[2].clone(), Some(0))];
    assert_eq!(saved_projects(&app), expected);
}

#[test]
fn dropping_a_project_on_a_collapsed_group_puts_it_last_in_the_group() {
    let (mut app, _rx, paths, _dirs) = grouped(3, &[1, 2]);
    app.groups[0].entry.collapsed = true;

    drag_entry(&mut app, SidebarRow::Project(0), SidebarRow::Group(0));

    let expected = vec![(paths[1].clone(), Some(0)), (paths[2].clone(), Some(0)), (paths[0].clone(), Some(0))];
    assert_eq!(saved_projects(&app), expected);
}

#[test]
fn dropping_a_project_between_grouped_projects_puts_it_there() {
    let (mut app, _rx, paths, _dirs) = grouped(3, &[1, 2]);

    drag_entry(&mut app, SidebarRow::Project(0), SidebarRow::Project(1));

    let expected = vec![(paths[1].clone(), Some(0)), (paths[0].clone(), Some(0)), (paths[2].clone(), Some(0))];
    assert_eq!(saved_projects(&app), expected);
}

#[test]
fn holding_a_drag_on_the_more_line_scrolls_the_list() {
    const SHORT: Rect = Rect { x: 0, y: 0, width: 100, height: 12 };
    let (mut app, _rx, _dirs) = app_with(4);
    app.active = 0;
    app.follow(SHORT);
    let short = ui::layout(SHORT, state::Widths::default());
    let rows = ui::project_rows(short.list, short.pitch, &app.sidebar_rows(), app.scroll.of(List::Projects));

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), short.list.as_position(), SHORT);
    mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), rows.more_below().as_position(), SHORT);
    let dragged = app.scroll.of(List::Projects);
    app.refresh(Instant::now() + AUTO_SCROLL_EVERY);

    assert_eq!((dragged, app.scroll.of(List::Projects)), (1, 2));
}

#[test]
fn a_drag_in_the_compact_menu_reorders_and_keeps_it_open() {
    const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 40 };
    let (mut app, _rx, paths, _dirs) = grouped(2, &[]);
    app.screen.nav = Some(ui::Nav::Projects);
    let small = ui::layout(SMALL, state::Widths::default());
    let entry = |p| ui::entry_row(small.list, small.pitch, &app.sidebar_rows(), 0, SidebarRow::Project(p));
    let (from, to) = (entry(1).as_position(), entry(0).as_position());

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), from, SMALL);
    mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), to, SMALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), to, SMALL);

    let expected = vec![(paths[1].clone(), None), (paths[0].clone(), None)];
    assert_eq!((saved_projects(&app), app.screen.nav), (expected, Some(ui::Nav::Projects)));
}

#[test]
fn a_restored_session_keeps_the_order_of_every_level() {
    let (mut app, _rx, _dirs) = with_tabs(&["one", "two"]);
    let other = TempDir::new();
    app.open_project(other.path().to_path_buf(), AREA).expect("open project");
    app.add_group("work".into());
    app.add_group("oss".into());
    drag_entry(&mut app, SidebarRow::Group(1), SidebarRow::Group(0));
    drag_entry(&mut app, SidebarRow::Project(1), SidebarRow::Project(0));
    let path = app.projects[1].path.clone();
    let second = app.new_workspace(AREA, path, Some("second".into()), false).expect("workspace");
    app.projects[1].workspaces.push(second);
    app.active = 1;
    drag_row(&mut app, WorkspaceRow::Tab(0, 1), WorkspaceRow::Tab(0, 0));
    drag_row(&mut app, WorkspaceRow::Workspace(1), WorkspaceRow::Workspace(0));
    let saved = app.state();

    let (mut restored, _rx2) = empty_app();
    restored.restore(&saved, AREA);

    let state = restored.state();
    let groups: Vec<&str> = state.groups.iter().map(|g| g.name.as_str()).collect();
    let workspaces: Vec<Option<&str>> = state.projects[1].workspaces.iter().map(|w| w.name.as_deref()).collect();
    let tabs: Vec<Option<&str>> = state.projects[1].workspaces[1].tabs.iter().map(|t| t.name.as_deref()).collect();
    assert_eq!(
        (groups, state.projects[1].path.clone(), workspaces, tabs),
        (vec!["oss", "work"], app.projects[1].path.clone(), vec![Some("second"), None], vec![Some("two"), Some("one")])
    );
    assert_eq!(state, saved);
}
