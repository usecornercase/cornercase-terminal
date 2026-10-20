use super::*;
use crate::config;

const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };

fn stacked(projects: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(projects);
    app.config.sidebar = config::Sidebar::ProjectsOnTop.id().into();
    (app, rx, dirs)
}

fn press_at(app: &mut App, kind: MouseEventKind, pos: Position) {
    mouse_in(app, kind, pos, TALL);
}

fn drag_line(app: &mut App, rows: u16) -> Position {
    let line = app.layout(TALL).stack_border;
    let (from, to) = (Position::new(line.x + 3, line.y), Position::new(line.x + 3, line.y + rows));
    press_at(app, MouseEventKind::Down(MouseButton::Left), from);
    press_at(app, MouseEventKind::Drag(MouseButton::Left), to);
    press_at(app, MouseEventKind::Up(MouseButton::Left), to);
    to
}

#[test]
fn a_click_on_a_project_selects_it() {
    let (mut app, _rx, _dirs) = stacked(2);
    let list = app.layout(TALL).list;
    click_in(&mut app, Position::new(list.x + 3, list.y), TALL);
    assert_eq!(app.active, 0);
}

#[test]
fn a_click_on_a_tab_selects_it() {
    let (mut app, _rx, _dirs) = stacked(1);
    app.add_tab(0, 0, TALL).expect("add a tab");
    let list = app.layout(TALL).workspaces_list;
    let tab = ui::workspace_row(list, 1, &app.tab_lines(), app.scroll.of(List::Workspaces), WorkspaceRow::Tab(0, 0));
    click_in(&mut app, tab.as_position(), TALL);
    assert_eq!(app.projects[0].workspaces[0].active, 0);
}

#[test]
fn the_wheel_over_the_projects_scrolls_only_the_projects() {
    let (mut app, _rx, _dirs) = stacked(4);
    app.active = 0;
    app.follow(AREA);
    let (before, at) = (app.scroll.of(List::Workspaces), app.layout(AREA).list.as_position());
    mouse(&mut app, MouseEventKind::ScrollDown, at);
    assert_eq!((app.scroll.of(List::Projects) > 0, app.scroll.of(List::Workspaces)), (true, before));
}

#[test]
fn the_wheel_over_the_workspaces_scrolls_only_the_workspaces() {
    let (mut app, _rx, _dirs) = stacked(1);
    for _ in 1..4 {
        app.add_tab(0, 0, AREA).expect("add a tab");
    }
    app.projects[0].workspaces[0].active = 0;
    app.follow(AREA);
    let (before, at) = (app.scroll.of(List::Workspaces), app.layout(AREA).workspaces_list.as_position());
    mouse(&mut app, MouseEventKind::ScrollDown, at);
    assert_eq!((app.scroll.of(List::Workspaces) > before, app.scroll.of(List::Projects)), (true, 0));
}

#[test]
fn dragging_the_line_moves_it_and_saves_it() {
    let (mut app, _rx, _dirs) = stacked(1);
    let to = drag_line(&mut app, 3);
    let saved = app.state().widths.and_then(|w| w.stack).is_some();
    assert_eq!((app.layout(TALL).stack_border.y, saved), (to.y, true));
}

#[test]
fn a_double_click_on_the_line_splits_the_lists_in_half_again() {
    let (mut app, _rx, _dirs) = stacked(1);
    let at = drag_line(&mut app, 3);
    let moved = app.screen.widths.stack.is_some();

    press_at(&mut app, MouseEventKind::Down(MouseButton::Left), at);
    press_at(&mut app, MouseEventKind::Up(MouseButton::Left), at);
    press_at(&mut app, MouseEventKind::Down(MouseButton::Left), at);

    assert_eq!((moved, app.screen.widths.stack), (true, None));
}

#[test]
fn the_column_can_take_the_room_of_the_workspaces_column() {
    let (mut app, _rx, _dirs) = stacked(1);
    let border = app.layout(AREA).projects_border;
    let from = Position::new(border.x, border.y + 1);

    press(&mut app, from);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(59, from.y));
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(59, from.y));

    assert_eq!(app.screen.widths.projects, 60);
}

#[test]
fn the_terminals_get_the_room_of_the_workspaces_column() {
    let (mut app, _rx, _dirs) = app_with(1);
    let before = term(&app, 0).emulator.size().expect("size").1;

    app.config.sidebar = config::Sidebar::WorkspacesOnTop.id().into();
    app.resize(AREA);

    assert_eq!(term(&app, 0).emulator.size().expect("size").1, before + ui::WORKSPACES_WIDTH);
}
