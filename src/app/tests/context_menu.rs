use super::*;

#[test]
fn a_project_offers_rename_project() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click(&mut app, entry_pos());
    assert_eq!(menu_labels(&app), ["rename project"]);
}

#[test]
fn a_workspace_offers_rename_workspace() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click_row(&mut app, WorkspaceRow::Workspace(0));
    assert_eq!(menu_labels(&app), ["rename workspace"]);
}

#[test]
fn a_tab_offers_rename_tab() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click_row(&mut app, WorkspaceRow::Tab(0, 0));
    assert_eq!(menu_labels(&app), ["rename tab"]);
}

#[test]
fn the_menu_button_of_a_project_opens_its_menu() {
    let (mut app, _rx, _dirs) = app_with(1);
    let row = ui::entry_row(list(), 1, &app.sidebar_rows(), 0, SidebarRow::Project(0));
    click(&mut app, ui::row_menu_button(row, 1).as_position());
    assert_eq!(menu_labels(&app), ["rename project"]);
}

#[rstest::rstest]
#[case::a_workspace(WorkspaceRow::Workspace(0), "rename workspace")]
#[case::a_tab(WorkspaceRow::Tab(0, 0), "rename tab")]
fn the_menu_button_of_a_row_opens_its_menu(#[case] row: WorkspaceRow, #[case] label: &str) {
    let (mut app, _rx, _dirs) = app_with(1);
    let menu = ui::row_menu_button(row_rect(&app, row), areas().pitch);
    click(&mut app, menu.as_position());
    assert_eq!(menu_labels(&app), [label]);
}

#[test]
fn clicking_elsewhere_closes_it_without_acting() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click(&mut app, entry_pos());

    click(&mut app, ui::new_project_button(list(), 1, &plain(1)).as_position());

    assert_eq!((app.overlay.is_none(), app.projects.len()), (true, 1));
}

#[test]
fn esc_closes_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    right_click(&mut app, entry_pos());

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(app.overlay.is_none());
}
