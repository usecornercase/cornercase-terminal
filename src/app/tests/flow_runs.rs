use super::flow_panel::team;
use super::*;
use crate::agents::AgentState;
use crate::agents::activity::Activity;
use crate::flows::ORCHESTRATOR;
use crate::flows::progress::Edge;
use crate::flows::run::{Kind, Message};

fn send(app: &mut App, to: &str) {
    let message = Message {
        from: ORCHESTRATOR.into(),
        to: to.into(),
        kind: Kind::Send,
        text: None,
        bytes: 0,
        at: Instant::now(),
        time: SystemTime::now(),
        seq: 0,
        edge: None,
    };
    app.runs.list[0].record(message);
}

fn screen(app: &App, pane: u64) -> String {
    app.projects.pane_by(pane).and_then(|term| term.emulator.screen_text().ok()).unwrap_or_default()
}

#[test]
fn a_pane_where_another_agent_ran_resumes_that_agent_and_its_orchestrator_is_not_briefed() {
    let (app, _rx, _dir) = team();
    let mut saved = app.state();
    let workspace = saved.projects[0].workspaces.iter_mut().find(|w| w.run.is_some()).expect("the run is saved");
    let seats: Vec<(String, usize, usize)> =
        workspace.run.as_ref().expect("the run").members.iter().map(|s| (s.role.clone(), s.tab, s.pane)).collect();
    for (role, tab, pane) in &seats {
        let kind = if role == ORCHESTRATOR || role == "coder" { "codex" } else { "claude" };
        let agent = AgentState { kind: kind.into(), conversation: format!("c-{role}"), mode: None, account: None };
        workspace.tabs[*tab].panes[*pane].agent = Some(agent);
    }
    let (mut again, rx) = new_app(no_config());

    assert!(again.restore(&saved, AREA));

    let pane = |role: &str| again.runs.list[0].pane_of(role).expect("a member");
    let (orchestrator, coder, tester) = (pane(ORCHESTRATOR), pane("coder"), pane("tester"));
    let briefed = again.agents.launches.iter().find(|launch| launch.term == orchestrator).map(|l| !l.after.is_empty());
    assert_eq!(briefed, Some(false), "an orchestrator pane that ran Codex gets no brief");
    let trusted = |pane: u64| again.agents.launches.iter().find(|l| l.term == pane).map(Launch::trusted);
    assert_eq!((trusted(coder), trusted(tester)), (Some(false), Some(true)), "only the role's own agent is trusted");
    refreshing(&mut again, &rx, "the conversations are typed", |a| {
        screen(a, orchestrator).contains("codex resume c-orchestrator")
            && screen(a, coder).contains("codex resume c-coder")
            && screen(a, tester).contains("claude --dangerously-skip-permissions --resume c-tester")
    });
}

#[test]
fn the_plan_goes_on_where_it_stood_after_a_restart() {
    let (mut app, _rx, _dir) = team();
    for to in ["coder", "reviewer", "coder", "reviewer"] {
        send(&mut app, to);
    }
    let (mut again, _rx) = new_app(no_config());

    assert!(again.restore(&app.state(), AREA));
    send(&mut again, "tester");

    let progress = &again.runs.list[0].progress;
    assert_eq!((progress.active, progress.rounds(1)), (Some(2), 1));
    assert_eq!(progress.last.as_ref().map(|step| step.edge), Some(Edge::Forward(1)));
    assert_eq!(progress.outside, None);
}

#[test]
fn a_restored_run_keeps_its_latched_end() {
    let (mut app, _rx, _dir) = team();
    app.runs.list[0].ended = Some("feat/x".into());
    let (mut again, _rx2) = new_app(no_config());

    assert!(again.restore(&app.state(), AREA));

    assert_eq!(again.runs.list[0].ended.as_deref(), Some("feat/x"));
    let end = again.flow_view(Instant::now(), None).and_then(|view| view.diagram.live).expect("live").end;
    assert!(end.reached);
}

#[test]
fn the_roles_come_back_after_a_restart() {
    let (app, _rx, _dir) = team();
    let (mut again, _rx) = new_app(no_config());

    assert!(again.restore(&app.state(), AREA));
    for tab in &mut again.projects[0].workspaces[0].tabs {
        for term in &mut tab.panes {
            term.agent.follow(Some(agents::Known::Claude.name()));
            term.agent.update(Some(Activity::Idle), true, Instant::now());
        }
    }
    again.runs.list[0].members.iter().for_each(|member| assert!(again.projects.pane_by(member.pane).is_some()));

    let seated: Vec<(Option<String>, bool)> =
        again.agents_view().entries.into_iter().map(|e| (e.role, e.place)).collect();
    let (orchestrators, members): (Vec<_>, Vec<_>) =
        seated.iter().filter(|(role, _)| role.is_some()).partition(|(role, _)| role.as_deref() == Some(ORCHESTRATOR));
    assert_eq!(
        (orchestrators, members.iter().all(|(_, place)| !place)),
        (vec![&(Some(ORCHESTRATOR.to_string()), true)], true)
    );
}
