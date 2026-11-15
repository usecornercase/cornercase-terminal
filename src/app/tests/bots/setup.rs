use super::*;
use crate::control::bots::{EditBot, NewBot};
use crate::control::contract::Code;

const ABOUT: &str = "Review my open pull requests every weekday at 9";
const SESSION: &str = "8b610e1d-23f2-4ef4-8666-258ef8b082dc";

fn agent_only(out: &str) -> Chat {
    let (mut app, rx) = empty_app();
    let (dir, bin, claude) = install_agent(&mut app, out);
    Chat { app, rx, dir, claude, bin }
}

impl Chat {
    fn make(&mut self, about: &str) -> usize {
        let new = NewBot { name: "PRs".into(), agent: Some("claude".into()), about: about.into(), ..NewBot::default() };
        self.app.bot_new(&new).expect("a bot")
    }
}

#[test]
fn a_bot_made_with_what_it_is_for_sets_itself_up_at_once() {
    use chat::Kind::{Bot, Notice, Step};
    let mut c = agent_only(ANSWER);
    assert_eq!(c.make(ABOUT), 0);
    assert!(c.app.panels.bots.live[0].turn.is_some());
    assert_eq!(c.app.bot_info(0).state, "thinking");
    c.settle();
    assert_eq!(c.kinds(), [Notice, Step, Step, Bot]);
    assert_eq!(c.lines()[0].text, "setting up");
    let argv = &c.argvs()[0];
    assert!(argv.contains("## You were just made") && argv.contains(ABOUT), "{argv}");
    assert!(!argv.contains("--resume") && !argv.contains("## The user says"), "{argv}");
    assert_unread_with_session(&c);
    let toast = c.app.messages.toast.as_ref().expect("a toast");
    assert_eq!(toast.message.as_str(), "PRs: Two are blocked.");
}

fn assert_unread_with_session(c: &Chat) {
    assert_eq!(folder::load_all(c.dir.path())[0].conversation.as_deref(), Some(SESSION));
    assert_eq!(c.app.panels.bots.live[0].unread, 1);
}

#[rstest]
#[case::empty("")]
#[case::blank("  \n ")]
fn a_bot_made_without_what_it_is_for_starts_nothing(#[case] about: &str) {
    let mut c = agent_only(ANSWER);
    c.make(about);
    assert!(c.app.panels.bots.live[0].turn.is_none());
    assert_eq!(c.lines(), []);
    assert_eq!(c.argvs(), Vec::<String>::new());
}

#[test]
fn loading_bots_whose_bot_md_says_what_they_are_for_starts_no_turn() {
    let c = chat(ANSWER);
    assert!(c.app.panels.bots.live[0].turn.is_none());
    assert_eq!(c.argvs(), Vec::<String>::new());
}

#[test]
fn the_next_message_continues_the_setups_conversation() {
    let mut c = agent_only(ANSWER);
    c.make(ABOUT);
    c.settle();
    c.say("and now?");
    c.settle();
    let argv = &c.argvs()[1];
    assert!(argv.contains(&format!("\"--resume\", \"{SESSION}\"")), "{argv}");
    assert!(argv.trim_end().ends_with("\"--\", \"and now?\"]"), "{argv}");
}

#[test]
fn a_message_sent_during_the_setup_is_held_and_goes_next() {
    let mut c = agent_only(ANSWER);
    c.hold();
    c.make(ABOUT);
    c.started(1);
    assert!(c.say("hi").held);
    c.release(ANSWER);
    c.started(2);
    c.settle();
    let answers = c.kinds().iter().filter(|kind| **kind == chat::Kind::Bot).count();
    assert_eq!(answers, 2);
    let argv = &c.argvs()[1];
    assert!(argv.contains("--resume") && argv.trim_end().ends_with("\"--\", \"hi\"]"), "{argv}");
}

#[test]
fn setup_is_refused_while_a_turn_runs_and_when_bot_md_is_empty() {
    let mut c = agent_only(ANSWER);
    c.make("");
    assert_eq!(c.app.bot_setup("prs").expect_err("empty").code, Code::Conflict);
    let edit = EditBot { bot: "prs".into(), about: Some("review".into()), ..EditBot::default() };
    c.app.bot_edit(&edit).expect("edit");
    assert!(c.app.panels.bots.live[0].turn.is_none(), "an edit runs no setup");
    c.hold();
    let (_, line) = c.app.bot_setup("prs").expect("setup");
    assert_eq!(line.text, "setting up");
    assert_eq!(c.app.bot_setup("prs").expect_err("busy").code, Code::Busy);
    c.release(ANSWER);
    c.settle();
}

#[test]
fn setup_again_opens_a_new_conversation_and_keeps_the_old_one_when_it_fails() {
    let mut c = chat(ANSWER);
    c.app.panels.bots.live[0].bot.conversation = Some("old".into());
    c.app.bot_setup("prs").expect("setup");
    c.settle();
    assert!(!c.argvs()[0].contains("--resume"));
    assert_eq!(c.app.panels.bots.live[0].bot.conversation.as_deref(), Some(SESSION));

    let mut failing = chat(FAILED);
    failing.app.panels.bots.live[0].bot.conversation = Some("old".into());
    failing.app.bot_setup("prs").expect("setup");
    failing.settle();
    assert_eq!(failing.app.panels.bots.live[0].bot.conversation.as_deref(), Some("old"));

    let mut fresh = agent_only(FAILED);
    fresh.make(ABOUT);
    fresh.settle();
    assert_eq!(fresh.app.panels.bots.live[0].bot.conversation, None);
}

#[test]
fn retry_under_a_failed_setup_runs_the_setup_again() {
    use chat::Kind::{Bot, Error, Notice, Step};
    let mut c = agent_only(FAILED);
    c.app.window_attached();
    c.make(ABOUT);
    c.app.show_bot(0);
    c.retried();
    assert_eq!(c.kinds(), [Notice, Error, Notice, Step, Step, Bot]);
    assert!(c.argvs()[1].contains("## You were just made"));
    assert!(c.app.messages.toast.is_none());
}
