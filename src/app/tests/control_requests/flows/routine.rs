use super::*;
use crate::queue::repeat::Repeat;
use crate::queue::{self, State as QueueState};

fn flow_routine(team: &mut Team, label: &str) -> u64 {
    team.app.set_queue_area(AREA);
    let at = crate::time::now() + 3600;
    let place = queue::Place { project: team.app.projects[0].path.clone(), ..queue::Place::default() };
    let task = queue::Task {
        at,
        zone: "UTC".into(),
        label: label.into(),
        prompt: "settle the login bug".into(),
        flow: Some("Pair".into()),
        repeat: Some(Repeat::parse("2h", "UTC", at).expect("repeat")),
        place,
        ..queue::Task::default()
    };
    team.app.panels.queued.schedule(None, task).expect("routine")
}

#[test]
fn a_routine_starts_its_flow_on_a_routine_branch_with_its_prompt_as_the_task() {
    let mut team = Team::new();
    let id = flow_routine(&mut team, "nightly pair");
    team.app.run_queued(id, Instant::now()).expect("start now");
    let Team { app, rx, .. } = &mut team;
    wait_until_within("the flow's run starts", STARTING, || {
        drain(app, rx);
        app.refresh(Instant::now());
        app.panels.queued.queue().runs_of(id).any(|run| matches!(run.state, QueueState::Started { .. }))
    });
    let run = team.app.panels.queued.queue().runs_of(id).next().cloned().expect("run");
    let branch = run.task.place.branch.clone().expect("branch");
    assert!(branch.starts_with("routine/nightly-pair-"), "{branch}");
    let workspace = run.task.place.workspace_id.expect("the run's workspace");
    let flow_run = team.app.runs.of_workspace(workspace).expect("the flow's run");
    assert_eq!(flow_run.task, "settle the login bug");
    let QueueState::Started { pane, .. } = run.state else { panic!("started") };
    assert_eq!(Some(pane), flow_run.pane_of(ORCHESTRATOR));
    assert!(git::branch(&run.task.place.workspace.expect("folder")).is_some_and(|found| found == branch));
    team.until_got(3);
    assert!(team.order().iter().any(|line| line.contains("settle the login bug")), "{:#?}", team.order());
}
