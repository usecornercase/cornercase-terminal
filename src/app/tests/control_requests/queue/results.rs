use super::*;
use crate::agents::context::Pointer;
use crate::queue::{Place, Task};

const PROMPT: &str = r#"{"type":"user","origin":{"kind":"human"},"message":{"content":"fix the login"}}"#;
const FIRST: &str = r#"{"type":"assistant","timestamp":"2026-11-12T09:41:00.000Z","message":{"id":"r1","model":"claude-opus-5-5","content":[{"type":"text","text":"Reviewed the login."}]}}"#;
const SECOND: &str = r#"{"type":"assistant","timestamp":"2026-11-12T09:42:00.000Z","message":{"id":"r2","model":"claude-opus-5-5","content":[{"type":"text","text":"Second thoughts."}]}}"#;

struct Run {
    agent: Agent,
    id: u64,
    pane: u64,
}

fn state_of(app: &App, id: u64) -> State {
    app.panels.queued.queue().entry(id).expect("entry").state.clone()
}

fn text_of(app: &App, id: u64) -> String {
    app.queue_shown(id).ok().and_then(|shown| shown.result).map(|result| result.text).unwrap_or_default()
}

impl Run {
    fn started() -> Self {
        let mut agent = Agent::new();
        let app = &mut agent.app;
        let project = app.projects[0].path.clone();
        let task = Task {
            at: crate::time::now() + 3600,
            zone: "UTC".into(),
            label: "fix the login".into(),
            prompt: "fix the login".into(),
            agent: "claude".into(),
            command: crate::agents::command_line(&app.config, "claude"),
            program: crate::agents::command(&app.config, "claude"),
            submit: true,
            place: Place { workspace: Some(app.projects[0].workspaces[0].path.clone()), project, ..Place::default() },
            ..Task::default()
        };
        let id = app.panels.queued.schedule(None, task).expect("scheduled");
        app.set_queue_area(AREA);
        app.run_queued(id, Instant::now()).expect("starts");
        agent.refreshing("the entry starts", |a| matches!(state_of(a, id), State::Started { .. }));
        let State::Started { pane, .. } = state_of(&agent.app, id) else { panic!("started") };
        agent.refreshing("the agent works", |a| {
            a.projects.pane_by(pane).is_some_and(|term| term.agent.status() == Some(activity::Status::Working))
        });
        Self { agent, id, pane }
    }

    fn says(&mut self, lines: &[&str]) {
        self.agent.transcript(self.pane, lines);
    }

    fn until_text(&mut self, text: &str) {
        let id = self.id;
        self.agent.refreshing("the result is kept", |a| text_of(a, id) == text);
    }
}

#[test]
fn a_queued_agent_keeps_its_last_message_when_its_turn_ends() {
    let mut run = Run::started();
    run.says(&[PROMPT, FIRST]);
    run.agent.finish();

    run.until_text("Reviewed the login.");

    let entry = run.agent.app.panels.queued.queue().entry(run.id).expect("entry").clone();
    let result = entry.result.expect("a result");
    assert_eq!(result.agent.as_deref(), Some("claude"));
    assert!(matches!(result.record, Some(Pointer::Claude(_))));
    assert!(entry.started.is_some());
    let saved = serde_json::to_string(&run.agent.app.queue_saved()).expect("saved");
    assert!(saved.contains("Reviewed the login."), "{saved}");
}

#[test]
fn each_later_turn_end_replaces_the_text() {
    let mut run = Run::started();
    run.says(&[PROMPT, FIRST]);
    run.agent.finish();
    run.until_text("Reviewed the login.");

    ask(&mut run.agent.app, None, send_text(run.pane, "more", true, false));
    done(run.agent.answered("the enter is pressed"));
    let pane = run.pane;
    run.agent.refreshing("the agent works again", |a| pane_status(a, pane) == Some(activity::Status::Working));
    run.says(&[SECOND]);
    run.agent.finish();

    run.until_text("Second thoughts.");
}

fn pane_status(app: &App, pane: u64) -> Option<activity::Status> {
    app.projects.pane_by(pane).and_then(|term| term.agent.status())
}

#[test]
fn closing_the_pane_keeps_what_was_said_while_it_still_worked() {
    let mut run = Run::started();
    run.says(&[PROMPT, FIRST]);
    run.agent.refreshing("a pass sees the pane", |a| pane_status(a, run.pane).is_some());

    run.agent.app.remove(run.pane);

    run.until_text("Reviewed the login.");
}

fn kept(run: &mut Run) {
    run.says(&[PROMPT, FIRST]);
    run.agent.finish();
    run.until_text("Reviewed the login.");
}

#[test]
fn queue_show_reads_the_conversation_on_a_job() {
    let mut run = Run::started();
    kept(&mut run);

    let Run { agent, id, .. } = &mut run;
    let mut answers =
        send_waiting(&mut agent.app, &agent.rx, None, "queue-show", &json!({"id": *id, "conversation": true}));
    let shown = reply_of(&answers.remove(0).1).expect("answered");

    let said: Vec<(&str, &str)> = shown["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|message| (message["role"].as_str().unwrap_or_default(), message["text"].as_str().unwrap_or_default()))
        .collect();
    assert_eq!(said, [("prompt", "fix the login"), ("reply", "Reviewed the login.")]);
}

#[test]
fn queue_list_gives_result_at_and_never_the_text() {
    let mut run = Run::started();
    kept(&mut run);

    let listed = ok(&mut run.agent.app, "queue", &json!({"action": "list"}));

    let entry = &listed["entries"][0];
    assert!(entry["result_at"].is_i64() && entry["started"].is_i64(), "{entry}");
    assert!(entry.get("result").is_none(), "{entry}");
}

#[test]
fn an_entry_of_another_server_is_not_watched() {
    let mut agent = Agent::new();
    let pane = agent.start(None);
    let at = crate::time::now() + 3600;
    let task = Task {
        at,
        agent: "claude".into(),
        place: Place { project: "/project".into(), ..Place::default() },
        ..Task::default()
    };
    let id = agent.app.panels.queued.schedule(None, task).expect("scheduled");
    agent.app.panels.queued.set_state(id, State::Started { pane, server: "old".into() });
    ask(&mut agent.app, None, send_text(pane, "go", true, false));
    done(agent.answered("the enter is pressed"));
    agent.refreshing("the agent works", |a| pane_status(a, pane) == Some(activity::Status::Working));
    agent.transcript(pane, &[PROMPT, FIRST]);
    agent.finish();

    agent.refreshing("the turn ends", |a| pane_status(a, pane) != Some(activity::Status::Working));
    drain(&mut agent.app, &agent.rx);

    assert!(!agent.app.panels.queued.results.watches(id));
    assert_eq!(text_of(&agent.app, id), "");
}
