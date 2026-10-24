use rstest::rstest;

use super::posts::{Posting, THREAD, first_post, sends};
use super::*;
use crate::control::Close;
use crate::messenger::Tap;

const STRANGER: i64 = 66;
const GUILD: i64 = 7;

struct Thread {
    posting: Posting,
    next: i64,
}

impl Thread {
    fn new() -> (Self, u64) {
        Self::with(|_| {})
    }

    fn with(others: impl FnOnce(&mut Posting)) -> (Self, u64) {
        let mut posting = Posting::new();
        let pane = posting.start(None);
        others(&mut posting);
        let post = first_post(&mut posting, pane);
        posting.posted(&post);
        posting.report(pane, "working");
        posting.sent();
        (Self { posting, next: 7000 }, pane)
    }

    fn message(&mut self, text: &str, reply_to: Option<i64>, (chat, user, guild): (i64, i64, Option<i64>)) -> i64 {
        self.next += 1;
        let message = Message {
            id: self.next,
            chat,
            user,
            private: guild.is_none(),
            guild,
            date: 0,
            name: "@ada".into(),
            text: text.into(),
            reply_to,
        };
        update(&mut self.posting.agent.app, Via::Discord, Update::Message(message));
        self.next
    }

    fn says(&mut self, text: &str) -> i64 {
        self.message(text, None, (THREAD, USER, Some(GUILD)))
    }

    fn reacted(&mut self, what: &str, message: i64) -> Vec<Order> {
        self.posting.waits(what, |order| matches!(order, Order::React { message: m, .. } if *m == message))
    }
}

fn reaction(orders: &[Order], message: i64) -> Option<i64> {
    orders.iter().find_map(|order| match order {
        Order::React { chat, message: m, .. } if *m == message => Some(*chat),
        _ => None,
    })
}

fn typed(thread: &Thread, pane: u64, text: &str) -> bool {
    let shown = thread.posting.agent.app.projects.pane_by(pane).and_then(|t| t.emulator.screen_text().ok());
    shown.is_some_and(|screen| screen.contains(text))
}

fn which(orders: &[Order]) -> Option<(i64, usize)> {
    orders.iter().find_map(|order| match order {
        Order::Send { chat, text, keyboard, .. } if text == "Which agent is this for?" => Some((*chat, keyboard.len())),
        _ => None,
    })
}

mod routing {
    use super::*;

    fn elsewhere(posting: &mut Posting) {
        let api = posting.workspace("api");
        posting.start(Some(api));
    }

    #[test]
    fn a_message_goes_to_the_only_agent_of_the_workspace_while_others_run_elsewhere() {
        let (mut thread, pane) = Thread::with(elsewhere);

        let said = thread.says("hello from the post");
        let got = thread.reacted("the text is confirmed", said);

        assert_eq!(reaction(&got, said), Some(THREAD), "{got:?}");
        assert_eq!(which(&got), None, "{got:?}");
        wait_until("the text reaches the pane", || typed(&thread, pane, "hello from the post"));
    }

    #[test]
    fn with_two_agents_in_the_workspace_the_question_offers_only_those() {
        let (mut thread, _) = Thread::with(|posting| {
            posting.start(None);
            elsewhere(posting);
        });

        thread.says("for one of you");
        let got = thread.posting.sent();

        assert_eq!(which(&got), Some((THREAD, 2)), "{got:?}");
    }

    #[test]
    fn a_reply_to_a_message_of_the_bot_goes_to_its_pane() {
        let (mut thread, pane) = Thread::with(|posting| {
            posting.start(None);
        });

        let said = thread.message("about that", Some(5000), (THREAD, USER, Some(GUILD)));
        let got = thread.reacted("the reply is confirmed", said);

        assert_eq!((reaction(&got, said), which(&got)), (Some(THREAD), None), "{got:?}");
        wait_until("the reply reaches the pane", || typed(&thread, pane, "about that"));
    }

    #[test]
    fn after_write_in_the_post_a_message_answers_it() {
        let (mut thread, pane) = Thread::with(|posting| {
            posting.start(None);
        });
        let data = Tap::Write(pane).data(wiring::run(&thread.posting.agent.app, Via::Discord));
        let press = Press { id: "q1".into(), chat: THREAD, user: USER, message: 5000, data };
        update(&mut thread.posting.agent.app, Via::Discord, Update::Press(press));
        thread.posting.sent();

        let said = thread.says("the answer");
        let got = thread.reacted("the answer is confirmed", said);

        assert_eq!(which(&got), None, "{got:?}");
        wait_until("the answer reaches the pane", || typed(&thread, pane, "the answer"));
    }

    #[test]
    fn a_command_in_the_post_points_to_the_direct_message() {
        let (mut thread, _) = Thread::new();

        thread.says("/status");
        let got = thread.posting.sent();

        assert_eq!(sends(&got), [(THREAD, "Commands work in the direct message with the bot.")], "{got:?}");
    }

    #[test]
    fn without_an_agent_in_the_workspace_it_says_so_in_the_post() {
        let (mut thread, pane) = Thread::new();
        let close = Close { item: Item::Pane(pane), remove_worktree: false, force: true, unlock: false };
        ask(&mut thread.posting.agent.app, None, Command::Close(close));
        thread.posting.agent.answered("the agent is closed");
        thread.posting.sent();

        thread.says("anyone?");
        let got = thread.posting.sent();

        let said = "No agent is running in this workspace. /new in the direct message starts one.";
        assert_eq!(sends(&got), [(THREAD, said)], "{got:?}");
    }
}

mod ignored {
    use super::*;

    #[rstest]
    #[case::someone_else_in_the_post((THREAD, STRANGER, Some(GUILD)))]
    #[case::the_owner_in_another_channel_of_the_server((777, USER, Some(GUILD)))]
    #[case::the_owner_in_a_post_cornercase_did_not_make((901, USER, Some(GUILD)))]
    #[case::the_owner_in_another_server((902, USER, Some(8)))]
    #[case::the_owner_in_the_posts_channel_said_to_be_of_another_server((THREAD, USER, Some(8)))]
    fn a_message_reaches_no_pane(#[case] from: (i64, i64, Option<i64>)) {
        let (mut thread, pane) = Thread::new();

        thread.message("secret words", None, from);

        assert_eq!(thread.posting.sent(), Vec::new());
        assert!(!typed(&thread, pane, "secret words"));
    }
}
