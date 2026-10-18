use super::agents::in_status;
use super::*;

fn working(agent: &mut Agent, prompt: &str) -> u64 {
    let id = agent.start(None);
    ask(&mut agent.app, None, send_text(id, prompt, true, false));
    done(agent.answered("the enter is pressed"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    id
}

fn left_with_subagents() -> (Agent, u64) {
    let mut agent = Agent::new();
    let id = working(&mut agent, "run a subagent");
    agent.finish();
    agent.refreshing("the subagent runs", |a| pane(a, id).context.subagents() == 1);
    (agent, id)
}

#[test]
fn a_turn_left_with_subagents_keeps_every_wait_waiting() {
    let (mut agent, id) = left_with_subagents();

    ask(&mut agent.app, None, wait_for(id, Until::TurnOver, Some(0.3)));
    let timed_out = error(agent.answered("the wait times out"));
    agent.finish();
    ask(&mut agent.app, None, wait_for(id, Until::TurnOver, None));
    let ended = done(agent.answered("the subagent ends"));

    assert!(timed_out.contains("1 subagent it started still run"), "{timed_out}");
    assert_eq!(ended.ended.as_deref(), Some("done"));
}

#[test]
fn status_counts_the_subagents() {
    let (mut agent, id) = left_with_subagents();

    let info = in_status(&mut agent.app, id);
    let report: Report =
        serde_json::from_value(value(&mut agent.app, None, Command::Status(wire::Status {}))).expect("a report");
    let rows = report.panes();
    agent.finish();
    agent.refreshing("the subagent ended", |a| pane(a, id).context.subagents() == 0);

    assert_eq!((info.status.as_deref(), info.subagents, info.background_shell), (Some("working"), 1, false));
    assert_eq!(rows.iter().find(|row| row.pane == id).map(|row| row.subagents), Some(1));
}

#[test]
fn restart_when_idle_waits_for_the_subagents() {
    let (mut agent, _id) = left_with_subagents();

    ask(&mut agent.app, None, Command::RestartWhenIdle(wire::RestartWhenIdle { timeout: None }));
    let since = Instant::now();
    agent.refreshing("another look at the agents", |a| a.agents.watched.is_some_and(|at| at > since));
    let held = (answers(&mut agent.app), agent.app.take_restart().is_none());
    agent.finish();
    assert!(matches!(agent.answered("the subagent ends"), Response::Ok(_)));

    assert_eq!(held, (Vec::new(), true));
    assert!(agent.app.take_restart().is_some());
}
