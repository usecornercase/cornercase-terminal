use rstest::rstest;

use super::*;

fn ask_to_close(app: &mut App, p: usize) {
    let close = sidebar_close(app, SidebarRow::Project(p));
    click(app, close);
}

fn confirm_close(app: &mut App, p: usize) {
    ask_to_close(app, p);
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
}

#[test]
fn new_button_opens_the_folder_picker() {
    let (mut app, _rx) = app();
    click_new_project(&mut app);
    assert_eq!((matches!(app.overlay, Some(Overlay::Picker { .. })), app.projects.len()), (true, 1));
}

#[test]
fn entry_click_selects_it() {
    let (mut app, _rx, _dirs) = app_with(2);
    click(&mut app, Position::new(list().x + 2, list().y));
    assert_eq!(app.active, 0);
}

#[rstest]
#[case::one_tab(1, " Its tab and the programs running in it are stopped.")]
#[case::two_tabs(2, " Its 2 tabs and the programs running in them are stopped.")]
fn the_confirmation_says_what_stops(#[case] tabs: usize, #[case] stopped: &str) {
    let (mut app, _rx, _dirs) = app_with(1);
    for _ in 1..tabs {
        click_row(&mut app, WorkspaceRow::NewTab(0));
    }

    ask_to_close(&mut app, 0);

    let label = app.project_label(&app.projects[0]);
    let expected = format!("Close the project {label}?{stopped} Folders and worktrees stay on disk.");
    assert_eq!(confirmation(&app), Some(expected));
}

#[test]
fn without_tabs_the_confirmation_only_asks() {
    let (mut app, _rx, _dirs) = closing_last_tab();

    ask_to_close(&mut app, 0);

    let label = app.project_label(&app.projects[0]);
    let expected = format!("Close the project {label}? Folders and worktrees stay on disk.");
    assert_eq!(confirmation(&app), Some(expected));
}

#[test]
fn confirming_closes_that_project() {
    let (mut app, rx, _dirs) = app_with(3);
    let second = app.projects[1].id;
    ask_to_close(&mut app, 1);
    let asked = confirmation(&app).is_some();

    click(&mut app, form_button(CLOSE_SUBMIT, 0));

    pump_until(&mut app, &rx, "second project closes", |a| a.projects.iter().all(|p| p.id != second));
    assert!(asked);
}

#[test]
fn cancelling_keeps_the_project_running() {
    let (mut app, _rx, _dirs) = app_with(2);
    ask_to_close(&mut app, 1);
    let asked = confirmation(&app).is_some();

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!((asked, app.overlay.is_none(), app.projects[1].closing), (true, true, false));
}

#[test]
fn closing_the_active_one_activates_the_previous() {
    let (mut app, rx, _dirs) = app_with(2);
    confirm_close(&mut app, 1);
    pump_until(&mut app, &rx, "second project closes", |a| a.projects.len() == 1);
    assert_eq!(app.active, 0);
}

#[test]
fn quit_button_asks_to_detach_once() {
    let (mut app, _rx, _dirs) = app_with(2);
    click(&mut app, areas().quit.as_position());
    assert_eq!((app.take_detach(), app.take_detach(), app.projects.len()), (true, false, 2));
}

#[test]
fn new_project_button_works_with_no_projects() {
    let (mut app, rx) = app();
    let home = TempDir::new();
    app.home = Some(home.path().to_path_buf());
    confirm_close(&mut app, 0);
    pump_until(&mut app, &rx, "the project closes", App::is_empty);

    click_new_project(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!((app.projects.len(), app.active, app.projects[0].path.clone()), (1, 0, canonical(&home)));
}

#[test]
fn the_new_button_of_an_empty_window_opens_the_new_menu() {
    let (mut app, _rx) = empty_app();
    let button = ui::welcome_button(app_areas(&app).pane).expect("the button fits");

    click(&mut app, button.as_position());

    assert_eq!(menu_labels(&app), ["open project", "new group"]);
}
