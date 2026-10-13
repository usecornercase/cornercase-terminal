use super::*;
use crate::project::group;

fn named() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(2);
    app.add_tab(0, 0, AREA).expect("add a tab");
    app.projects[0].name = Some("alpha".into());
    app.projects[1].name = Some("beta".into());
    app.projects[0].workspaces[0].name = Some("feat/login".into());
    app.projects[0].workspaces[0].tabs[0].name = Some("server".into());
    app.projects[0].workspaces[0].tabs[1].name = Some("editor".into());
    app.active = 1;
    (app, rx, dirs)
}

fn open_search(app: &mut App) {
    click(app, areas().search.as_position());
}

fn query(app: &App) -> Option<&str> {
    match &app.overlay {
        Some(Overlay::Search(search)) => Some(search.query()),
        _ => None,
    }
}

fn active(app: &App) -> (usize, usize, usize) {
    let project = &app.projects[app.active];
    (app.active, project.active, project.workspaces[project.active].active)
}

fn result_pos(app: &App, i: usize) -> Position {
    let len = app.search_results(query(app).expect("search is open")).len();
    ui::result_item(areas().results, len, 0, i).as_position()
}

#[test]
fn clicking_the_bar_opens_it_and_keys_go_to_it() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    type_text(&mut app, "alp");
    assert_eq!(query(&app), Some("alp"));
}

#[test]
fn enter_goes_to_the_best_match() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    submit_text(&mut app, "alpha");
    assert_eq!((app.active, query(&app)), (0, None));
}

#[test]
fn a_tab_is_found_in_another_project() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    submit_text(&mut app, "editor");
    assert_eq!(active(&app), (0, 0, 1));
}

#[test]
fn tabs_are_found_by_their_workspace() {
    let (app, _rx, _dirs) = named();
    let names: Vec<String> = app.search_results("login").into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["feat/login", "server", "editor"]);
}

#[test]
fn down_selects_the_next_result() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    type_text(&mut app, "login");
    send_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(active(&app), (0, 0, 0));
}

fn with_group(app: &mut App, collapsed: bool) {
    let id = app.add_group("clients".into());
    app.groups[0].entry.collapsed = collapsed;
    app.projects[0].group = Some(id);
}

#[test]
fn a_group_is_found_before_its_projects() {
    let (mut app, _rx, _dirs) = named();
    with_group(&mut app, false);
    app.projects[1].name = Some("clients-api".into());
    let found: Vec<(Kind, String)> = app.search_results("clients").into_iter().map(|c| (c.kind, c.name)).collect();
    let group = format!("{} clients", group::GROUP_STYLES[0].0);
    assert_eq!(found, [(Kind::Group, group), (Kind::Project, "clients-api".into())]);
}

#[test]
fn a_project_shows_its_group_as_context() {
    let (mut app, _rx, _dirs) = named();
    with_group(&mut app, false);
    let contexts: Vec<String> = app.search_results("alpha").into_iter().map(|c| c.context).collect();
    assert_eq!(contexts, ["clients"]);
}

#[test]
fn going_to_a_group_expands_it_and_opens_its_first_project() {
    let (mut app, _rx, _dirs) = named();
    with_group(&mut app, true);
    open_search(&mut app);
    submit_text(&mut app, "clients");
    assert_eq!((app.active, app.groups[0].entry.collapsed, query(&app)), (0, false, None));
}

#[test]
fn going_to_an_empty_group_expands_it_and_keeps_the_project() {
    let (mut app, _rx, _dirs) = named();
    app.add_group("empty".into());
    app.groups[0].entry.collapsed = true;
    open_search(&mut app);
    submit_text(&mut app, "empty");
    assert_eq!((app.active, app.groups[0].entry.collapsed), (1, false));
}

#[test]
fn enter_with_no_match_keeps_it_open() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    submit_text(&mut app, "zzz");
    assert_eq!((app.active, query(&app)), (1, Some("zzz")));
}

#[test]
fn esc_closes_it_without_switching() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    type_text(&mut app, "alpha");
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!((app.active, query(&app)), (1, None));
}

#[test]
fn clicking_a_result_goes_there() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    type_text(&mut app, "editor");
    let pos = result_pos(&app, 0);
    click(&mut app, pos);
    assert_eq!((active(&app), query(&app)), ((0, 0, 1), None));
}

#[test]
fn clicking_outside_closes_it_without_acting() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    let new = new_project_pos(&app);
    click(&mut app, new);
    assert!(app.overlay.is_none(), "the search is still open or the new menu opened");
}

#[test]
fn a_paste_goes_into_the_query() {
    let (mut app, _rx, _dirs) = named();
    open_search(&mut app);
    app.handle_event(AppEvent::Input(Event::Paste("bet\r".into())), AREA).expect("handle paste");
    assert_eq!(query(&app), Some("bet"));
}
