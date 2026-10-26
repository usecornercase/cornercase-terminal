use rstest::rstest;

use super::*;
use crate::state;

const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 40 };

fn small() -> ui::Areas {
    ui::layout(SMALL, state::Widths::default())
}

fn press(app: &mut App, pos: Position) {
    click_in(app, pos, SMALL);
}

fn open_menu(app: &mut App) {
    press(app, small().bar.as_position());
}

fn row(app: &App, row: WorkspaceRow) -> Position {
    let r = ui::workspace_row(
        small().workspaces_list,
        small().pitch,
        &app.tab_lines(),
        app.scroll.of(List::Workspaces),
        row,
    );
    Position::new(r.x + 3, r.y)
}

#[test]
fn the_bar_opens_the_workspaces_of_the_active_project() {
    let (mut app, _rx) = app();
    open_menu(&mut app);
    assert_eq!(app.screen.nav, Some(ui::Nav::Workspaces));
}

#[test]
fn the_bar_opens_the_projects_without_a_project() {
    let (mut app, _rx) = app();
    app.projects.clear();
    open_menu(&mut app);
    assert_eq!(app.screen.nav, Some(ui::Nav::Projects));
}

#[test]
fn the_bar_closes_an_open_menu() {
    let (mut app, _rx) = app();
    open_menu(&mut app);
    open_menu(&mut app);
    assert_eq!(app.screen.nav, None);
}

#[test]
fn back_shows_the_projects() {
    let (mut app, _rx) = app();
    open_menu(&mut app);
    press(&mut app, small().back.as_position());
    assert_eq!(app.screen.nav, Some(ui::Nav::Projects));
}

#[test]
fn picking_a_project_shows_its_workspaces() {
    let (mut app, _rx, _dirs) = app_with(2);
    open_menu(&mut app);
    press(&mut app, small().back.as_position());
    let first =
        ui::entry_row(small().list, small().pitch, &plain(2), app.scroll.of(List::Projects), SidebarRow::Project(0))
            .as_position();
    press(&mut app, first);
    assert_eq!((app.active, app.screen.nav), (0, Some(ui::Nav::Workspaces)));
}

#[test]
fn picking_a_tab_closes_the_menu() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.add_tab(workspace_at(&app, 0, 0), SMALL).expect("add a tab");
    open_menu(&mut app);
    let pos = row(&app, WorkspaceRow::Tab(0, 0));
    press(&mut app, pos);
    assert_eq!((app.projects[0].workspaces[0].active, app.screen.nav), (0, None));
}

#[test]
fn closing_a_tab_keeps_the_menu_open() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.add_tab(workspace_at(&app, 0, 0), SMALL).expect("add a tab");
    open_menu(&mut app);
    let r = ui::workspace_row(small().workspaces_list, small().pitch, &app.tab_lines(), 0, WorkspaceRow::Tab(0, 0));
    press(&mut app, ui::row_close_button(r, small().pitch).as_position());
    assert_eq!(app.screen.nav, Some(ui::Nav::Workspaces));
}

#[test]
fn clicks_on_the_menu_never_reach_the_pane() {
    let (mut app, _rx) = app();
    open_menu(&mut app);
    press(&mut app, small().back.as_position());
    let under_the_title = Position::new(3, small().title.y + 1);
    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), under_the_title, SMALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Right), under_the_title, SMALL);
    assert!(app.overlay.is_none(), "the pane menu opened under the projects menu");
}

#[test]
fn usage_in_the_more_menu_opens_the_usage_modal() {
    let (mut app, _rx) = app();
    app.config.agent_commands.insert(agents::Known::Claude.name().into(), "/nonexistent/claude".into());
    app.config.agent_commands.insert(agents::Known::Codex.name().into(), "/nonexistent/codex".into());
    open_menu(&mut app);
    press(&mut app, small().back.as_position());
    press(&mut app, small().more.as_position());
    pick_in(&mut app, "usage", SMALL);
    assert_eq!((matches!(app.overlay, Some(Overlay::Usage)), app.screen.nav), (true, None));
}

#[test]
fn escape_closes_the_menu() {
    let (mut app, _rx) = app();
    open_menu(&mut app);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.screen.nav, None);
}

#[test]
fn the_search_button_opens_the_search() {
    let (mut app, _rx) = app();
    press(&mut app, small().search_button.as_position());
    assert!(matches!(app.overlay, Some(Overlay::Search(_))));
}

#[test]
fn a_wide_terminal_closes_the_menu() {
    let (mut app, _rx) = app();
    open_menu(&mut app);
    render(&mut app, AREA);
    assert_eq!(app.screen.nav, None);
}

#[test]
fn the_pane_takes_the_whole_width() {
    let (mut app, _rx) = app();
    app.resize(SMALL);
    assert_eq!(term(&app, 0).emulator.size().expect("size"), (SMALL.height - ui::COMPACT_PITCH, SMALL.width));
}

fn with_workspace(worktree: bool) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    let path = app.projects[0].path.clone();
    let other = app.new_workspace(AREA, path, Some("other".into()), worktree).expect("workspace");
    app.projects[0].workspaces.push(other);
    open_menu(&mut app);
    (app, rx, dirs)
}

fn click_close(app: &mut App, row: WorkspaceRow) {
    let r = ui::workspace_row(
        small().workspaces_list,
        small().pitch,
        &app.tab_lines(),
        app.scroll.of(List::Workspaces),
        row,
    );
    press(app, ui::row_close_button(r, small().pitch).as_position());
}

fn confirm(app: &mut App) {
    press(app, ui::form_buttons(ui::form_area(SMALL), CLOSE_SUBMIT)[0].as_position());
}

#[test]
fn closing_a_tab_asks_first() {
    let (mut app, _rx, _dirs) = with_workspace(false);
    app.projects[0].workspaces[1].tabs[0].name = Some("build".into());

    click_close(&mut app, WorkspaceRow::Tab(1, 0));

    let expected = "Close the tab build? The programs running in it are stopped.";
    assert_eq!(confirmation(&app).as_deref(), Some(expected));
}

#[test]
fn confirming_closes_the_tab() {
    let (mut app, rx, _dirs) = with_workspace(false);
    click_close(&mut app, WorkspaceRow::Tab(1, 0));

    confirm(&mut app);

    pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[1].tabs.is_empty());
}

#[test]
fn the_question_goes_when_its_tab_exits_by_itself() {
    let (mut app, rx, _dirs) = with_workspace(false);
    click_close(&mut app, WorkspaceRow::Tab(1, 0));
    let asked = confirmation(&app).is_some();

    app.close_tab(tab_at(&app, 0, 1, 0));

    pump_until(&mut app, &rx, "the question goes", |a| a.overlay.is_none());
    assert!(asked);
}

#[test]
fn closing_a_workspace_asks_first() {
    let (mut app, _rx, _dirs) = with_workspace(false);

    click_close(&mut app, WorkspaceRow::Workspace(1));

    let expected = "Close the workspace other? Its tab and the programs running in it are stopped.";
    assert_eq!(confirmation(&app).as_deref(), Some(expected));
}

#[test]
fn confirming_closes_the_workspace() {
    let (mut app, rx, _dirs) = with_workspace(false);
    click_close(&mut app, WorkspaceRow::Workspace(1));

    confirm(&mut app);

    pump_until(&mut app, &rx, "the workspace closes", |a| a.projects[0].workspaces.len() == 1);
}

#[rstest]
#[case::workspace(WorkspaceRow::Workspace(1))]
#[case::tab(WorkspaceRow::Tab(1, 0))]
fn cancelling_keeps_everything_running(#[case] row: WorkspaceRow) {
    let (mut app, rx, _dirs) = with_workspace(false);
    click_close(&mut app, row);
    let asked = confirmation(&app).is_some();

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.screen.nav = None;
    app.projects[0].active = 1;
    type_in_pane(&mut app, &rx, "still");

    assert_eq!((asked, app.projects[0].workspaces[1].closing()), (true, false));
}

#[test]
fn removing_a_worktree_shows_only_its_own_dialog() {
    let (mut app, _rx, _dirs) = with_workspace(true);
    click_close(&mut app, WorkspaceRow::Workspace(1));
    assert!(matches!(app.overlay, Some(Overlay::RemoveWorkspace { .. })));
}

#[test]
fn closing_a_project_asks_first() {
    let (mut app, _rx, _dirs) = app_with(2);
    open_menu(&mut app);
    press(&mut app, small().back.as_position());
    let rows = app.sidebar_rows();

    press(&mut app, ui::close_button(small().list, small().pitch, &rows, 0, SidebarRow::Project(1)).as_position());

    assert!(matches!(app.overlay, Some(Overlay::CloseProject { .. })));
}

#[test]
fn plus_tab_in_an_empty_pane_opens_a_shell_without_asking() {
    let (mut app, rx, _dirs) = app_with(1);
    app.close_tab(tab_at(&app, 0, 0, 0));
    pump_until(&mut app, &rx, "the tab goes", |a| a.projects[0].workspaces[0].tabs.is_empty());
    let button = ui::no_tab_buttons(app.layout(SMALL).pane, app.no_tab()).new_tab.expect("the button fits");

    press(&mut app, button.as_position());

    assert!(app.overlay.is_none());
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
}
