use super::super::super::flow_panel::team;
use super::*;
use crate::app::flow_runs::Starting;
use crate::control::{FlowRuns, RunState};
use crate::flows::ORCHESTRATOR;

fn closed_runs(app: &App) -> Vec<RunState> {
    let FlowRuns { runs, .. } = app.flow_runs_state();
    runs
}

fn with_resumes(app: &mut App) {
    let panes: Vec<(String, u64)> =
        app.runs.all()[0].members.iter().map(|member| (member.role.clone(), member.pane)).collect();
    for (role, pane) in panes {
        let term = app.projects.pane_by_mut(pane).expect("the pane");
        term.agent.follow(Some("claude"));
        term.resume = Some(resume_state(&format!("c-{role}")));
    }
}

fn team_closed() -> (App, Receiver<AppEvent>, TempDir, u64) {
    let (mut app, rx, dir) = team();
    app.config.agent_commands.insert("claude".into(), "/bin/sh".into());
    with_resumes(&mut app);
    let id = workspace(&app);
    ok(&mut app, "agents-close", &json!({"workspace": id, "force": true}));
    pump_until(&mut app, &rx, "the tabs go", |a| a.projects[0].workspaces[0].tabs.len() == 1);
    (app, rx, dir, id)
}

#[test]
fn closing_a_flows_agents_keeps_its_run_whole_and_out_of_the_live_ones() {
    let (app, _rx, _dir, id) = team_closed();

    assert!(app.runs.all().is_empty());
    assert!(app.agents.closed.has_run(id));
    let runs = closed_runs(&app);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].state.as_deref(), Some("closed"));
    assert!(runs[0].roles.iter().all(|role| role.pane.is_none() && role.closed));
    assert!(!app.has_run());
}

#[test]
fn reopening_brings_the_whole_team_back_with_the_same_run() {
    let (mut app, rx, _dir, id) = team_closed();
    let run_id = closed_runs(&app)[0].run;

    let reopened: AgentsReopened = parsed(ok(&mut app, "agents-reopen", &json!({"workspace": id})));

    assert_eq!(reopened.run, Some(run_id));
    assert_eq!(app.runs.all().len(), 1);
    let run = &app.runs.all()[0];
    assert_eq!((run.id, run.members.len()), (run_id, 5));
    let own = run.pane_of(ORCHESTRATOR).expect("the orchestrator");
    let launch = app.agents.launches.of_pane(own).next().expect("the orchestrator is briefed");
    assert_eq!(launch.after.len(), 4);
    assert!(
        run.members
            .iter()
            .filter(|member| member.pane != own)
            .all(|member| { app.projects.pane_by(member.pane).and_then(|term| term.parent) == Some(own) })
    );
    refreshing(&mut app, &rx, "the members' conversations are typed", |a| {
        panes_on_screen(a).contains("--resume c-coder")
    });
}

#[test]
fn a_role_closed_earlier_comes_back_fresh_in_a_tab_named_after_it() {
    let (mut app, rx, _dir) = team();
    app.config.agent_commands.insert("claude".into(), "/bin/sh".into());
    with_resumes(&mut app);
    let id = workspace(&app);
    let tester = app.runs.all()[0].pane_of("tester").expect("a tester");
    app.projects.pane_by_mut(tester).expect("the pane").kill();
    pump_until(&mut app, &rx, "the tester goes", |a| a.projects.pane_by(tester).is_none());
    ok(&mut app, "agents-close", &json!({"workspace": id, "force": true}));
    pump_until(&mut app, &rx, "the tabs go", |a| a.projects[0].workspaces[0].tabs.len() == 1);

    let reopened: AgentsReopened = parsed(ok(&mut app, "agents-reopen", &json!({"workspace": id})));

    let fresh = reopened.panes.iter().find(|pane| pane.role.as_deref() == Some("tester")).expect("the tester");
    assert!(!fresh.resumes);
    let tab = app.projects[0].workspaces[0].tabs.iter().find(|tab| tab.id == fresh.tab).expect("its tab");
    assert_eq!(tab.name.as_deref(), Some("tester"));
    assert_eq!(app.runs.all()[0].members.len(), 5);
}

#[test]
fn a_new_run_is_refused_while_the_agents_are_closed_and_accepted_after_forgetting() {
    let (mut app, _rx, _dir, id) = team_closed();

    let starting = Starting::new(
        crate::flows::builtin::flows(&crate::agents::Known::ALL, Some(crate::agents::Known::Claude)).remove(0),
        "again".into(),
        "main".into(),
        app.active_accounts(),
    );
    let refusal = app.open_run(workspace_at(&app, 0, 0), starting, AREA, false).expect_err("a run is open");
    assert_eq!(
        refusal,
        "workspace has a flow's agents closed: reopen or forget them".replace("workspace", &format!("workspace {id}"))
    );

    ok(&mut app, "agents-forget", &json!({"workspace": id}));
    assert!(!app.agents.closed.has_run(id));
}

#[test]
fn forgetting_ends_the_run() {
    let (mut app, _rx, _dir, id) = team_closed();

    let forgot: AgentsForgot = parsed(ok(&mut app, "agents-forget", &json!({"workspace": id})));

    assert!(forgot.run.is_some());
    assert_eq!(closed_runs(&app).len(), 0);
}
