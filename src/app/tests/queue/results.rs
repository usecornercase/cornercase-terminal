use super::*;
use crate::agents::context::Said;
use crate::error::Error;
use crate::queue::outcome::{CLOSING, Pull, Taken};

fn opened() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (app, rx, repo, config, worktrees) = queued_app();
    (app, rx, vec![repo, config, worktrees])
}

fn started(app: &mut App) -> u64 {
    let id = app.panels.queued.schedule(None, task(app, time::now() + 3600, None)).expect("entry");
    app.panels.queued.started(id, 999);
    id
}

fn said(text: &str) -> Said {
    Said { text: text.into(), written: None }
}

fn text_of(app: &App, id: u64) -> String {
    app.queue_shown(id).expect("shown").result.map(|result| result.text).unwrap_or_default()
}

#[test]
fn a_read_that_fails_keeps_the_previous_result() {
    let (mut app, _rx, _dirs) = opened();
    let id = started(&mut app);
    app.queue_captured(id, 1, Taken::default(), Ok(Some(said("first"))));

    app.queue_captured(id, 2, Taken::default(), Err(Error::Control("boom".into())));

    assert_eq!(text_of(&app, id), "first");
}

#[test]
fn an_older_read_that_finishes_late_is_dropped() {
    let (mut app, _rx, _dirs) = opened();
    let id = started(&mut app);

    app.queue_captured(id, 2, Taken::default(), Ok(Some(said("two"))));
    app.queue_captured(id, 1, Taken::default(), Ok(Some(said("one"))));

    assert_eq!(text_of(&app, id), "two");
}

#[test]
fn a_custom_agents_capture_keeps_the_branch_and_pull_request() {
    let (mut app, _rx, _dirs) = opened();
    let id = started(&mut app);
    let pull = Pull { number: 4, title: "Fix".into(), state: "open".into(), ..Pull::default() };
    let taken = Taken {
        agent: Some("fake".into()),
        branch: Some("feat/x".into()),
        pull_request: Some(pull.clone()),
        ..Taken::default()
    };

    app.queue_captured(id, 1, taken, Ok(None));

    let shown = app.queue_shown(id).expect("shown");
    let result = shown.result.expect("a result");
    assert_eq!((result.branch.as_deref(), result.pull_request, result.text.as_str()), (Some("feat/x"), Some(pull), ""));
    assert!(shown.note.is_some_and(|note| note.contains("keeps no record")));
}

#[test]
fn the_closing_line_is_sent_and_never_stored() {
    let (mut app, _rx, _dirs) = opened();
    let id = started(&mut app);
    app.panels.queued.set_state(id, QueueState::Pending);

    let client = app.panels.queued.start(id, Instant::now());
    let spec = app.queued_spec(client).expect("a spec");

    assert_eq!(spec.prompt, Some(format!("fix the login\n\n{CLOSING}")));
    assert_eq!(app.panels.queued.queue().entry(id).expect("entry").task.prompt, "fix the login");
    assert!(!serde_json::to_string(&app.queue_saved()).expect("saved").contains(CLOSING));
}
