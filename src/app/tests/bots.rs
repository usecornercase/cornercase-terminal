use std::time::Duration;

use super::*;
use crate::bots::chat::{self, Line};
use crate::bots::folder;
use crate::bots::sample as bot;
use crate::test_util::{fake_program, fixture, wait_until};

fn chat_of(dir: &Path, id: &str) -> Vec<Line> {
    chat::read(&folder::of(dir, id).join(folder::CHAT_FILE))
}

fn texts(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(|line| line.text.as_str()).collect()
}

#[test]
fn a_turn_that_never_ended_is_interrupted_and_what_was_held_is_dropped_once() {
    let dir = TempDir::new();
    folder::create(dir.path(), &bot("prs", "PRs"), "").expect("create");
    let path = folder::of(dir.path(), "prs").join(folder::CHAT_FILE);
    for line in [Line::you(1, 10, "first", false), Line::you(2, 11, "second", true)] {
        chat::append(&path, &line).expect("append");
    }
    let (mut app, _rx) = empty_app();

    app.set_bots_dir(dir.path());
    app.set_bots_dir(dir.path());

    let lines = chat_of(dir.path(), "prs");
    assert_eq!(texts(&lines)[2..], ["interrupted by a restart", "1 held message dropped"]);
    assert_eq!(app.panels.bots.live.len(), 1);
    assert_eq!(app.panels.bots.live[0].next_line, 5);
    assert_eq!(app.panels.bots.live[0].last.as_ref().map(|line| line.text.as_str()), Some("second"));
}

#[test]
fn a_finished_chat_gets_no_notice() {
    let dir = TempDir::new();
    folder::create(dir.path(), &bot("prs", "PRs"), "").expect("create");
    let path = folder::of(dir.path(), "prs").join(folder::CHAT_FILE);
    chat::append(&path, &Line::you(1, 10, "first", false)).expect("append");
    chat::append(&path, &Line::failed(2, 11, "no")).expect("append");
    let (mut app, _rx) = empty_app();

    app.set_bots_dir(dir.path());

    assert_eq!(chat_of(dir.path(), "prs").len(), 2);
}

const ANSWER: &str = fixture!("claude/2.1.325/bot-new.jsonl");
const FAILED: &str = fixture!("claude/2.1.318/recap-failed.jsonl");

struct Chat {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
    claude: PathBuf,
    _bin: TempDir,
}

fn chat(out: &str) -> Chat {
    let (mut app, rx) = empty_app();
    let (dir, bin) = (TempDir::new(), TempDir::new());
    let claude = fake_program(bin.path(), "claude");
    std::fs::write(claude.with_extension("out"), out).expect("canned output");
    app.config.agent_commands.insert("claude".into(), claude.display().to_string());
    folder::create(dir.path(), &bot("prs", "PRs"), "review my PRs").expect("create");
    app.set_bots_dir(dir.path());
    app.panels.bots.timeout = Duration::from_secs(20);
    Chat { app, rx, dir, claude, _bin: bin }
}

impl Chat {
    fn settle(&mut self) {
        let Self { app, rx, .. } = self;
        wait_until("the turn ends", || {
            drain(app, rx);
            app.panels.bots.live[0].turn.is_none()
        });
    }

    fn lines(&self) -> Vec<Line> {
        chat_of(self.dir.path(), "prs")
    }

    fn kinds(&self) -> Vec<chat::Kind> {
        self.lines().iter().map(|line| line.kind).collect()
    }

    fn argvs(&self) -> Vec<String> {
        let text = std::fs::read_to_string(self.claude.with_extension("argv")).unwrap_or_default();
        text.lines().map(str::to_string).collect()
    }

    fn say(&mut self, text: &str) -> Line {
        self.app.bot_say("prs", text, false).expect("say").1
    }

    fn hold(&mut self) {
        std::fs::write(self.claude.with_extension("hold"), "").expect("hold");
        std::fs::write(self.claude.with_extension("out"), "").expect("nothing yet");
    }

    fn started(&mut self, runs: usize) {
        let Self { app, rx, claude, .. } = self;
        wait_until("the agent started", || {
            drain(app, rx);
            std::fs::read_to_string(claude.with_extension("argv")).is_ok_and(|text| text.lines().count() >= runs)
        });
    }

    fn release(&mut self, answer: &str) {
        std::fs::write(self.claude.with_extension("after"), answer).expect("the answer");
        std::fs::remove_file(self.claude.with_extension("hold")).expect("release");
    }
}

#[test]
fn a_message_starts_a_turn_whose_steps_come_before_its_answer() {
    use chat::Kind::{Bot, Step, You};
    let mut c = chat(ANSWER);
    let line = c.say("which PRs are blocked?");
    c.settle();
    assert_eq!(c.kinds(), [You, Step, Step, Bot]);
    assert_eq!(line.kind, You);
    let lines = c.lines();
    assert_eq!(lines[1].tool.as_deref(), Some("Bash"));
    assert_eq!(lines[3].text, "Two are blocked.");
    assert_eq!(lines[3].conversation.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
    assert_eq!(lines[3].model.as_deref(), Some("claude-opus-5-5"));
    let saved = &folder::load_all(c.dir.path())[0];
    assert_eq!(saved.conversation.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
}

#[test]
fn the_first_turn_carries_the_brief_and_the_next_one_resumes_with_the_text_alone() {
    let mut c = chat(ANSWER);
    c.say("first");
    c.settle();
    c.say("again");
    c.settle();
    let argvs = c.argvs();
    assert!(argvs[0].contains("You are PRs") && argvs[0].contains("review my PRs"), "{}", argvs[0]);
    assert!(!argvs[0].contains("--resume"));
    assert!(argvs[1].contains("\"--resume\", \"8b610e1d-23f2-4ef4-8666-258ef8b082dc\""), "{}", argvs[1]);
    assert!(argvs[1].trim_end().ends_with("\"--\", \"again\"]"), "{}", argvs[1]);
}

#[test]
fn messages_sent_while_it_answers_are_held_and_go_together_as_the_next_turn() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("first");
    c.started(1);
    let second = c.say("second");
    let third = c.say("third");
    assert!(second.held && third.held);
    assert_eq!(c.app.panels.bots.live[0].held.len(), 2);
    c.release(ANSWER);
    c.started(2);
    c.settle();
    let argvs = c.argvs();
    assert!(argvs[1].trim_end().ends_with("\"--\", \"second\\n\\nthird\"]"), "{}", argvs[1]);
    assert!(argvs[1].contains("--resume"), "{}", argvs[1]);
    let answers = c.kinds().iter().filter(|kind| **kind == chat::Kind::Bot).count();
    assert_eq!(answers, 2);
}

#[test]
fn stopping_ends_the_turn_keeps_the_session_and_drops_what_was_held() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.live[0].bot.conversation = Some(String::from("s0"));
    c.hold();
    c.say("long job");
    c.started(1);
    c.say("and this");
    assert_eq!(c.app.bot_stop("prs"), Ok(true));
    c.settle();
    let texts: Vec<_> = c.lines().into_iter().map(|line| line.text).collect();
    assert_eq!(texts[2..], ["stopped", "1 held message dropped"]);
    assert_eq!(c.app.panels.bots.live[0].bot.conversation.as_deref(), Some("s0"));
    assert_eq!(c.app.panels.bots.live[0].held, Vec::new());
    assert_eq!(c.app.bot_stop("prs"), Ok(false));
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}

#[test]
fn a_turn_that_takes_too_long_ends_with_an_error() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.timeout = Duration::from_millis(300);
    c.hold();
    c.say("slow");
    c.settle();
    let last = c.lines().pop().expect("a line");
    assert_eq!((last.kind, last.text.as_str()), (chat::Kind::Error, "claude did not answer in time"));
    assert_eq!(c.app.panels.bots.live[0].bot.conversation, None);
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
    c.say("again");
    c.settle();
    assert_eq!(c.app.panels.bots.live[0].unread, 2);
}

#[test]
fn an_answer_nobody_waits_for_is_unread_and_makes_a_toast_and_a_notification() {
    let mut c = chat(ANSWER);
    c.say("hi");
    c.settle();
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    let toast = c.app.messages.toast.as_ref().expect("a toast");
    assert_eq!((toast.message.as_str(), toast.go.as_deref()), ("PRs: Two are blocked.", None));
    let notes = c.app.outbox.take_notifications();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].text, "PRs: Two are blocked.");
    c.app.bot_seen("prs").expect("seen");
    assert_eq!(c.app.panels.bots.live[0].unread, 0);
}

#[test]
fn an_error_counts_as_unread_too_and_a_waited_answer_does_not() {
    let mut c = chat(FAILED);
    c.say("hi");
    c.settle();
    let last = c.lines().pop().expect("a line");
    assert_eq!((last.kind, last.text.as_str()), (chat::Kind::Error, "Not logged in · Please run /login"));
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    std::fs::write(c.claude.with_extension("out"), ANSWER).expect("answer");
    let (at, asked) = c.app.bot_say("prs", "now?", true).expect("say");
    c.settle();
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
    let answer = c.app.bot_answer(at, asked.id).expect("the answer");
    assert_eq!(answer.kind, chat::Kind::Bot);
}

#[test]
fn a_new_conversation_forgets_the_session_and_starts_with_the_brief_again() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("first");
    c.started(1);
    let refused = c.app.bot_reset("prs").expect_err("it is answering");
    assert!(refused.message.contains("stop it first"), "{}", refused.message);
    c.release(ANSWER);
    c.settle();
    c.app.bot_reset("prs").expect("reset");
    assert_eq!(c.app.panels.bots.live[0].bot.conversation, None);
    assert_eq!(c.lines().pop().expect("a line").text, "new conversation");
    c.say("again");
    c.settle();
    let argvs = c.argvs();
    assert!(!argvs[1].contains("--resume") && argvs[1].contains("You are PRs"), "{}", argvs[1]);
}

#[test]
fn a_bot_is_removed_only_when_it_is_idle() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("first");
    c.started(1);
    assert!(c.app.bot_remove("prs").is_err());
    c.release(ANSWER);
    c.settle();
    let folder = c.dir.path().join("prs");
    assert_eq!(c.app.bot_remove("PRs").as_deref(), Ok("prs"));
    assert!(!folder.exists());
    assert!(c.app.bot_at("prs").is_err());
}

#[test]
fn a_message_must_be_there_and_not_too_long() {
    let mut c = chat(ANSWER);
    assert!(c.app.bot_say("prs", "  ", false).is_err());
    let long = "x".repeat(crate::bots::MESSAGE_MAX + 1);
    let refused = c.app.bot_say("prs", &long, false).expect_err("too long");
    assert_eq!(refused.message, "the message is 20001 characters; a bot takes at most 20000");
    assert_eq!(
        c.app.bot_say("nobody", "hi", false).expect_err("no bot").code,
        crate::control::contract::Code::NotFound
    );
    assert_eq!(c.lines(), Vec::new());
}

#[test]
fn stopping_twice_still_answers_the_client_waiting_on_a_held_message() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("long job");
    c.started(1);
    let (at, held) = c.app.bot_say("prs", "and this", true).expect("say");
    assert_eq!(c.app.bot_stop("prs"), Ok(true));
    assert_eq!(c.app.bot_stop("prs"), Ok(true));
    c.settle();
    let notices = c.lines().into_iter().filter(|line| line.text == "1 held message dropped").count();
    assert_eq!(notices, 1);
    let answer = c.app.bot_answer(at, held.id).expect("the waiter is answered");
    assert_eq!(answer.text, "1 held message dropped");
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}

#[test]
fn the_server_stopping_ends_the_turns_that_run() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.timeout = Duration::from_secs(600);
    c.hold();
    c.say("long job");
    c.started(1);
    c.app.stop_background();
    c.settle();
    assert!(c.app.panels.bots.live[0].turn.is_none());
    std::fs::remove_file(c.claude.with_extension("hold")).expect("release");
}
