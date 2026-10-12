use super::*;

fn open_rename(app: &mut App, at: Position) {
    right_click(app, at);
    let item = menu_item_at(app, 0);
    click(app, item);
}

fn folder(dir: &TempDir) -> String {
    dir.path().file_name().and_then(|n| n.to_str()).expect("folder name").to_string()
}

#[test]
fn opens_with_the_current_name() {
    let (mut app, _rx, dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    assert_eq!(form_value(&app), Some(folder(&dirs[0]).as_str()));
}

#[test]
fn renames_the_project() {
    let (mut app, _rx, _dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    clear_input(&mut app);
    type_text(&mut app, "  my api  ");

    click(&mut app, form_button(RENAME_SUBMIT, 0));

    assert_eq!((app.overlay.is_none(), app.project_label(&app.projects[0])), (true, "my api".into()));
}

#[test]
fn an_empty_name_goes_back_to_the_folder_name() {
    let (mut app, _rx, dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    submit_text(&mut app, "-x");
    open_rename(&mut app, entry_pos());

    clear_input(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.project_label(&app.projects[0]), folder(&dirs[0]));
}

#[test]
fn esc_keeps_the_old_name() {
    let (mut app, _rx, dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    type_text(&mut app, "-changed");

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!((app.overlay.is_none(), app.project_label(&app.projects[0])), (true, folder(&dirs[0])));
}

#[test]
fn renames_a_workspace() {
    let (mut app, _rx, _dirs) = app_with(1);
    let at = row_pos(&app, WorkspaceRow::Workspace(0));
    open_rename(&mut app, at);
    clear_input(&mut app);

    submit_text(&mut app, "main line");

    assert_eq!(workspace_labels(&app), ["main line"]);
}

#[test]
fn renames_a_tab_until_it_is_cleared() {
    let (mut app, _rx, _dirs) = app_with(1);
    let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
    open_rename(&mut app, at);
    clear_input(&mut app);
    submit_text(&mut app, "server");
    assert_eq!(app.projects[0].workspaces[0].tabs[0].label(&app.config), "server");

    let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
    open_rename(&mut app, at);
    clear_input(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    wait_until("back to the program name", || is_sh(&app.projects[0].workspaces[0].tabs[0].label(&app.config)));
}
