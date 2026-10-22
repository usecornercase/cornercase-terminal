use std::time::Duration;

use super::*;
use crate::agents::Known;
use crate::app::WATCH_AGENTS_EVERY;
use crate::control::{FlowRuns, Snapshot};
use crate::flows::run::{Member, Run};

fn team(agent: &mut Agent, roles: &[&str]) -> Vec<u64> {
    let panes: Vec<u64> = roles.iter().map(|_| agent.start(None)).collect();
    let workspace = agent.app.projects[0].workspaces[0].id;
    let mut run = Run::new(workspace, pair(), "task".into());
    run.members = roles.iter().zip(&panes).map(|(role, pane)| Member { role: (*role).into(), pane: *pane }).collect();
    agent.app.runs.push(run);
    panes
}

fn prompts_in(app: &App, id: u64) -> usize {
    let Some(context::Record::Claude(path)) = pane(app, id).context.record_for(Known::Claude) else { return 0 };
    std::fs::read_to_string(path).map_or(0, |text| text.matches("\"type\":\"user\"").count())
}

fn prompts(agent: &Agent, id: u64) -> usize {
    prompts_in(&agent.app, id)
}

fn reads(agent: &mut Agent, count: u32) {
    let start = agent.app.agents.watched().unwrap_or_else(Instant::now);
    let until = start + WATCH_AGENTS_EVERY * count;
    agent.refreshing("the agents are read again", |app| app.agents.watched().is_some_and(|at| at >= until));
}

fn working(agent: &mut Agent, id: u64) {
    agent.refreshing("the agent works", |app| pane(app, id).agent.status() == Some(activity::Status::Working));
}

fn sends(agent: &mut Agent, from: u64, to: u64, text: &str) {
    ask(&mut agent.app, Some(from), send_text(to, text, true, false));
    done(agent.answered("the send is confirmed"));
}

fn working_coder(agent: &mut Agent) -> (u64, u64) {
    let [o, c] = team(agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    sends(agent, o, c, "build the form");
    working(agent, c);
    (o, c)
}

fn sent(agent: &Agent) -> usize {
    agent.app.runs.all()[0].sent.len()
}

#[test]
fn a_role_that_finishes_sent_work_wakes_the_orchestrator_once() {
    let mut agent = Agent::new();
    let [o, c] = team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    sends(&mut agent, o, c, "build the form");
    assert_eq!(sent(&agent), 1);
    working(&mut agent, c);

    agent.finish();
    working(&mut agent, o);
    agent.finish();
    agent.refreshing("the orchestrator is told", |app| {
        pane_screen(app, o).contains(&format!("done: coder (pane {c}) finished"))
    });
    reads(&mut agent, 3);

    assert_eq!(prompts(&agent, o), 1, "one message");
    assert_eq!(sent(&agent), 0);
}

#[test]
fn a_wait_from_the_orchestrators_pane_keeps_it_quiet() {
    let mut agent = Agent::new();
    let (o, c) = working_coder(&mut agent);
    ask(&mut agent.app, Some(o), wait_for(c, Until::TurnOver, Some(60.0)));

    agent.finish();
    let ended = done(agent.answered("the wait ends"));
    reads(&mut agent, 3);

    assert_eq!(ended.ended.as_deref(), Some("idle"));
    assert_eq!(prompts(&agent, o), 0);
    assert_eq!(agent.status(o), Some(activity::Status::Idle));
}

#[test]
fn a_turn_the_person_takes_in_the_role_wakes_no_one() {
    let mut agent = Agent::new();
    let (o, c) = working_coder(&mut agent);
    agent.app.show(Item::Pane(c)).expect("the role's pane shows");
    type_line(&mut agent.app, "also the footer");

    agent.finish();
    agent.refreshing("the person's line is taken", |app| prompts_in(app, c) == 2);
    working(&mut agent, c);
    agent.finish();
    reads(&mut agent, 3);

    assert_eq!(prompts(&agent, o), 0);
    assert_eq!(sent(&agent), 0);
}

fn busy(agent: &Agent, id: u64, status: &str) {
    let (pid, _) = pane(&agent.app, id).context.agent().expect("the agent's process");
    let cwd = pane(&agent.app, id).cwd().expect("the folder");
    let file = agent.fake.dir().join("sessions").join(format!("{pid}.json"));
    let json = format!(r#"{{"pid":{pid},"sessionId":"s{pid}","cwd":"{}","status":"{status}"}}"#, cwd.display());
    std::fs::write(file, json).expect("rewrite the session");
}

#[test]
fn a_busy_orchestrator_hears_once_its_turn_is_over_about_every_role() {
    let mut agent = Agent::new();
    let [o, c, r] = team(&mut agent, &[ORCHESTRATOR, "coder", "reviewer"])[..] else { panic!("three panes") };
    sends(&mut agent, o, c, "build the form");
    sends(&mut agent, o, r, "review the form");
    working(&mut agent, c);
    working(&mut agent, r);
    busy(&agent, o, "busy");
    agent.refreshing("the orchestrator works", |app| pane(app, o).agent.status() == Some(activity::Status::Working));

    agent.finish();
    agent.refreshing("a role ended", |app| !app.runs.all()[0].due().is_empty());
    if agent.app.runs.all()[0].due().len() == 1 {
        agent.finish();
    }
    agent.refreshing("both roles ended", |app| app.runs.all()[0].due().len() == 2);
    reads(&mut agent, 3);
    assert_eq!(prompts(&agent, o), 0, "it is busy, so nobody types into it");

    busy(&agent, o, "idle");
    agent.refreshing("the orchestrator is told", |app| prompts_in(app, o) == 1);
    agent.finish();
    agent.refreshing("the orchestrator answers", |app| pane_screen(app, o).contains("done: coder"));
    assert_eq!(prompts(&agent, o), 1);
    assert!(
        pane_screen(&agent.app, o).contains(&format!("(pane {c}) and reviewer (pane {r})")),
        "{}",
        pane_screen(&agent.app, o)
    );
}

#[test]
fn a_role_awaited_shows_in_the_run_feed() {
    let mut agent = Agent::new();
    let [o, c] = team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    let awaited = |agent: &mut Agent| {
        agent.app.clients.events.runs.set_every(Duration::ZERO);
        let answer = value(&mut agent.app, None, Command::FlowRuns(wire::FlowRunsWatch::default()));
        let fed: Snapshot<FlowRuns> = serde_json::from_value(answer).expect("a runs snapshot");
        fed.report.runs[0]
            .roles
            .iter()
            .filter(|role| role.role == "coder")
            .map(|role| role.awaited.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(awaited(&mut agent), vec![None]);

    sends(&mut agent, o, c, "build the form");

    assert_eq!(awaited(&mut agent), vec![Some("working".to_string())]);
}

#[test]
fn with_the_orchestrators_tab_closed_the_person_is_told() {
    let mut agent = Agent::new();
    let (o, _) = working_coder(&mut agent);
    done(now(
        &mut agent.app,
        None,
        Command::Close(wire::Close { item: Item::Pane(o), remove_worktree: false, force: false, unlock: false }),
    ));

    agent.finish();
    agent
        .refreshing("the person is told", |app| toast(app).is_some_and(|toast| toast.starts_with("coder finished in")));

    assert_eq!(sent(&agent), 0);
}
