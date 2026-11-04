use super::*;
use crate::queue::repeat::Repeat;
use crate::queue::{self, State as QueueState};

fn flow_routine(team: &mut Team, label: &str, flow: &str) -> u64 {
    team.app.set_queue_area(AREA);
    let at = crate::time::now() + 3600;
    let place = queue::Place { project: team.app.projects[0].path.clone(), ..queue::Place::default() };
    let task = queue::Task {
        at,
        zone: "UTC".into(),
        label: label.into(),
        prompt: "settle the login bug".into(),
        flow: Some(flow.into()),
        repeat: Some(Repeat::parse("2h", "UTC", at).expect("repeat")),
        place,
        ..queue::Task::default()
    };
    team.app.panels.queued.schedule(None, task).expect("routine")
}

#[test]
fn a_routine_starts_its_flow_on_a_routine_branch_with_its_prompt_as_the_task() {
    let mut team = Team::new();
    let id = flow_routine(&mut team, "nightly pair", "Pair");
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

fn duel() -> Flow {
    let apart = |name: &str| Role { worktree: crate::flows::Worktree::Own, ..role(name, None, true, "Compete.") };
    Flow {
        name: "Duel".into(),
        orchestrator: role(ORCHESTRATOR, None, false, "Be brief."),
        roles: vec![apart("coder A"), apart("coder B")],
        stages: vec![Stage { roles: vec!["coder A".into(), "coder B".into()], back: None }],
        ..Flow::default()
    }
}

#[test]
fn a_finished_flow_run_takes_its_role_worktrees_and_branches_with_it() {
    let mut team = Team::new();
    let flows = paths::flows(&team.config.path().join("config.json"));
    store::save(&flows, &duel(), None).expect("save the duel");
    let id = flow_routine(&mut team, "nightly duel", "Duel");
    team.app.panels.queued.queue_task_for_cleanup(id);
    let since = Instant::now();
    team.app.run_queued(id, since).expect("start now");
    let Team { app, rx, .. } = &mut team;
    wait_until_within("the flow's run starts", STARTING, || {
        drain(app, rx);
        app.refresh(Instant::now());
        app.panels.queued.queue().runs_of(id).any(|run| matches!(run.state, QueueState::Started { .. }))
    });
    let run = team.app.panels.queued.queue().runs_of(id).next().cloned().expect("run");
    let workspace = run.task.place.workspace_id.expect("workspace");
    let at = team.app.projects.workspace_index(workspace).expect("workspace");
    let trees: Vec<(PathBuf, String)> = team
        .app
        .projects
        .workspace(at)
        .role_worktrees
        .iter()
        .map(|tree| (tree.path.clone(), tree.branch.clone()))
        .collect();
    assert_eq!(trees.len(), 2);
    team.app.runs.run_mut(team.app.runs.of_workspace(workspace).expect("run").id).expect("run").ended =
        Some("done".into());
    let now = Instant::now() + Duration::from_secs(10);
    team.app.drive_cleanups(now);
    team.app.drive_cleanups(now + Duration::from_secs(181));
    let repo = team.app.projects[0].path.clone();
    let Team { app, rx, .. } = &mut team;
    wait_until_within("the run is removed", STARTING, || {
        drain(app, rx);
        app.panels.queued.queue().entry(run.id).is_some_and(|run| run.state == QueueState::Removed)
    });
    for (path, branch) in &trees {
        assert!(!path.exists(), "{}", path.display());
        wait_until_within("the branch goes", STARTING, || !git::has_branch(&repo, branch));
    }
}
