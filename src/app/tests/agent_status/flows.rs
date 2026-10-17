use super::*;
use crate::flows::run::{Member, Run};
use crate::flows::{Flow, ORCHESTRATOR};

#[rstest]
#[case::a_member_that_finishes(Some("coder"), "idle", false, Status::Idle)]
#[case::a_member_that_needs_you(Some("coder"), "waiting", true, Status::Waiting)]
#[case::the_orchestrator_that_finishes(Some(ORCHESTRATOR), "idle", true, Status::Done)]
#[case::an_agent_in_no_run(None, "idle", true, Status::Done)]
fn in_a_flow_a_member_finishing_a_turn_tells_no_one_and_leaves_no_mark(
    #[case] role: Option<&str>,
    #[case] status: &str,
    #[case] told: bool,
    #[case] shown: Status,
) {
    let mut w = Watched::start(agents::Known::Claude, true);
    if let Some(role) = role {
        let pane = tab_term(&w.app, 0, 0).id;
        let mut run = Run::new(w.app.projects[0].workspaces[0].id, Flow::default(), "fix it".into());
        run.members.push(Member { role: role.into(), pane });
        w.app.runs.push(run);
    }

    w.report(status, 6);

    let (toast, notifications) = w.told();
    assert_eq!((toast.is_some(), !notifications.is_empty()), (told, told), "{toast:?} {notifications:?}");
    assert_eq!(tab_term(&w.app, 0, 0).agent.status(), Some(shown));
}

#[test]
fn a_run_that_ends_marked_for_review_says_it_finished() {
    let mut w = Watched::start(agents::Known::Claude, true);
    let pane = tab_term(&w.app, 0, 0).id;
    let mut run = Run::new(w.app.projects[0].workspaces[0].id, Flow::default(), "fix it".into());
    run.members.push(Member { role: ORCHESTRATOR.into(), pane });
    w.app.runs.push(run);
    w.app.projects.pane_by_mut(pane).expect("the orchestrator").agent.mark(true);

    w.report("idle", 6);

    assert_eq!(tab_term(&w.app, 0, 0).agent.status(), Some(Status::Pending));
    let text = format!("claude finished in {}", w.place());
    let token = w.token();
    assert_eq!(w.told().1, [Notification { text, channel: None, token }]);
}
