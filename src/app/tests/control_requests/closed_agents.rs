use super::super::close_agent::{agent, tabs};
use super::*;
use crate::agents::AgentState;
use crate::agents::activity::Activity;
use crate::control::closed_agents::{AgentsClosed, AgentsForgot, AgentsReopened};
use crate::project::Project;

mod flows;

fn resume_state(conversation: &str) -> AgentState {
    AgentState { kind: "claude".into(), conversation: conversation.into(), mode: None, account: None }
}

fn setup(resumable: bool) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    app.config.agent_commands.insert("claude".into(), "/bin/sh".into());
    tabs(&mut app, 0, 1);
    for t in 0..2 {
        let pane = agent(&mut app, 0, t);
        if resumable {
            app.projects.pane_by_mut(pane).expect("the pane").resume = Some(resume_state(&format!("c-{t}")));
        }
    }
    (app, rx, dirs)
}

fn workspace(app: &App) -> u64 {
    app.project().and_then(Project::workspace).expect("a workspace").id
}

fn parsed<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("an answer")
}

fn resuming(app: &App) -> Vec<String> {
    let mut found: Vec<String> = app
        .projects
        .terms()
        .filter(|term| term.resuming.is_some())
        .filter_map(|term| term.resume.as_ref().map(|agent| agent.conversation.clone()))
        .collect();
    found.sort();
    found
}

fn commands_on_sh(app: &mut App) {
    for kind in crate::agents::kinds(&app.config) {
        app.config.agent_commands.insert(kind, "/bin/sh".into());
    }
}

fn closed_all(app: &mut App, rx: &Receiver<AppEvent>) -> AgentsClosed {
    let id = workspace(app);
    let closed: AgentsClosed = parsed(ok(app, "agents-close", &json!({"workspace": id})));
    pump_until(app, rx, "the tabs go", |a| a.projects[0].workspaces[0].tabs.is_empty());
    closed
}

#[test]
fn closing_and_reopening_resumes_each_conversation() {
    let (mut app, rx, _dirs) = setup(true);

    let closed = closed_all(&mut app, &rx);
    let reopened: AgentsReopened = parsed(ok(&mut app, "agents-reopen", &json!({"workspace": closed.workspace})));

    assert_eq!((closed.closed, closed.closed_agents, closed.agents.len()), (true, 2, 2));
    assert!(closed.agents.iter().all(|pane| pane.resumes));
    assert_eq!(reopened.panes.len(), 2);
    assert!(reopened.panes.iter().all(|pane| pane.resumes));
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);
    assert_eq!(resuming(&app), ["c-0", "c-1"]);
}

#[test]
fn check_reports_what_would_close_and_closes_nothing() {
    let (mut app, _rx, _dirs) = setup(true);
    let id = workspace(&app);

    let found: AgentsClosed = parsed(ok(&mut app, "agents-close", &json!({"workspace": id, "check": true})));

    assert_eq!((found.closed, found.agents.len(), found.closed_agents), (false, 2, 0));
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);
}

#[test]
fn a_working_agent_needs_force() {
    let (mut app, rx, _dirs) = setup(true);
    let pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    app.projects.pane_by_mut(pane).expect("the pane").agent.update(Some(Activity::Working), true, Instant::now());
    let id = workspace(&app);

    let refused = refused(&mut app, "agents-close", &json!({"workspace": id}));
    assert_eq!(refused.code, "conflict");
    assert!(refused.message.contains("working") && refused.message.contains("force"), "{}", refused.message);
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);

    let forced: AgentsClosed = parsed(ok(&mut app, "agents-close", &json!({"workspace": id, "force": true})));
    assert!(forced.closed);
    pump_until(&mut app, &rx, "the tabs go", |a| a.projects[0].workspaces[0].tabs.is_empty());
}

#[test]
fn the_workspace_defaults_to_the_shown_one() {
    let (mut app, _rx, _dirs) = setup(true);

    let found: AgentsClosed = parsed(ok(&mut app, "agents-close", &json!({"check": true})));

    assert_eq!(found.workspace, workspace(&app));
}

#[test]
fn an_unknown_workspace_and_nothing_to_reopen_are_refused() {
    let (mut app, _rx, _dirs) = setup(true);
    let id = workspace(&app);

    assert_eq!(refused(&mut app, "agents-close", &json!({"workspace": 999})).code, "not-found");
    assert_eq!(refused(&mut app, "agents-reopen", &json!({"workspace": id})).code, "conflict");
    assert_eq!(refused(&mut app, "agents-forget", &json!({"workspace": id})).code, "conflict");
}

#[test]
fn a_missing_agent_command_refuses_the_reopen_and_keeps_what_was_closed() {
    let (mut app, rx, _dirs) = setup(true);
    let closed = closed_all(&mut app, &rx);
    app.config.agent_commands.insert("claude".into(), "/nonexistent/claude".into());

    let refused = refused(&mut app, "agents-reopen", &json!({"workspace": closed.workspace}));

    assert_eq!(refused.code, "failed");
    assert!(refused.message.contains("/nonexistent/claude"), "{}", refused.message);
    assert_eq!(app.agents.closed.count(closed.workspace), 2);
    assert!(app.projects[0].workspaces[0].tabs.is_empty());
}

#[test]
fn an_agent_with_no_conversation_starts_again_and_says_so() {
    let (mut app, rx, _dirs) = setup(false);
    let id = workspace(&app);
    let refused = refused(&mut app, "agents-close", &json!({"workspace": id}));
    assert!(refused.message.contains("new one"), "{}", refused.message);
    let closed: AgentsClosed = parsed(ok(&mut app, "agents-close", &json!({"workspace": id, "force": true})));
    pump_until(&mut app, &rx, "the tabs go", |a| a.projects[0].workspaces[0].tabs.is_empty());
    assert!(closed.agents.iter().all(|pane| !pane.resumes));

    let reopened: AgentsReopened = parsed(ok(&mut app, "agents-reopen", &json!({"workspace": id})));

    let pane = reopened.panes[0].pane;
    assert!(reopened.panes.iter().all(|pane| !pane.resumes));
    assert_eq!(app.pane_bands().get(&pane), Some(&ui::PaneBand::Resuming("starting claude".into())));
    assert_eq!(resuming(&app), Vec::<String>::new());
}

#[test]
fn a_reopen_resumes_even_when_resuming_on_restart_is_off() {
    let (mut app, rx, _dirs) = setup(true);
    app.config.resume_agents = false;
    let closed = closed_all(&mut app, &rx);

    ok(&mut app, "agents-reopen", &json!({"workspace": closed.workspace}));

    assert_eq!(resuming(&app), ["c-0", "c-1"]);
}

#[test]
fn reopened_tabs_come_after_the_tabs_that_stayed() {
    let (mut app, rx, _dirs) = setup(true);
    tabs(&mut app, 0, 1);
    let stayed = app.projects[0].workspaces[0].tabs[2].id;
    let id = workspace(&app);
    ok(&mut app, "agents-close", &json!({"workspace": id}));
    pump_until(&mut app, &rx, "the agent tabs go", |a| a.projects[0].workspaces[0].tabs.len() == 1);

    let reopened: AgentsReopened = parsed(ok(&mut app, "agents-reopen", &json!({"workspace": id})));

    let order: Vec<u64> = app.projects[0].workspaces[0].tabs.iter().map(|tab| tab.id).collect();
    assert_eq!(order[0], stayed);
    assert_eq!(order[1..], reopened.tabs[..]);
}

#[test]
fn a_second_close_adds_to_what_is_kept() {
    let (mut app, rx, _dirs) = setup(true);
    let id = workspace(&app);
    ok(&mut app, "agents-close", &json!({"workspace": id}));
    pump_until(&mut app, &rx, "the tabs go", |a| a.projects[0].workspaces[0].tabs.is_empty());
    app.reap();
    tabs(&mut app, 0, 1);
    let third = agent(&mut app, 0, 0);
    app.projects.pane_by_mut(third).expect("the pane").resume = Some(resume_state("c-9"));

    let again: AgentsClosed = parsed(ok(&mut app, "agents-close", &json!({"workspace": id})));

    assert_eq!(again.closed_agents, 3);
}

#[test]
fn forgetting_drops_them_for_good() {
    let (mut app, rx, _dirs) = setup(true);
    let closed = closed_all(&mut app, &rx);

    let forgot: AgentsForgot = parsed(ok(&mut app, "agents-forget", &json!({"workspace": closed.workspace})));

    assert_eq!((forgot.forgot, forgot.run), (2, None));
    assert_eq!(app.agents.closed.count(closed.workspace), 0);
}

#[test]
fn the_status_counts_the_closed_agents_of_a_workspace() {
    let (mut app, rx, _dirs) = setup(true);
    closed_all(&mut app, &rx);

    let status = ok(&mut app, "status", &json!({}));

    let workspace = &status["projects"][0]["workspaces"][0];
    assert_eq!(workspace["closed_agents"], 2);
}

#[test]
fn closed_agents_survive_a_restart_of_the_server_and_reopen_there() {
    let (mut app, rx, _dirs) = setup(true);
    let closed = closed_all(&mut app, &rx);
    let saved = app.state();
    let (mut again, _rx_again) = new_app(no_config());
    again.config.agent_commands.insert("claude".into(), "/bin/sh".into());

    assert!(again.restore(&saved, AREA));
    let id = workspace(&again);
    let status = ok(&mut again, "status", &json!({}));
    ok(&mut again, "agents-reopen", &json!({"workspace": id}));

    assert_eq!(closed.closed_agents, 2);
    assert_eq!(status["projects"][0]["workspaces"][0]["closed_agents"], 2);
    assert_eq!(resuming(&again), ["c-0", "c-1"]);
}
