use super::*;
use crate::agents::gemini::fake::FakeGemini;

struct Running {
    app: App,
    rx: Receiver<AppEvent>,
    fake: FakeGemini,
    _dirs: Vec<TempDir>,
}

impl Running {
    fn new() -> Self {
        let (mut app, rx, dirs) = app_with(1);
        let fake = FakeGemini::new(&app.projects[0].path);
        type_line(&mut app, &fake.command_line());
        refreshing(&mut app, &rx, "Gemini is idle", |a| {
            a.term().and_then(|t| t.agent.status()) == Some(activity::Status::Idle)
        });
        Self { app, rx, fake, _dirs: dirs }
    }

    fn send(&mut self, text: &str) {
        send_enter(&mut self.app, text);
    }

    fn title(&mut self, title: &str) {
        self.fake.signal(title);
        pump_until(&mut self.app, &self.rx, "Gemini sets its title", |a| {
            a.term().is_some_and(|t| t.emulator.title() == title)
        });
    }
}

#[test]
fn a_send_is_confirmed_and_wait_and_last_message_follow_the_same_conversation() {
    let mut running = Running::new();
    let id = first(&running.app);
    running.send("fix the login");
    assert_eq!(done(answered(&mut running.app, &running.rx, "Gemini records the prompt")).ids.pane, Some(id));
    ask(&mut running.app, None, wait_for(id, Until::TurnOver, Some(5.0)));
    running.app.refresh(Instant::now());
    assert_eq!(answers(&mut running.app).len(), 0);
    running.fake.append(crate::test_util::fixture!("gemini/0.63.0/reply.jsonl"));
    running.title("◇  Ready (project)");
    let ended = done(answered(&mut running.app, &running.rx, "the Gemini turn ends"));
    assert_eq!(ended.ended.as_deref(), Some("idle"));
    let message = last_message(&mut running.app, &running.rx, id);
    assert_eq!(message["agent"], "gemini");
    assert_eq!(message["text"], "OK");
    assert_eq!(message["turn_over"], true);
    assert!(message["written"].as_str().is_some());
}

#[rstest::rstest]
#[case::slash("/clear")]
#[case::help("?help")]
#[case::shell("!printf OK")]
fn local_commands_return_after_enter_without_a_prompt_record(#[case] text: &str) {
    let mut running = Running::new();
    a_local_command_needs_no_record(&mut running.app, &running.rx, text);
}

#[test]
fn send_refuses_a_gemini_tool_confirmation() {
    let mut running = Running::new();
    running.title("✋  Action Required (project)");
    refreshing(&mut running.app, &running.rx, "Gemini waits", |a| {
        a.term().and_then(|t| t.agent.status()) == Some(activity::Status::Waiting)
    });
    running.send("continue");
    assert!(error(answers(&mut running.app).remove(0)).contains("waiting for an answer"));
}

#[test]
fn status_events_and_a_pending_restart_follow_geminis_turn() {
    let mut running = Running::new();
    let id = first(&running.app);
    follow(&mut running.app, &[id]);
    running.send("fix the login");
    done(answered(&mut running.app, &running.rx, "Gemini records the prompt"));
    let events = until_told(&mut running.app, &running.rx, "Gemini works", |events| {
        statuses(events, id) == [change(Some("idle"), Some("working"))]
    });
    assert!(
        told(&events).iter().any(|(what, _)| { matches!(what, wire::What::Status { agent, .. } if agent == "gemini") })
    );
    let status = value(&mut running.app, None, Command::Status(wire::Status {}));
    assert_eq!(status["projects"][0]["workspaces"][0]["tabs"][0]["panes"][0]["status"], "working");
    ask(&mut running.app, None, Command::RestartWhenIdle(wire::RestartWhenIdle { timeout: Some(5.0) }));
    running.app.refresh(Instant::now());
    assert_eq!(answers(&mut running.app), Vec::<Response>::new());
    assert!(running.app.take_restart().is_none());
    running.title("◇  Ready (project)");
    assert!(matches!(answered(&mut running.app, &running.rx, "Gemini releases the restart"), Response::Ok(_)));
    assert!(running.app.take_restart().is_some());
}
