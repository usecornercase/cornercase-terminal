use super::*;

fn run_from(team: &mut Team, caller: Option<u64>) -> RunInfo {
    let run = wire::FlowRun { name: "Pair".into(), prompt: "Fix the login form".into(), ..wire::FlowRun::default() };
    ask(&mut team.app, caller, Command::FlowRun(run));
    match answered(&mut team.app, &team.rx, "the flow's tabs are open") {
        Response::Ok(value) => serde_json::from_value(value).expect("the run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    }
}

fn parents(app: &App, run: &RunInfo) -> Vec<(String, Option<u64>)> {
    run.members.iter().map(|m| (m.role.clone(), pane(app, m.pane).parent)).collect()
}

#[test]
fn every_member_of_a_run_started_from_outside_hangs_from_the_orchestrator() {
    let mut team = Team::new();

    let run = run_from(&mut team, None);

    let own = run.members[0].pane;
    let expected =
        [(ORCHESTRATOR.to_string(), None), ("coder".to_string(), Some(own)), ("reviewer".to_string(), Some(own))];
    assert_eq!(parents(&team.app, &run), expected);
}

#[test]
fn a_run_started_from_a_pane_hangs_its_orchestrator_from_that_pane() {
    let mut team = Team::new();
    let caller = first(&team.app);

    let run = run_from(&mut team, Some(caller));

    let own = run.members[0].pane;
    let expected = [
        (ORCHESTRATOR.to_string(), Some(caller)),
        ("coder".to_string(), Some(own)),
        ("reviewer".to_string(), Some(own)),
    ];
    assert_eq!(parents(&team.app, &run), expected);
}

#[test]
fn restored_members_hang_from_the_restored_orchestrator() {
    let mut team = Team::new();
    run_from(&mut team, None);
    let saved = team.app.state();
    let (mut again, _rx) = new_app(team.config.path().join("config.json"));

    assert!(again.restore(&saved, AREA));

    let own = again.runs.all()[0].pane_of(ORCHESTRATOR).expect("the orchestrator");
    let members: Vec<(String, Option<u64>)> =
        again.runs.all()[0].members.iter().map(|m| (m.role.clone(), pane(&again, m.pane).parent)).collect();
    let expected =
        [(ORCHESTRATOR.to_string(), None), ("coder".to_string(), Some(own)), ("reviewer".to_string(), Some(own))];
    assert_eq!(members, expected);
}
