use rstest::rstest;

use super::*;
use crate::config;
use crate::state;

fn moved_projects_border() -> (App, Receiver<AppEvent>, Position) {
    let (mut app, rx) = app();
    let from = border(&app, ui::Border::Projects);
    drag(&mut app, from, 39);
    let moved = border(&app, ui::Border::Projects);
    (app, rx, moved)
}

fn grab_projects_border(app: &mut App) -> Position {
    let from = border(app, ui::Border::Projects);
    press(app, from);
    from
}

fn border(app: &App, border: ui::Border) -> Position {
    let r = app.layout(AREA).border(border);
    Position::new(r.x, r.y + 1)
}

fn drag(app: &mut App, from: Position, to_x: u16) {
    press(app, from);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), Position::new(to_x, from.y));
    mouse(app, MouseEventKind::Up(MouseButton::Left), Position::new(to_x, from.y));
}

fn pane_cols(app: &App) -> u16 {
    app.layout(AREA).pane.width
}

#[test]
fn dragging_the_projects_border_widens_the_sidebar() {
    let (mut app, _rx) = app();
    let from = border(&app, ui::Border::Projects);

    drag(&mut app, from, 39);

    assert_eq!(
        app.screen.widths,
        state::Widths { projects: 40, workspaces: ui::WORKSPACES_WIDTH, ..state::Widths::default() }
    );
}

#[test]
fn dragging_the_workspaces_border_narrows_the_pane() {
    let (mut app, _rx) = app();
    let before = pane_cols(&app);
    let from = border(&app, ui::Border::Workspaces);

    drag(&mut app, from, from.x + 5);

    assert_eq!(pane_cols(&app), before - 5);
}

#[test]
fn the_terminals_follow_the_new_pane_size() {
    let (mut app, _rx) = app();
    let from = border(&app, ui::Border::Projects);

    drag(&mut app, from, from.x - 10);
    app.resize(AREA);

    assert_eq!(term(&app, 0).emulator.size().expect("size"), (AREA.height, pane_cols(&app)));
}

#[test]
fn the_drag_ends_on_release() {
    let (mut app, _rx) = app();
    let from = border(&app, ui::Border::Projects);
    drag(&mut app, from, 39);

    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(45, from.y));

    assert_eq!((app.screen.drag, app.screen.widths.projects), (None, 40));
}

fn hover(app: &mut App, pos: Position) {
    mouse(app, MouseEventKind::Moved, pos);
}

#[rstest]
#[case::projects(config::Sidebar::SideBySide, ui::Border::Projects, pointer::Shape::Columns)]
#[case::workspaces(config::Sidebar::SideBySide, ui::Border::Workspaces, pointer::Shape::Columns)]
#[case::stack(config::Sidebar::ProjectsOnTop, ui::Border::Stack, pointer::Shape::Rows)]
#[case::agents(config::Sidebar::ProjectsOnTop, ui::Border::Agents, pointer::Shape::Rows)]
fn hovering_a_border_asks_for_a_resize_pointer(
    #[case] sidebar: config::Sidebar,
    #[case] which: ui::Border,
    #[case] expected: pointer::Shape,
) {
    let (mut app, _rx) = app();
    app.config.sidebar = sidebar.id().into();
    app.config.agents_section = true;
    let r = app.layout(AREA).border(which);

    hover(&mut app, Position::new(r.x + r.width / 2, r.y));

    assert_eq!(app.pointer(AREA), expected);
}

#[test]
fn the_changes_border_asks_for_a_sideways_pointer() {
    let (mut app, _rx) = app();
    app.screen.side = Some(Side::Todo);
    let from = border(&app, ui::Border::Changes);

    hover(&mut app, from);

    assert_eq!(app.pointer(AREA), pointer::Shape::Columns);
}

#[test]
fn leaving_the_border_brings_back_the_default_pointer() {
    let (mut app, _rx) = app();
    let from = border(&app, ui::Border::Projects);
    hover(&mut app, from);

    hover(&mut app, Position::new(from.x + 4, from.y));

    assert_eq!(app.pointer(AREA), pointer::Shape::Default);
}

#[test]
fn the_pointer_stays_while_dragging_away_from_the_border_and_goes_on_release() {
    let (mut app, _rx) = app();
    let from = grab_projects_border(&mut app);
    let away = Position::new(AREA.width - 1, from.y + 4);
    let dragging = pointer_after_drag(&mut app, away);

    assert_eq!((dragging, app.pointer(AREA)), (pointer::Shape::Columns, pointer::Shape::Default));
}

#[test]
fn esc_ends_the_drag_where_it_is() {
    let (mut app, _rx) = app();
    let from = grab_projects_border(&mut app);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(AREA.width - 1, from.y + 4));
    let widths = app.screen.widths;

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(
        (app.screen.drag.as_ref(), app.screen.widths, app.pointer(AREA)),
        (None, widths, pointer::Shape::Default)
    );
}

#[test]
fn the_prefix_key_ends_the_drag_and_brings_back_the_default_pointer() {
    let (mut app, _rx) = app();
    app.config.prefix_key = "ctrl+]".into();
    let from = grab_projects_border(&mut app);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(AREA.width - 1, from.y + 4));

    send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);

    assert!(matches!(app.overlay, Some(Overlay::Keys(None))));
    assert_eq!((app.screen.drag.as_ref(), app.pointer(AREA)), (None, pointer::Shape::Default));
}

#[test]
fn an_overlay_brings_back_the_default_pointer() {
    let (mut app, _rx) = app();
    let at = border(&app, ui::Border::Projects);
    hover(&mut app, at);

    app.overlay = Some(Overlay::Search(Finder::default()));

    assert_eq!(app.pointer(AREA), pointer::Shape::Default);
}

#[test]
fn a_drag_away_from_the_border_keeps_resizing() {
    let (mut app, _rx) = app();
    let from = grab_projects_border(&mut app);

    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(39, from.y + 4));

    assert_eq!((app.screen.drag, app.screen.widths.projects), (Some(Drag::Border(ui::Border::Projects)), 40));
}

#[test]
fn a_double_click_brings_back_the_default_width() {
    let (mut app, _rx, moved) = moved_projects_border();

    click(&mut app, moved);
    click(&mut app, moved);

    assert_eq!((app.screen.widths, app.screen.drag), (state::Widths::default(), None));
}

#[test]
fn two_slow_clicks_are_not_a_double_click() {
    let (mut app, _rx, moved) = moved_projects_border();
    click(&mut app, moved);
    app.screen.border_click =
        app.screen.border_click.map(|(b, at)| (b, at.checked_sub(DOUBLE_CLICK).expect("an earlier instant")));

    click(&mut app, moved);

    assert_eq!(app.screen.widths.projects, 40);
}

#[test]
fn a_click_right_after_a_drag_is_not_a_double_click() {
    let (mut app, _rx, moved) = moved_projects_border();

    press(&mut app, moved);

    assert_eq!((app.screen.widths.projects, app.screen.drag), (40, Some(Drag::Border(ui::Border::Projects))));
}

#[test]
fn the_widths_are_saved() {
    let (mut app, _rx) = app();
    let from = border(&app, ui::Border::Projects);

    drag(&mut app, from, 39);

    assert_eq!(
        app.state().widths,
        Some(state::Widths { projects: 40, workspaces: ui::WORKSPACES_WIDTH, ..state::Widths::default() })
    );
}

#[test]
fn restore_brings_back_the_widths() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();
    let widths = state::Widths { projects: 40, workspaces: 20, ..state::Widths::default() };
    let project = ProjectState {
        path: dir.path().to_path_buf(),
        name: None,
        group: None,
        workspaces: vec![],
        active: 0,
        collapsed: false,
    };
    let saved = State {
        groups: Vec::new(),
        version: state::VERSION,
        projects: vec![project],
        active: 0,
        widths: Some(widths),
        issues: None,
        changes: None,
        todo: false,
        files: false,
        flow: false,
    };

    app.restore(&saved, AREA);

    assert_eq!(app.screen.widths, widths);
}
