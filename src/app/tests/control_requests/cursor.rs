use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::agents::cursor::fake::FakeCursor;

struct Running {
    app: App,
    rx: Receiver<AppEvent>,
    fake: FakeCursor,
    _dirs: Vec<TempDir>,
}

fn millis(at: SystemTime) -> u64 {
    u64::try_from(at.duration_since(UNIX_EPOCH).expect("clock").as_millis()).expect("millis")
}

impl Running {
    fn new() -> Self {
        let (mut app, rx, dirs) = app_with(1);
        let fake = FakeCursor::new(&app.projects[0].path);
        type_line(&mut app, &fake.command_line());
        refreshing(&mut app, &rx, "Cursor holds its store", |a| {
            a.term().is_some_and(|t| t.context.record_for(crate::agents::Known::Cursor).is_some())
        });
        Self { app, rx, fake, _dirs: dirs }
    }

    fn send(&mut self, text: &str) {
        send_enter(&mut self.app, text);
    }
}

#[test]
fn a_later_submission_confirms_a_pending_send() {
    let mut running = Running::new();
    let now = SystemTime::now();
    running.fake.turn_at("first", Some(millis(now - Duration::from_secs(60))));
    refreshing(&mut running.app, &running.rx, "the first prompt is recorded", |a| {
        a.term().is_some_and(|term| term.context.prompted().is_some())
    });
    let id = first(&running.app);

    running.send("next");
    running.app.refresh(Instant::now());
    assert_eq!(answers(&mut running.app).len(), 0);
    running.fake.turn_at("next", Some(millis(now + Duration::from_secs(60))));

    assert_eq!(done(answered(&mut running.app, &running.rx, "the later prompt confirms")).ids.pane, Some(id));
}

#[test]
fn an_unconfirmed_send_fails_instead_of_hanging() {
    let mut running = Running::new();

    an_unconfirmed_send_gives_up(&mut running.app, &running.rx);
}

#[test]
fn the_last_message_is_the_reply_and_not_the_thinking_or_the_tools() {
    let mut running = Running::new();
    let id = first(&running.app);
    running.fake.turn("fix the login");
    running.fake.reply("The login is fixed.");
    running.fake.tool("Reading src/login.rs");
    running.fake.thinking("let me check");
    running.fake.end("success");
    refreshing(&mut running.app, &running.rx, "the pane knows its conversation and the turn is over", |a| {
        a.term().is_some_and(|t| t.context.conversation().is_some() && t.agent.status() == Some(activity::Status::Idle))
    });

    let message = last_message(&mut running.app, &running.rx, id);

    assert_eq!((message["agent"].as_str(), message["text"].as_str()), (Some("cursor"), Some("The login is fixed.")));
    assert_eq!(message["turn_over"], true);
}

#[rstest::rstest]
#[case::slash("/plan fix it")]
#[case::shell("!ls")]
fn local_commands_return_after_enter_without_a_prompt_record(#[case] text: &str) {
    let mut running = Running::new();

    a_local_command_needs_no_record(&mut running.app, &running.rx, text);
}

#[test]
fn send_refuses_a_cursor_that_waits_for_confirmation() {
    let mut running = Running::new();
    running.fake.title("Cursor Agent - 🔐 Waiting for confirmation");
    refreshing(&mut running.app, &running.rx, "Cursor waits", |a| {
        a.term().and_then(|t| t.agent.status()) == Some(activity::Status::Waiting)
    });

    running.send("continue");

    assert!(error(answers(&mut running.app).remove(0)).contains("waiting for an answer"));
}

#[test]
fn two_cursor_in_one_folder_each_follow_their_own_conversation() {
    let (mut app, rx, _dirs) = app_with(1);
    let folder = app.projects[0].path.clone();
    let (one, other) = (FakeCursor::new(&folder), FakeCursor::new(&folder));
    type_line(&mut app, &one.command_line());
    let first_pane = first(&app);
    app.add_tab(0, 0, AREA).expect("a second tab");
    type_line(&mut app, &other.command_line());
    let second_pane = app.projects[0].workspaces[0].tabs[1].pane().map(|t| t.id).expect("the second pane");
    for id in [first_pane, second_pane] {
        refreshing(&mut app, &rx, "each Cursor holds a store", |a| {
            pane(a, id).context.record_for(crate::agents::Known::Cursor).is_some()
        });
    }
    one.turn("one");
    one.reply("first answer");

    refreshing(&mut app, &rx, "only the first has a conversation", |a| {
        pane(a, first_pane).context.conversation().is_some()
    });
    assert!(pane(&app, second_pane).context.conversation().is_none());
    other.turn("other");
    other.reply("second answer");
    refreshing(&mut app, &rx, "the second has one too", |a| pane(a, second_pane).context.conversation().is_some());

    let records = [first_pane, second_pane].map(|id| pane(&app, id).context.record_for(crate::agents::Known::Cursor));
    assert_ne!(records[0], records[1]);
}

#[test]
fn a_todo_cursor_cannot_confirm_is_sent_but_stays_pending() {
    let mut running = Running::new();
    let id = first(&running.app);
    let todo = running.app.panels.todos.add("/plan the login").expect("an id");

    ask(&mut running.app, None, Command::TodoSend(wire::TodoSend { id: todo, pane: Some(id), ..Default::default() }));
    let sent = done(answered(&mut running.app, &running.rx, "the enter is pressed"));

    let pending = running.app.panels.todos.item(todo).is_some_and(|item| !item.done);
    assert_eq!((sent.todo, sent.unconfirmed, pending), (Some(todo), Some(true), true));
}

#[test]
fn a_draft_typed_into_cursor_holds_a_send_until_the_line_is_cleared() {
    let mut running = Running::new();
    let id = first(&running.app);
    type_text(&mut running.app, "my own question");

    running.send("next");
    running.app.refresh(Instant::now());
    assert_eq!((answers(&mut running.app).len(), running.app.held_sends(id)), (0, 1));
    assert!(pane(&running.app, id).draft(Instant::now()), "the person's draft is seen");

    send_key(&mut running.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    refreshing(&mut running.app, &running.rx, "the cleared line lets the send go", |a| a.held_sends(id) == 0);
    running.fake.turn_at("next", Some(millis(SystemTime::now() + Duration::from_secs(60))));

    assert_eq!(done(answered(&mut running.app, &running.rx, "the send is confirmed")).ids.pane, Some(id));
}
