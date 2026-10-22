use super::close_agent::{agent, tabs};
use super::*;
use crate::agents::AgentState;
use crate::agents::activity::Activity;
use crate::project::Project;

fn resumable(app: &mut App, pane: u64) {
    let resume = AgentState { kind: "claude".into(), conversation: "c-1".into(), mode: None, account: None };
    app.projects.pane_by_mut(pane).expect("the pane").resume = Some(resume);
}

fn with_agents() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    app.config.agent_commands.insert("claude".into(), "/bin/sh".into());
    tabs(&mut app, 0, 2);
    for t in 0..2 {
        let pane = agent(&mut app, 0, t);
        resumable(&mut app, pane);
    }
    (app, rx, dirs)
}

fn workspace_id(app: &App) -> u64 {
    app.project().and_then(Project::workspace).expect("a workspace").id
}

fn tab_count(app: &App) -> usize {
    app.projects[0].workspaces[0].tabs.len()
}

fn close_from_menu(app: &mut App) {
    right_click_row(app, WorkspaceRow::Workspace(0));
    pick(app, "close agents");
}

fn closed(app: &mut App, rx: &Receiver<AppEvent>) {
    pump_until(app, rx, "the agent tabs go", |a| tab_count(a) == 1);
}

fn reopen_tag(app: &App) -> Position {
    let workspace = &app.projects[0].workspaces[0];
    let entry = app.workspace_entry(workspace);
    let areas = areas();
    let tabs = app.tab_lines();
    let layout = ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, 0);
    ui::workspace_tag(&layout, areas.pitch, &tabs, 0, (&entry, ui::Tag::Reopen)).expect("the reopen tag").as_position()
}

fn emptied() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = with_agents();
    app.projects[0].workspaces[0].tabs.remove(2);
    close_from_menu(&mut app);
    pump_until(&mut app, &rx, "the tabs go", |a| tab_count(a) == 0);
    (app, rx, dirs)
}

#[test]
fn closing_agents_from_the_menu_closes_the_agent_tabs_and_keeps_the_shell_tab() {
    let (mut app, rx, _dirs) = with_agents();
    let shell = app.projects[0].workspaces[0].tabs[2].id;

    close_from_menu(&mut app);
    closed(&mut app, &rx);

    assert_eq!(app.projects[0].workspaces[0].tabs[0].id, shell);
    let toast = app.messages.toast.as_ref().expect("a toast");
    assert!(toast.message.contains("2 agents closed"), "{}", toast.message);
    assert_eq!(toast.view().button, Some(ui::ToastButton::Reopen));
    assert_eq!(app.agents.closed.count(workspace_id(&app)), 2);
}

#[test]
fn a_workspace_with_nothing_to_close_does_not_offer_it() {
    let (mut app, _rx, _dirs) = app_with(1);

    right_click_row(&mut app, WorkspaceRow::Workspace(0));

    assert!(!menu_labels(&app).contains(&"close agents".to_string()));
}

#[test]
fn a_working_agent_is_asked_about_first() {
    let (mut app, _rx, _dirs) = with_agents();
    let pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    app.projects.pane_by_mut(pane).expect("the pane").agent.update(Some(Activity::Working), true, Instant::now());

    close_from_menu(&mut app);

    let ui::Overlay::Update(update) = overlay_shown(&app, AREA) else { panic!("the dialog is shown") };
    let notes: String = update.notes.iter().map(ToString::to_string).collect();
    assert_eq!(update.title, "Close agents");
    assert!(notes.contains("working: closing cuts its turn"), "{notes}");
    assert_eq!(tab_count(&app), 3);
}

#[test]
fn confirming_asks_again_when_another_agent_joined_the_plan() {
    let (mut app, _rx, _dirs) = with_agents();
    let pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    app.projects.pane_by_mut(pane).expect("the pane").agent.update(Some(Activity::Working), true, Instant::now());
    close_from_menu(&mut app);
    let third = agent(&mut app, 0, 2);
    resumable(&mut app, third);

    app.confirm_close_agents();

    assert!(matches!(app.overlay, Some(Overlay::CloseAgents(_))));
    assert_eq!(tab_count(&app), 3);
}

#[test]
fn the_row_shows_a_reopen_tag_that_brings_the_agents_back() {
    let (mut app, rx, _dirs) = with_agents();
    close_from_menu(&mut app);
    closed(&mut app, &rx);

    let tag = reopen_tag(&app);
    press(&mut app, tag);

    assert_eq!(tab_count(&app), 3);
    assert_eq!(app.agents.closed.count(workspace_id(&app)), 0);
    let toast = app.messages.toast.as_ref().expect("a toast");
    assert!(toast.message.contains("2 agents reopened"), "{}", toast.message);
}

#[test]
fn the_empty_pane_offers_the_agents_back() {
    let (mut app, _rx, _dirs) = emptied();
    let pane = app_areas(&app).pane;
    let button = ui::reopen_button(pane, 2).expect("the button fits");

    click(&mut app, button.as_position());

    assert_eq!(tab_count(&app), 2);
}

#[test]
fn the_toast_reopens_them() {
    let (mut app, _rx, _dirs) = emptied();
    let toast = app.messages.toast.as_ref().expect("a toast").view();
    let button = ui::toast_button(AREA, toast);

    click(&mut app, button.as_position());

    assert_eq!(tab_count(&app), 2);
}

#[test]
fn forgetting_asks_first_and_then_drops_them() {
    let (mut app, _rx, _dirs) = emptied();
    right_click_row(&mut app, WorkspaceRow::Workspace(0));

    pick(&mut app, "forget closed agents");
    assert!(matches!(app.overlay, Some(Overlay::ForgetAgents { .. })));
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.agents.closed.count(workspace_id(&app)), 0);
    assert_eq!(tab_count(&app), 0);
}

#[test]
fn the_row_menu_of_a_workspace_with_closed_agents_offers_to_reopen_and_forget() {
    let (mut app, _rx, _dirs) = emptied();

    right_click_row(&mut app, WorkspaceRow::Workspace(0));

    let labels = menu_labels(&app);
    assert!(labels.contains(&"reopen 2 agents".to_string()), "{labels:?}");
    assert!(labels.contains(&"forget closed agents".to_string()), "{labels:?}");
}

#[test]
fn closing_a_plain_workspace_that_holds_closed_agents_asks_first() {
    let (mut app, rx, _dirs) = with_agents();
    tabs(&mut app, 0, 0);
    close_from_menu(&mut app);
    closed(&mut app, &rx);

    click_close(&mut app, WorkspaceRow::Workspace(0));

    assert!(matches!(app.overlay, Some(Overlay::CloseWorkspace { .. })));
    assert!(confirmation(&app).expect("a question").contains("closed agents are forgotten"));
}

#[test]
fn closed_agents_do_not_notify() {
    let (mut app, rx, _dirs) = with_agents();
    let pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    app.projects.pane_by_mut(pane).expect("the pane").agent.update(Some(Activity::Working), true, Instant::now());
    let plan = app.agents_close_plan(workspace_id(&app)).expect("a plan");

    app.close_agents(&plan);
    closed(&mut app, &rx);
    app.messages.toast = None;
    app.watch_agents(Instant::now() + Duration::from_secs(30));

    assert!(app.messages.toast.is_none());
    assert_eq!(app.outbox.take_notifications().len(), 0);
}

#[test]
fn closing_again_while_the_panes_are_still_dying_keeps_them_once() {
    let (mut app, _rx, _dirs) = with_agents();
    let plan = app.agents_close_plan(workspace_id(&app)).expect("a plan");

    app.close_agents(&plan);

    assert!(app.agents_close_plan(workspace_id(&app)).is_err());
    assert_eq!(app.agents.closed.count(workspace_id(&app)), 2);
}

#[test]
fn the_keys_menu_closes_and_reopens_the_agents_of_the_shown_workspace() {
    let (mut app, rx, _dirs) = with_agents();
    app.close_agents_by_key();
    closed(&mut app, &rx);

    app.reopen_agents_by_key(AREA);

    assert_eq!(tab_count(&app), 3);
}

#[test]
fn the_keys_say_when_there_is_nothing_to_do() {
    let (mut app, _rx, _dirs) = app_with(1);

    app.close_agents_by_key();
    assert_eq!(toast(&app), Some("no agent to close in this workspace"));
    app.reopen_agents_by_key(AREA);
    assert_eq!(toast(&app), Some("this workspace has no closed agents"));
}

#[test]
fn removing_the_workspace_forgets_its_closed_agents() {
    let (mut app, rx, _dirs) = with_agents();
    close_from_menu(&mut app);
    closed(&mut app, &rx);
    let id = workspace_id(&app);
    assert_eq!(app.agents.closed.count(id), 2);

    app.close_workspace(workspace_at(&app, 0, 0));

    refreshing(&mut app, &rx, "the closed agents are forgotten", |a| a.agents.closed.count(id) == 0);
}
