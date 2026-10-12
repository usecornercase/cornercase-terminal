use super::*;

const SHORT: Rect = Rect { x: 0, y: 0, width: 100, height: 12 };

fn short() -> ui::Areas {
    ui::layout(SHORT, ui::Widths::default())
}

fn wheel_at(app: &mut App, kind: MouseEventKind, pos: Position) {
    mouse_in(app, kind, pos, SHORT);
}

fn with_tabs(n: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    for _ in 1..n {
        app.add_tab(0, 0, SHORT).expect("add a tab");
    }
    (app, rx, dirs)
}

#[test]
fn the_active_project_is_scrolled_into_view() {
    let (mut app, _rx, _dirs) = app_with(4);
    app.follow(SHORT);
    assert!(
        !ui::entry_row(short().list, short().pitch, &plain(4), app.projects_scroll, SidebarRow::Project(3))
            .is_empty()
    );
}

#[test]
fn the_wheel_scrolls_the_projects() {
    let (mut app, _rx, _dirs) = app_with(4);
    app.active = 0;
    app.follow(SHORT);
    wheel_at(&mut app, MouseEventKind::ScrollDown, short().list.as_position());
    assert_eq!(app.projects_scroll, 2);
}

#[test]
fn scrolling_away_does_not_snap_back_to_the_active_project() {
    let (mut app, _rx, _dirs) = app_with(4);
    app.follow(SHORT);
    wheel_at(&mut app, MouseEventKind::ScrollUp, short().list.as_position());
    app.follow(SHORT);
    assert_eq!(app.projects_scroll, 0);
}

#[test]
fn a_click_on_a_scrolled_entry_selects_that_project() {
    let (mut app, _rx, _dirs) = app_with(4);
    app.follow(SHORT);
    app.active = 0;
    let pos = Position::new(short().list.x + 3, short().list.y);
    click_in(&mut app, pos, SHORT);
    assert_eq!(app.active, 2);
}

#[test]
fn a_new_tab_is_scrolled_into_view() {
    let (mut app, _rx, _dirs) = with_tabs(4);
    app.follow(SHORT);
    let row = ui::workspace_row(
        short().workspaces_list,
        short().pitch,
        &app.tab_lines(),
        app.workspaces_scroll,
        WorkspaceRow::Tab(0, 3),
    );
    assert!(!row.is_empty());
}

#[test]
fn the_wheel_scrolls_the_workspaces() {
    let (mut app, _rx, _dirs) = with_tabs(4);
    app.projects[0].workspaces[0].active = 0;
    app.follow(SHORT);
    wheel_at(&mut app, MouseEventKind::ScrollDown, short().workspaces_list.as_position());
    assert_eq!(app.workspaces_scroll, 3);
}
