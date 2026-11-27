use super::*;
use crate::control::{Command, Mark, Request};

fn mark(w: &mut Watched, reviewed: bool, seen: bool) -> serde_json::Value {
    let pane = w.app.projects[0].workspaces[0].tabs[0].panes[0].id;
    let command = Command::Mark(Mark { pane: Some(pane), tab: None, reviewed, seen });
    let request = Request {
        caller: None,
        server: None,
        contract: Some(1),
        boot: None,
        view: None,
        bot: None,
        bot_run: None,
        command,
    };
    w.app.request(1, &serde_json::to_string(&request).expect("json"), Some(AREA), Instant::now());
    let (_, answer) = w.app.take_answers().pop().expect("an answer");
    serde_json::from_str(&answer).expect("a response")
}

#[rstest]
#[case::done(false, Status::Idle)]
#[case::done_and_pending_review(true, Status::Pending)]
fn a_seen_mark_takes_only_the_done_mark_off(#[case] pending: bool, #[case] wanted: Status) {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.report("idle", 6);
    if pending {
        assert!(mark(&mut w, false, false).get("ok").is_some());
    }
    let agent = |w: &Watched| w.app.projects[0].workspaces[0].tabs[0].panes[0].agent.unseen();
    assert!(agent(&w));

    let answer = mark(&mut w, false, true);
    tick(&mut w.app);

    assert!(answer.get("ok").is_some(), "{answer}");
    assert!(!agent(&w));
    assert_eq!(status(&w.app, 0, 0), Some(wanted));
}

#[test]
fn a_mark_cannot_be_both_seen_and_reviewed() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.report("idle", 6);

    let answer = mark(&mut w, true, true);

    assert_eq!(answer["error"]["code"], "malformed", "{answer}");
    assert_eq!(status(&w.app, 0, 0), Some(Status::Done));
}
