use std::time::{Duration, SystemTime};

use super::*;
use crate::agents::Known;
use crate::agents::copilot::fake::FakeCopilot;

struct Running {
    app: App,
    rx: Receiver<AppEvent>,
    fake: FakeCopilot,
    _dirs: Vec<TempDir>,
}

impl Running {
    fn new() -> Self {
        let (mut app, rx, dirs) = app_with(1);
        let fake = FakeCopilot::new(&app.projects[0].path);
        type_line(&mut app, &fake.command_line());
        refreshing(&mut app, &rx, "Copilot holds its session", |a| {
            a.term().is_some_and(|t| t.context.record_for(Known::Copilot).is_some())
        });
        fake.started("gpt-4.1");
        Self { app, rx, fake, _dirs: dirs }
    }

    fn send(&mut self, text: &str) {
        send_enter(&mut self.app, text);
    }

    fn working(&mut self) {
        self.fake.prompt("first");
        self.fake.turn(true);
        self.fake.working(true);
        refreshing(&mut self.app, &self.rx, "Copilot works", |a| {
            a.term().and_then(|t| t.agent.status()) == Some(activity::Status::Working)
        });
    }
}

#[test]
fn a_later_submission_confirms_a_pending_send() {
    let mut running = Running::new();
    let now = SystemTime::now();
    running.fake.prompt_at("first", now - Duration::from_secs(60));
    refreshing(&mut running.app, &running.rx, "the first prompt is recorded", |a| {
        a.term().is_some_and(|term| term.context.prompted().is_some())
    });
    let id = first(&running.app);

    running.send("next");
    running.app.refresh(Instant::now());
    assert_eq!(answers(&mut running.app).len(), 0);
    running.fake.prompt_at("next", now + Duration::from_secs(60));

    assert_eq!(done(answered(&mut running.app, &running.rx, "the later prompt confirms")).ids.pane, Some(id));
}

#[test]
fn a_queued_prompt_confirms_once_copilot_takes_it() {
    let mut running = Running::new();
    running.app.confirm_within = Duration::from_millis(300);
    running.working();
    let id = first(&running.app);

    running.send("then this");
    let begun = Instant::now();
    wait_until("the confirmation window has passed", || {
        drain(&mut running.app, &running.rx);
        running.app.refresh(Instant::now());
        begun.elapsed() > Duration::from_millis(900)
    });
    assert_eq!(answers(&mut running.app).len(), 0);
    running.fake.queued("then this");

    assert_eq!(done(answered(&mut running.app, &running.rx, "the queued prompt confirms")).ids.pane, Some(id));
}

#[test]
fn an_unconfirmed_send_fails_instead_of_hanging() {
    let mut running = Running::new();

    an_unconfirmed_send_gives_up(&mut running.app, &running.rx);
}

#[test]
fn the_last_message_is_the_main_reply_and_not_a_sub_agents() {
    let mut running = Running::new();
    let id = first(&running.app);
    running.fake.prompt("fix the login");
    running.fake.reply("The login is fixed.", "gpt-4.1");
    running.fake.subagent_reply("a helper's finding");
    running.fake.turn(false);
    refreshing(&mut running.app, &running.rx, "the pane knows its conversation and the turn is over", |a| {
        a.term().is_some_and(|t| t.context.conversation().is_some() && t.agent.status() == Some(activity::Status::Idle))
    });

    let message = last_message(&mut running.app, &running.rx, id);

    assert_eq!(
        (message["agent"].as_str(), message["text"].as_str(), &message["turn_over"]),
        (Some("copilot"), Some("The login is fixed."), &Value::Bool(true))
    );
}

#[rstest::rstest]
#[case::slash("/clear")]
#[case::shell("!ls")]
#[case::help("?")]
fn local_commands_return_after_enter_without_a_prompt_record(#[case] text: &str) {
    let mut running = Running::new();

    a_local_command_needs_no_record(&mut running.app, &running.rx, text);
}

#[test]
fn send_refuses_a_copilot_that_asks_for_permission() {
    let mut running = Running::new();
    running.working();
    running.fake.permission(true);
    refreshing(&mut running.app, &running.rx, "Copilot waits", |a| {
        a.term().and_then(|t| t.agent.status()) == Some(activity::Status::Waiting)
    });

    a_waiting_agent_refuses_a_send(&mut running.app);
}

#[test]
fn two_copilot_in_one_folder_each_follow_their_own_conversation() {
    let (mut app, rx, _dirs) = app_with(1);
    let folder = app.projects[0].path.clone();
    let (left, right) = (FakeCopilot::new(&folder), FakeCopilot::new(&folder));
    type_line(&mut app, &left.command_line());
    let (left_pane, right_pane) = (first(&app), {
        app.add_tab(0, 0, AREA).expect("a second tab");
        type_line(&mut app, &right.command_line());
        app.projects[0].workspaces[0].tabs[1].pane().map(|t| t.id).expect("the second pane")
    });
    refreshing(&mut app, &rx, "each Copilot holds a session", |a| {
        [left_pane, right_pane].iter().all(|id| pane(a, *id).context.record_for(Known::Copilot).is_some())
    });
    left.started("gpt-4.1");
    left.prompt("one");

    refreshing(&mut app, &rx, "only the first has a conversation", |a| {
        pane(a, left_pane).context.conversation().is_some()
    });
    assert!(pane(&app, right_pane).context.conversation().is_none());
    right.started("gpt-4.1");
    right.prompt("other");
    refreshing(&mut app, &rx, "the second has one too", |a| pane(a, right_pane).context.conversation().is_some());

    let records = [left_pane, right_pane].map(|id| pane(&app, id).context.record_for(Known::Copilot));
    assert_ne!(records[0], records[1]);
}
