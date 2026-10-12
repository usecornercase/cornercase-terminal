use super::*;
use crate::activity::Activity;
use crate::files::Mode;
use crate::shortcuts::Group;

fn with_prefix(mut app: App) -> App {
    app.config.prefix_key = "ctrl+]".into();
    app
}

fn prefix(app: &mut App) {
    send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
}

fn keys(app: &mut App, typed: &str) {
    prefix(app);
    for c in typed.chars() {
        send_key(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn add_tabs(app: &mut App, n: usize) {
    for _ in 0..n {
        app.push_tab(app.active, 0, AREA, None).expect("open a tab");
    }
}

fn active_tab(app: &App) -> usize {
    app.project().and_then(Project::workspace).map(|w| w.active).expect("a workspace")
}

fn add_workspace(app: &mut App) {
    let path = app.projects[app.active].path.clone();
    let workspace = app.new_workspace(AREA, path, Some("second".into()), false).expect("a workspace");
    app.projects[app.active].workspaces.push(workspace);
}

fn split_right(app: &mut App) {
    keys(app, "|");
    assert_eq!(app.tab().map(|t| t.panes.len()), Some(2));
}

fn active_pane(app: &App) -> u64 {
    app.term().map(|t| t.id).expect("a pane")
}

#[test]
fn without_a_prefix_the_key_reaches_the_pane() {
    let (mut app, _rx) = app();
    prefix(&mut app);
    assert!(app.overlay.is_none());
}

#[test]
fn the_prefix_opens_the_keys_menu() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    prefix(&mut app);
    assert!(matches!(app.overlay, Some(Overlay::Keys(None))));
}

#[test]
fn the_prefix_twice_types_it_in_the_pane() {
    let (app, rx) = app();
    let mut app = with_prefix(app);
    type_line(&mut app, "echo cat-\"\"starts; cat -v");
    shows(&mut app, &rx, "cat-starts");
    prefix(&mut app);
    prefix(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    shows(&mut app, &rx, "^]");
    assert!(app.overlay.is_none());
}

#[rstest::rstest]
#[case::esc(KeyCode::Esc)]
#[case::unknown(KeyCode::Char('z'))]
fn a_key_that_runs_nothing_closes_the_menu(#[case] code: KeyCode) {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    prefix(&mut app);
    send_key(&mut app, code, KeyModifiers::NONE);
    assert_eq!((app.overlay.is_none(), app.tab().map(|t| t.panes.len())), (true, Some(1)));
}

#[test]
fn a_group_key_opens_its_menu() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "f");
    assert!(matches!(app.overlay, Some(Overlay::Keys(Some(Group::Find)))));
}

#[test]
fn a_dialog_keeps_the_prefix() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    app.open_usage();
    prefix(&mut app);
    assert!(matches!(app.overlay, Some(Overlay::Usage)));
}

#[test]
fn a_paste_closes_the_menu() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    prefix(&mut app);
    app.handle_event(AppEvent::Input(Event::Paste("x".into())), AREA).expect("paste");
    assert!(app.overlay.is_none());
}

#[test]
fn a_click_on_an_entry_runs_it() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    prefix(&mut app);
    let view = app.keys_view(None);
    let menu = ui::keys::area(ui::keys::frame(app.layout(AREA).pane, AREA, &view), &view);
    let i = view.items.iter().position(|item| item.label == "new tab").expect("a new tab entry");
    click(&mut app, ui::keys::item(menu, &view, i).as_position());
    assert_eq!((app.overlay.is_none(), app.projects[0].workspaces[0].tabs.len()), (true, 2));
}

#[test]
fn n_and_p_step_through_the_tabs_and_wrap() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    add_tabs(&mut app, 2);
    let mut seen = Vec::new();
    for typed in ["n", "n", "p", "p"] {
        keys(&mut app, typed);
        seen.push(active_tab(&app));
    }
    assert_eq!(seen, [1, 2, 1, 0]);
}

#[test]
fn a_digit_shows_that_tab() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    add_tabs(&mut app, 2);
    keys(&mut app, "3");
    keys(&mut app, "9");
    assert_eq!(active_tab(&app), 2);
}

#[test]
fn brackets_switch_the_workspace() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    add_workspace(&mut app);
    keys(&mut app, "]");
    let next = app.projects[0].active;
    keys(&mut app, "]");
    assert_eq!((next, app.projects[0].active), (1, 0));
}

#[test]
fn braces_switch_the_project() {
    let (app, _rx, _dirs) = app_with(3);
    let mut app = with_prefix(app);
    app.active = 0;
    keys(&mut app, "{");
    let previous = app.active;
    keys(&mut app, "}");
    assert_eq!((previous, app.active), (2, 0));
}

#[test]
fn arrows_focus_the_pane_on_that_side() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    let left = active_pane(&app);
    split_right(&mut app);
    let right = active_pane(&app);
    prefix(&mut app);
    send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    let after_left = active_pane(&app);
    prefix(&mut app);
    send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!((after_left, active_pane(&app), right != left), (left, left, true));
}

#[test]
fn a_jumps_to_the_agent_that_needs_you() {
    let (app, _rx, _dirs) = app_with(2);
    let mut app = with_prefix(app);
    app.active = 0;
    let term = app.projects[1].workspaces[0].tabs[0].pane_mut().expect("a pane");
    term.agent.follow(Some(agents::CLAUDE));
    term.agent.update(Some(Activity::Waiting), false, Instant::now());
    keys(&mut app, "a");
    assert_eq!(app.active, 1);
}

#[test]
fn a_says_when_no_agent_needs_you() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "a");
    assert_eq!(toast(&app), Some("no agent needs you"));
}

#[test]
fn c_opens_a_tab_and_shows_it() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "c");
    assert_eq!((app.projects[0].workspaces[0].tabs.len(), active_tab(&app)), (2, 1));
}

#[test]
fn x_asks_before_closing_a_pane_of_a_split() {
    let (app, rx) = app();
    let mut app = with_prefix(app);
    split_right(&mut app);
    keys(&mut app, "x");
    assert_eq!(confirmation(&app).as_deref().map(|m| m.starts_with("Close the pane running")), Some(true));
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    pump_until(&mut app, &rx, "the pane closes", |app| app.tab().is_some_and(|t| t.panes.len() == 1));
}

#[test]
fn x_on_the_only_pane_asks_to_close_the_tab() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "x");
    assert!(matches!(app.overlay, Some(Overlay::CloseTab { .. })));
}

#[test]
fn r_asks_for_the_tab_name() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "r");
    assert!(matches!(app.overlay, Some(Overlay::Rename { target: Target::Tab(..), .. })));
}

#[rstest::rstest]
#[case::names("ff", Mode::Name)]
#[case::text("fw", Mode::Text)]
fn find_opens_the_files_panel_with_its_search_taking_the_keys(#[case] typed: &str, #[case] mode: Mode) {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, typed);
    let workspace = app.focus().workspace.expect("a workspace");
    assert_eq!((app.files_typing(), app.files.mode(workspace)), (true, mode));
}

#[test]
fn the_prefix_still_works_while_the_files_search_has_the_keys() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "ff");
    keys(&mut app, "c");
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);
}

#[test]
fn g_then_d_outside_git_says_so() {
    let (app, _rx, _dirs) = app_with(1);
    let mut app = with_prefix(app);
    keys(&mut app, "gd");
    assert_eq!(toast(&app), Some("this workspace is not in a git repository"));
}

#[test]
fn g_then_d_opens_the_changes_with_their_filter_taking_the_keys() {
    let repo = git_repo(&[]);
    let (app, _rx) = app_in(repo.path(), no_config());
    let mut app = with_prefix(app);
    keys(&mut app, "gd");
    assert_eq!((app.changes_shown(), app.filtering()), (true, true));
}

#[test]
fn t_opens_the_todo_list_on_a_new_item() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "t");
    assert!(app.todo_typing());
}

#[test]
fn w_then_n_asks_for_a_new_workspace() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "wn");
    assert!(matches!(app.overlay, Some(Overlay::NewWorkspace { .. })));
}

#[test]
fn w_then_x_asks_before_closing_a_plain_workspace() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    add_workspace(&mut app);
    keys(&mut app, "wx");
    assert!(matches!(app.overlay, Some(Overlay::CloseWorkspace { .. })));
}

#[test]
fn q_detaches() {
    let (app, _rx) = app();
    let mut app = with_prefix(app);
    keys(&mut app, "q");
    assert!(app.take_detach());
}
