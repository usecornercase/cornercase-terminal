#![cfg(unix)]

use super::following_the_app::follow as follow_app;
use super::*;
use crate::bots::chat::Kind;
use crate::test_util::{TempDir, fake_program, fixture};

const ANSWER: &str = fixture!("claude/2.1.325/bot-new.jsonl");

struct Bots {
    app: App,
    rx: Receiver<AppEvent>,
    claude: PathBuf,
    _dirs: (TempDir, TempDir),
}

fn bots() -> Bots {
    let (mut app, rx) = empty_app();
    let (state, bin) = (TempDir::new(), TempDir::new());
    let claude = fake_program(bin.path(), "claude");
    std::fs::write(claude.with_extension("out"), ANSWER).expect("canned output");
    app.config.agent_commands.insert("claude".into(), claude.display().to_string());
    app.config.agent_commands.insert("codex".into(), "/nonexistent/codex".into());
    app.set_bots_dir(&state.path().join("bots"));
    Bots { app, rx, claude, _dirs: (state, bin) }
}

fn wire(args: &Value) -> Value {
    let mut entry = args.as_object().cloned().unwrap_or_default();
    let action = entry.remove("action").unwrap_or(Value::Null);
    if entry.is_empty() { json!({"action": action}) } else { json!({"action": action, "entry": entry}) }
}

impl Bots {
    fn ask(&mut self, command: &str, args: &Value) -> Value {
        asked(&mut self.app, &self.rx, command, &wire(args))
            .unwrap_or_else(|e| panic!("{command}: {} {}", e.code, e.message))
    }

    fn refuse(&mut self, args: &Value) -> Refused {
        match asked(&mut self.app, &self.rx, "bot", &wire(args)) {
            Ok(value) => panic!("{args} was taken: {value}"),
            Err(refused) => refused,
        }
    }

    fn new_prs(&mut self) -> Value {
        self.ask("bot", &json!({"action": "new", "name": "PRs", "agent": "claude", "about": "review my PRs"}))
    }

    fn argvs(&self) -> Vec<String> {
        let text = std::fs::read_to_string(self.claude.with_extension("argv")).unwrap_or_default();
        text.lines().map(str::to_string).collect()
    }
}

fn kinds_of(history: &Value) -> Vec<&str> {
    history["lines"].as_array().expect("lines").iter().filter_map(|line| line["kind"].as_str()).collect()
}

#[test]
fn a_bot_is_made_asked_and_read_back() {
    let mut b = bots();
    let made = b.new_prs();
    assert_eq!(made["id"], "prs");
    assert_eq!((made["state"].as_str(), made["access"].as_str()), (Some("idle"), Some("cornercase")));
    assert!(b.argvs().is_empty(), "making a bot starts nothing");

    let said = b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "which PRs are blocked?", "wait": true}));
    assert_eq!((said["line"]["kind"].as_str(), said["line"]["text"].as_str()), (Some("bot"), Some("Two are blocked.")));
    assert!(said["asked"].as_u64().is_some());

    let history = b.ask("bot", &json!({"action": "history", "bot": "prs"}));
    assert_eq!(kinds_of(&history), ["you", "step", "step", "bot"]);
    let last = b.ask("bot", &json!({"action": "history", "bot": "prs", "last": 1}));
    assert_eq!(kinds_of(&last), ["bot"]);
    let after = b.ask("bot", &json!({"action": "history", "bot": "prs", "after": 3}));
    assert_eq!(kinds_of(&after), ["bot"]);
}

#[test]
fn a_reset_makes_the_next_message_start_a_new_session() {
    let mut b = bots();
    b.new_prs();
    b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "hi", "wait": true}));
    b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "and now?", "wait": true}));
    assert!(b.argvs()[1].contains("--resume"));
    b.ask("bot", &json!({"action": "reset", "bot": "prs"}));
    b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "again", "wait": true}));
    let argvs = b.argvs();
    assert!(!argvs[2].contains("--resume") && argvs[2].contains("You are PRs"), "{}", argvs[2]);
}

#[test]
fn the_events_of_a_turn_come_in_order_and_the_steps_before_the_answer() {
    let mut b = bots();
    follow(&mut b.app, &[]);
    b.new_prs();
    b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "hi", "wait": true}));
    let seen: Vec<String> = told(&streamed(&mut b.app))
        .into_iter()
        .filter_map(|(what, _)| match what {
            wire::What::Bot { bot, line, state, change } => {
                assert_eq!(bot, "prs");
                Some(line.map(|line| format!("line {:?}", line.kind)).or(state).or(change).expect("one of them"))
            }
            _ => None,
        })
        .collect();
    assert_eq!(seen, ["created", "thinking", "line You", "line Step", "line Step", "line Bot", "idle"]);
}

#[test]
fn the_app_feed_carries_the_bots() {
    let mut b = bots();
    let first = follow_app(&mut b.app);
    assert_eq!(first.report.bots, Vec::new());
    b.new_prs();
    b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "hi", "wait": true}));
    let state = b.app.app_state();
    assert_eq!(state.bots.len(), 1);
    assert_eq!((state.bots[0].id.as_str(), state.bots[0].state.as_str()), ("prs", "idle"));
    assert_eq!(state.bots[0].last.as_ref().map(|last| last.text.as_str()), Some("Two are blocked."));
}

#[test]
fn the_agents_say_which_can_be_a_bot_and_why_not() {
    let mut b = bots();
    let answer = b.ask("bot", &json!({"action": "agents"}));
    let find = |name: &str| {
        answer["agents"].as_array().expect("agents").iter().find(|a| a["agent"] == name).expect(name).clone()
    };
    let claude = find("claude");
    assert_eq!((&claude["bot"], &claude["accesses"]), (&json!(true), &json!(["cornercase", "full"])));
    assert_eq!(claude["why"], Value::Null);
    assert_eq!(find("gemini")["why"], "not yet");
    let codex = find("codex");
    assert_eq!((&codex["bot"], &codex["why"]), (&json!(false), &json!("not installed")));
}

#[test]
fn what_cannot_be_asked_is_refused() {
    let mut b = bots();
    let new = |name: &str| json!({"action": "new", "name": name, "agent": "claude"});
    assert_eq!(b.refuse(&new(&"x".repeat(41))).code, "malformed");
    b.new_prs();
    assert_eq!(b.refuse(&new("prs")).code, "conflict");
    assert_eq!(b.refuse(&json!({"action": "new", "name": "x"})).code, "malformed");
    assert_eq!(b.refuse(&json!({"action": "new", "name": "x", "agent": "codex"})).code, "malformed");
    let long = "x".repeat(20_001);
    let said = b.refuse(&json!({"action": "say", "bot": "prs", "text": long}));
    assert_eq!(
        (said.code.as_str(), said.message.as_str()),
        ("malformed", "the message is 20001 characters; a bot takes at most 20000")
    );
    let edit = b.refuse(&json!({"action": "edit", "bot": "prs", "agent": "codex"}));
    assert!(edit.message.contains("keeps its agent"), "{}", edit.message);
    assert_eq!(b.refuse(&json!({"action": "say", "bot": "nobody", "text": "hi"})).code, "not-found");
    assert_eq!(
        b.refuse(&json!({"action": "new", "name": "y", "agent": "claude", "model": "m", "effort": "bogus"})).code,
        "malformed"
    );
}

#[test]
fn there_are_at_most_fifty_bots() {
    let mut b = bots();
    for n in 0..50 {
        b.ask("bot", &json!({"action": "new", "name": format!("bot {n}"), "agent": "claude"}));
    }
    let refused = b.refuse(&json!({"action": "new", "name": "one more", "agent": "claude"}));
    assert_eq!(refused.code, "conflict");
    assert!(refused.message.contains("50 bots already"), "{}", refused.message);
}

#[test]
fn a_bot_is_edited_by_name_and_removed_by_id() {
    let mut b = bots();
    b.new_prs();
    let edited =
        b.ask("bot", &json!({"action": "edit", "bot": "PRs", "name": "Reviews", "model": "opus", "effort": "low"}));
    assert_eq!(
        (edited["id"].as_str(), edited["name"].as_str(), edited["model"].as_str()),
        (Some("prs"), Some("Reviews"), Some("opus"))
    );
    let cleared = b.ask("bot", &json!({"action": "edit", "bot": "prs", "model": ""}));
    assert_eq!(cleared["model"], Value::Null);
    assert_eq!(b.ask("bot", &json!({"action": "remove", "bot": "prs"}))["bot"], "prs");
    assert_eq!(b.ask("bot", &json!({"action": "list"}))["bots"], json!([]));
}

#[test]
fn a_turn_that_fails_answers_with_an_error_line_a_waiting_client_can_tell() {
    let mut b = bots();
    std::fs::write(b.claude.with_extension("out"), fixture!("claude/2.1.318/recap-failed.jsonl"))
        .expect("failing agent");
    b.new_prs();
    let said = b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "hi", "wait": true}));
    assert_eq!(said["line"]["kind"], "error");
    assert_eq!(said["line"]["text"], "Not logged in · Please run /login");
    let list = b.ask("bot", &json!({"action": "list"}));
    assert_eq!(list["bots"][0]["unread"], 0);
}

#[test]
fn a_message_that_is_not_waited_for_answers_at_once_and_makes_the_answer_unread() {
    let mut b = bots();
    b.new_prs();
    let said = b.ask("bot", &json!({"action": "say", "bot": "prs", "text": "hi"}));
    assert_eq!(said["line"]["kind"], "you");
    let Bots { app, rx, .. } = &mut b;
    refreshing(app, rx, "the answer", |app| app.bot_infos()[0].unread == 1);
    b.ask("bot", &json!({"action": "seen", "bot": "prs"}));
    assert_eq!(b.ask("bot", &json!({"action": "list"}))["bots"][0]["unread"], 0);
    let kinds: Vec<_> = b.app.bot_infos().iter().map(|info| info.state.clone()).collect();
    assert_eq!(kinds, ["idle"]);
    let _ = Kind::Bot;
}
