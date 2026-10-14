use super::flow_panel::team;
use super::*;
use crate::agents::AgentState;
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
    };
    app.runs[0].record(message);
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
        let agent = AgentState { kind: kind.into(), conversation: format!("c-{role}"), mode: None };
        workspace.tabs[*tab].panes[*pane].agent = Some(agent);
    }
    let (mut again, rx) = new_app(no_config());

    assert!(again.restore(&saved, AREA));

    let pane = |role: &str| again.runs[0].pane_of(role).expect("a member");
    let (orchestrator, coder, tester) = (pane(ORCHESTRATOR), pane("coder"), pane("tester"));
    let briefed = again.agents.launches.iter().find(|launch| launch.term == orchestrator).map(Launch::kind);
    assert_eq!(briefed, Some("command"), "an orchestrator pane that ran Codex gets no brief");
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

    let progress = &again.runs[0].progress;
    assert_eq!((progress.active, progress.rounds(1)), (Some(2), 1));
    assert_eq!(progress.last.as_ref().map(|step| step.edge), Some(Edge::Forward(1)));
    assert_eq!(progress.outside, None);
}
