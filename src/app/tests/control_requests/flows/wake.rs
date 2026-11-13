use std::time::Duration;

use super::*;
use crate::agents::Known;
use crate::app::WATCH_AGENTS_EVERY;
use crate::control::{FlowRuns, Snapshot};
use crate::flows::run::{Member, RETRY_AFTER, Run};

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

#[test]
fn a_role_whose_turn_ended_on_an_api_error_wakes_the_orchestrator_saying_the_work_was_not_done() {
    let mut agent = Agent::new();
    let [o, c] = team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    sends(&mut agent, o, c, "api-error build the form");
    working(&mut agent, c);

    agent.finish();
    working(&mut agent, o);
    agent.finish();
    agent.refreshing("the orchestrator is told", |app| {
        pane_screen(app, o).contains(&format!("done: coder (pane {c}) ended its turn on an error"))
    });
    reads(&mut agent, 3);

    assert_eq!(prompts(&agent, o), 1, "one message");
    assert_eq!(sent(&agent), 0);
}

fn failed_orchestrator(agent: &mut Agent) -> u64 {
    let [o, _] = team(agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    ask(&mut agent.app, None, send_text(o, "api-error go", true, false));
    done(agent.answered("the send is confirmed"));
    working(agent, o);
    agent.finish();
    agent.refreshing("the orchestrator failed", |app| {
        pane(app, o).agent.status() == Some(activity::Status::Failed)
            && app.runs.all()[0].retry.due(Instant::now() + RETRY_AFTER)
    });
    o
}

fn failed_long_ago(agent: &mut Agent) {
    let long_ago = Instant::now().checked_sub(RETRY_AFTER * 2).expect("a while ago");
    let retry = &mut agent.app.runs.first_mut().retry;
    retry.saw(None, long_ago);
    retry.saw(Some("failed"), long_ago);
}

#[test]
fn an_orchestrator_whose_own_turn_ended_on_an_api_error_is_woken_a_minute_later() {
    let mut agent = Agent::new();
    let o = failed_orchestrator(&mut agent);
    reads(&mut agent, 3);
    assert_eq!(prompts(&agent, o), 1, "not woken before the minute");

    failed_long_ago(&mut agent);
    agent.refreshing("the orchestrator is woken", |app| prompts_in(app, o) == 2);
    agent.finish();
    agent.refreshing("the orchestrator answers", |app| {
        pane_screen(app, o).contains("done: Your last turn ended on an error")
    });

    assert_eq!(prompts(&agent, o), 2);
}

#[test]
fn an_ended_run_is_not_retried() {
    let mut agent = Agent::new();
    let o = failed_orchestrator(&mut agent);
    agent.app.runs.first_mut().ended = Some("fix-login".into());

    failed_long_ago(&mut agent);
    reads(&mut agent, 4);

    assert_eq!(prompts(&agent, o), 1);
}

fn stopped(agent: &mut Agent) -> Option<String> {
    agent.app.clients.events.runs.set_every(Duration::ZERO);
    let answer = value(&mut agent.app, None, Command::FlowRuns(wire::FlowRunsWatch::default()));
    let fed: Snapshot<FlowRuns> = serde_json::from_value(answer).expect("a runs snapshot");
    fed.report.runs[0].stopped.clone()
}

#[test]
fn a_run_stopped_moving_while_its_orchestrator_and_team_rest_and_not_while_one_works() {
    let mut agent = Agent::new();
    let [o, c] = team(&mut agent, &[ORCHESTRATOR, "coder"])[..] else { panic!("two panes") };
    agent.refreshing("the run stopped", |app| app.runs.all()[0].stopped.is_some());
    let at = stopped(&mut agent).expect("a time");
    assert!(crate::time::Stamp::parse(&at).is_some(), "{at}");
    reads(&mut agent, 2);
    assert_eq!(stopped(&mut agent), Some(at), "the moment it stopped does not move");

    sends(&mut agent, o, c, "build the form");
    working(&mut agent, c);
    agent.refreshing("the run moves", |app| app.runs.all()[0].stopped.is_none());

    assert_eq!(stopped(&mut agent), None);
}

#[test]
fn a_send_the_orchestrator_waits_for_still_carries_the_warning_of_a_skipped_stage() {
    let mut agent = Agent::new();
    let [o, _, _, t] = team(&mut agent, &[ORCHESTRATOR, "coder", "reviewer", "tester"])[..] else {
        panic!("four panes")
    };
    let run = agent.app.runs.first_mut();
    run.flow.stages.push(Stage { roles: vec!["tester".into()], back: None });
    for to in ["coder", "reviewer", "coder"] {
        let message = crate::flows::run::Message {
            from: ORCHESTRATOR.into(),
            to: to.into(),
            kind: crate::flows::run::Kind::Send,
            text: None,
            bytes: 0,
            at: Instant::now(),
            time: SystemTime::now(),
            seq: 0,
            edge: None,
        };
        run.record(message);
    }

    ask(&mut agent.app, Some(o), send_text(t, "test it", true, false));
    let answer = done(agent.answered("the send is confirmed"));

    assert_eq!(
        answer.warning.as_deref(),
        Some(
            "stage 2 (reviewer) has not run since the last change in stage 1: run it first, or say in your summary \
             why not"
        )
    );
}
