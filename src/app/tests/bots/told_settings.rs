use super::*;
use crate::bots::Briefed;
use crate::control::bots::EditBot;

const FULL: &str = "Your access is `full`";
const SETTINGS: &str = "## Your settings now";

impl Chat {
    fn edit_access(&mut self, access: &str) {
        let edit = EditBot { bot: "prs".into(), access: Some(access.into()), ..EditBot::default() };
        self.app.bot_edit(&edit).expect("edited");
    }

    fn answered(&mut self, text: &str, out: &str) {
        std::fs::write(self.claude.with_extension("out"), out).expect("canned output");
        self.say(text);
        self.settle();
    }

    fn saved(&self) -> Bot {
        folder::load_all(self.dir.path()).remove(0)
    }
}

#[test]
fn an_access_changed_between_two_messages_is_told_once_and_then_kept() {
    let mut c = chat(ANSWER);
    c.answered("first", ANSWER);
    c.edit_access("full");
    c.answered("again", ANSWER);
    c.answered("third", ANSWER);
    let argvs = c.argvs();
    assert!(argvs[1].contains(SETTINGS) && argvs[1].contains(FULL), "{}", argvs[1]);
    assert!(argvs[1].trim_end().ends_with("## The user says\\n\\nagain\"]"), "{}", argvs[1]);
    assert!(argvs[2].trim_end().ends_with("\"--\", \"third\"]"), "{}", argvs[2]);
    assert_eq!(c.saved().briefed.map(|told| told.access), Some(Access::Full));
}

#[test]
fn a_turn_that_fails_leaves_the_change_to_be_told_again() {
    let mut c = chat(ANSWER);
    c.answered("first", ANSWER);
    c.edit_access("full");
    c.answered("again", FAILED);
    c.answered("third", ANSWER);
    let argvs = c.argvs();
    assert!(argvs[1].contains(FULL) && argvs[2].contains(FULL), "{argvs:?}");
}

#[test]
fn a_change_made_while_the_bot_answers_is_told_at_the_next_message() {
    let mut c = chat(ANSWER);
    c.hold();
    c.say("first");
    c.started(1);
    c.edit_access("full");
    c.release(ANSWER);
    c.settle();
    c.answered("again", ANSWER);
    let argvs = c.argvs();
    assert!(!argvs[0].contains(FULL), "{}", argvs[0]);
    assert!(argvs[1].contains("--resume") && argvs[1].contains(FULL), "{}", argvs[1]);
}

#[test]
fn a_conversation_kept_before_its_settings_were_is_told_all_of_them_once() {
    let mut c = chat(ANSWER);
    let kept = Bot { access: Access::Full, conversation: Some("s0".into()), ..c.saved() };
    folder::save(&c.app.panels.bots.live[0].folder, &kept).expect("save");
    c.app.set_bots_dir(c.dir.path());
    c.answered("hi", ANSWER);
    let argv = &c.argvs()[0];
    assert!(argv.contains("\"--resume\", \"s0\"") && argv.contains("You are PRs") && argv.contains(FULL), "{argv}");
    assert!(!argv.contains("## bot.md"), "{argv}");
    assert_eq!(c.saved().briefed, Some(Briefed::of(&kept)));
}
