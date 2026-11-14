use std::time::Duration;

use super::wake::{prompts_in, reads, sends, stopped, team, working};
use super::*;
use crate::agents::AgentState;
use crate::agents::launch::Launch;
use crate::app::menus::MenuAction;
use crate::control::closed_agents::AgentsReopened;
use crate::control::{FlowRuns, Snapshot};

fn kill(agent: &Agent, id: u64) {
    let (pid, _) = pane(&agent.app, id).context.agent().expect("the agent's process");
    std::process::Command::new("kill").arg(pid.to_string()).status().expect("kill");
}

fn exited(app: &App) -> Vec<u64> {
    app.runs.all()[0].exited.clone()
}

fn rests(agent: &mut Agent, id: u64) {
    agent.refreshing("the agent rests", |app| {
        pane(app, id).agent.status().is_some_and(|status| status != activity::Status::Working)
    });
}

fn exited_coder(agent: &mut Agent) -> (u64, u64, String) {
    let [o, c] = team(agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    sends(agent, o, c, "build the form");
    working(agent, c);
    agent.finish();
    agent.refreshing("the orchestrator is told", |app| prompts_in(app, o) == 1);
    working(agent, o);
    agent.finish();
    rests(agent, o);
    rests(agent, c);
    let conversation = pane(&agent.app, c).resume.clone().expect("the conversation").conversation;
    kill(agent, c);
    agent.refreshing("the coder exited", |app| exited(app) == [c]);
    (o, c, conversation)
}

fn runs(agent: &mut Agent) -> FlowRuns {
    agent.app.clients.events.runs.set_every(Duration::ZERO);
    let answer = value(&mut agent.app, None, Command::FlowRuns(wire::FlowRunsWatch::default()));
    let fed: Snapshot<FlowRuns> = serde_json::from_value(answer).expect("a runs snapshot");
    fed.report
}

fn reopened(agent: &mut Agent) -> AgentsReopened {
    let workspace = agent.app.runs.all()[0].workspace;
    serde_json::from_value(ok(&mut agent.app, "agents-reopen", &json!({"workspace": workspace}))).expect("an answer")
}

#[test]
fn a_role_whose_agent_exits_keeps_its_conversation_and_shows_as_exited() {
    let mut agent = Agent::new();
    let (o, c, conversation) = exited_coder(&mut agent);

    let kept = pane(&agent.app, c).exited_agent.clone().expect("the conversation is kept");
    assert_eq!(kept.conversation, conversation);
    assert!(pane(&agent.app, c).resume.is_none());
    let report = runs(&mut agent);
    let roles: Vec<(u64, bool, Option<String>)> = report.runs[0]
        .roles
        .iter()
        .map(|role| (role.pane.unwrap_or_default(), role.exited, role.status.clone()))
        .collect();
    assert_eq!((roles[0].0, roles[0].1), (o, false));
    assert_eq!(roles[1], (c, true, None));
    let status = value(&mut agent.app, None, Command::Status(wire::Status {}));
    let projects = status["projects"].as_array().expect("projects").clone();
    let workspaces: Vec<Value> =
        projects.iter().flat_map(|p| p["workspaces"].as_array().cloned().unwrap_or_default()).collect();
    assert_eq!(workspaces[0]["run"]["exited"], json!(["coder"]));
    let workspace = agent.app.runs.all()[0].workspace;
    let menu = agent.app.closed_agents_menu(workspace);
    assert!(menu.contains(&MenuAction::ReopenAgents(workspace)));
    assert!(!menu.contains(&MenuAction::ForgetAgents(workspace)));
}

#[test]
fn reopening_brings_the_role_back_in_its_own_pane_and_conversation() {
    let mut agent = Agent::new();
    let (_, c, conversation) = exited_coder(&mut agent);
    let tab = agent.app.projects.tab_of_pane(c).map(|at| agent.app.projects.tab(at).id).expect("its tab");
    let tabs = agent.app.projects[0].workspaces[0].tabs.len();

    let answer = reopened(&mut agent);

    assert_eq!(answer.tabs, Vec::<u64>::new());
    assert_eq!(answer.run, Some(agent.app.runs.all()[0].id));
    let panes: Vec<_> =
        answer.panes.iter().map(|p| (p.pane, p.tab, p.agent.as_deref(), p.role.as_deref(), p.resumes)).collect();
    assert_eq!(panes, [(c, tab, Some("claude"), Some("coder"), true)]);
    assert_eq!(agent.app.projects[0].workspaces[0].tabs.len(), tabs);
    assert_eq!(pane(&agent.app, c).resume.as_ref().map(|a| a.conversation.clone()), Some(conversation));
    assert!(pane(&agent.app, c).resuming.is_some());
    assert!(pane(&agent.app, c).exited_agent.is_none());
    assert!(agent.app.agents.launches.of_pane(c).any(|launch| launch.resume.is_some()));
    assert_eq!(exited(&agent.app), Vec::<u64>::new());
    agent.app.agents.launches.drop_panes(&[c]);
}

#[test]
fn a_running_orchestrator_is_told_which_roles_came_back() {
    let mut agent = Agent::new();
    let (o, c, _) = exited_coder(&mut agent);
    reopened(&mut agent);
    agent.app.agents.launches.drop_panes(&[c]);

    agent.refreshing("the orchestrator is told", |app| prompts_in(app, o) == 2);
    agent.finish();

    agent.refreshing("the orchestrator answers", |app| {
        pane_screen(app, o).contains(&format!("coder (pane {c}) exited"))
    });
}

#[test]
fn a_run_whose_orchestrators_agent_exited_stops_once_no_member_works() {
    let mut agent = Agent::new();
    let [o, c] = team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    sends(&mut agent, o, c, "build the form");
    working(&mut agent, c);

    kill(&agent, o);
    agent.refreshing("the orchestrator exited", |app| exited(app) == [o]);
    reads(&mut agent, 2);
    assert_eq!(stopped(&mut agent), None, "a member works");

    agent.finish();
    agent.refreshing("the run stopped", |app| app.runs.all()[0].stopped.is_some());

    assert!(stopped(&mut agent).is_some());
    let report = runs(&mut agent);
    assert!(report.runs[0].roles[0].exited);
}

#[test]
fn a_role_with_no_conversation_starts_over_and_one_starting_is_not_exited() {
    let mut agent = Agent::new();
    let [o, c] = team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    kill(&agent, c);
    agent.refreshing("the coder exited", |app| exited(app) == [c]);
    assert!(pane(&agent.app, c).exited_agent.is_none());

    agent.app.agents.launches.add(Launch::command(c, "true".into(), Instant::now()));
    assert!(!agent.app.role_exited(c, Instant::now()), "a launch is in flight");
    agent.app.agents.launches.drop_panes(&[c]);
    assert!(agent.app.role_exited(c, Instant::now()));

    let answer = reopened(&mut agent);

    assert!(!answer.panes[0].resumes);
    assert!(agent.app.agents.launches.of_pane(c).next().is_some());
    let told = agent.app.runs.all()[0].sent.iter().any(|sent| sent.pane == c && sent.back);
    assert!(told, "the orchestrator {o} is to be told");
    agent.app.agents.launches.drop_panes(&[c]);
}

#[test]
fn nothing_exited_leaves_reopen_refused() {
    let mut agent = Agent::new();
    team(&mut agent, &[ORCHESTRATOR, "coder"]);
    let workspace = agent.app.runs.all()[0].workspace;

    let refused = refused(&mut agent.app, "agents-reopen", &json!({"workspace": workspace}));

    assert_eq!(refused.code, "conflict");
    assert!(refused.message.contains("has no closed agents"), "{}", refused.message);
}

fn kill_role(team: &Team, role: &str) {
    let id = team.app.runs.all()[0].pane_of(role).expect("the role");
    let pid = pane(&team.app, id).foreground_pid().expect("the agent's process");
    std::process::Command::new("kill").arg(pid.to_string()).status().expect("kill");
}

fn keep_conversation(team: &mut Team, role: &str, conversation: &str) {
    let id = team.app.runs.all()[0].pane_of(role).expect("the role");
    let agent = AgentState { kind: "claude".into(), conversation: conversation.into(), mode: None, account: None };
    team.app.projects.pane_by_mut(id).expect("the pane").exited_agent = Some(agent);
}

fn exited_in(team: &Team) -> usize {
    team.app.runs.all()[0].exited.len()
}

fn empty_log(team: &Team) {
    std::fs::write(team.log.path().join("order"), "").expect("empty the log");
}

fn team_reopens(team: &mut Team) {
    let workspace = team.app.runs.all()[0].workspace;
    ok(&mut team.app, "agents-reopen", &json!({"workspace": workspace}));
}

#[test]
fn a_roles_agent_that_exits_comes_back_with_its_roles_flags_and_conversation() {
    let mut team = Team::new();
    team.started();
    team.until_got(3);
    let reviewer = team.app.runs.all()[0].pane_of("reviewer").expect("the reviewer");
    let tabs = team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).collect::<Vec<_>>();

    kill_role(&team, "reviewer");
    refreshing(&mut team.app, &team.rx, "the reviewer exited", |app| app.runs.all()[0].exited == [reviewer]);
    keep_conversation(&mut team, "reviewer", "c2");
    empty_log(&team);
    team_reopens(&mut team);

    let dir = team.app.runs.all()[0].dir.as_ref().expect("the run's folder").display().to_string();
    let line = format!("start --dangerously-skip-permissions --model sonnet --add-dir {dir} --resume c2");
    let log = team.log.path().to_path_buf();
    until_logged(&mut team.app, &team.rx, "the reviewer starts again", || read_order(&log).contains(&line));
    assert_eq!(team.app.runs.all()[0].pane_of("reviewer"), Some(reviewer));
    let after = team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).collect::<Vec<_>>();
    assert_eq!(after, tabs);
}

#[test]
fn an_exited_orchestrator_comes_back_last_told_where_its_team_stands() {
    let mut team = Team::new();
    team.started();
    team.until_got(3);
    let coder = team.app.runs.all()[0].pane_of("coder").expect("the coder");

    kill_role(&team, ORCHESTRATOR);
    kill_role(&team, "coder");
    refreshing(&mut team.app, &team.rx, "both exited and the run stopped", |app| {
        app.runs.all()[0].exited.len() == 2 && app.runs.all()[0].stopped.is_some()
    });
    keep_conversation(&mut team, ORCHESTRATOR, "c0");
    empty_log(&team);
    team_reopens(&mut team);

    let dir = team.app.runs.all()[0].dir.as_ref().expect("the run's folder").display().to_string();
    let log = team.log.path().to_path_buf();
    until_logged(&mut team.app, &team.rx, "the orchestrator is told", || {
        read_order(&log).iter().any(|l| l.starts_with("got Your agent exited"))
    });
    let order = team.order();
    let fresh = format!("start --dangerously-skip-permissions --add-dir {dir}");
    let resumed = format!("start --dangerously-skip-permissions --model opus --add-dir {dir} --resume c0");
    assert!(position(&order, &fresh) < position(&order, &resumed), "{order:#?}");
    let told = &order[position(&order, "got Your agent exited")];
    assert!(told.contains(&format!("coder is pane {coder}")), "{told}");
    assert!(told.contains("coder starts over"), "{told}");
    assert_eq!(exited_in(&team), 0);
}
