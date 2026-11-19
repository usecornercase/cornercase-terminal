use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::agents::headless::Access;
use crate::app::tests::bots::routines::{START, bot_routine, runs_settle};
use crate::app::tests::bots::{ANSWER, argvs, chat_of, install_bot, settle};
use crate::bots::ChatApps;
use crate::bots::chat::Kind;
use crate::queue::Task;

const BLOCKED: &str = "Two are blocked.";

struct Bots {
    app: App,
    rx: Receiver<AppEvent>,
    phone: Phone,
    dir: TempDir,
    claude: PathBuf,
    _bin: TempDir,
}

impl Bots {
    fn new(via: Via, chat_apps: ChatApps) -> Self {
        let (mut app, rx) = empty_app();
        let (dir, bin, claude) = install_bot(&mut app, ANSWER, Access::Cornercase);
        app.panels.bots.live[0].bot.chat_apps = chat_apps;
        let phone = Phone::on(&mut app, via, Some(chat()));
        Self { app, rx, phone, dir, claude, _bin: bin }
    }

    fn away(&mut self) {
        self.app.away_request(false).expect("away");
        self.phone.read(&mut self.app);
    }

    fn sent(&mut self) -> Vec<(i64, Order)> {
        drain(&mut self.app, &self.rx);
        self.phone.read(&mut self.app)
    }

    fn schedule(&mut self, task: Task) -> u64 {
        self.app.set_queue_area(AREA);
        self.app.panels.queued.schedule(None, task).expect("scheduled")
    }

    fn run_routine(&mut self) {
        self.schedule(bot_routine(START, "2h"));
        self.app.refresh_at(Instant::now(), START);
        runs_settle(&mut self.app, &self.rx);
    }

    fn settle(&mut self) {
        settle(&mut self.app, &self.rx);
    }

    fn key(&self) -> u64 {
        self.app.panels.bots.live[0].key
    }

    fn told(&self, text: &str) -> bool {
        chat_of(self.dir.path(), "prs").iter().any(|line| line.kind == Kind::You && line.text == text)
    }
}

fn texts(orders: &[(i64, Order)]) -> Vec<&str> {
    orders.iter().map(|(_, order)| html(order)).filter(|text| !text.is_empty()).collect()
}

fn answer(orders: &[(i64, Order)]) -> Option<&(i64, Order)> {
    orders.iter().find(|(_, order)| html(order).contains(BLOCKED))
}

fn reply_to(order: &Order) -> Option<i64> {
    match order {
        Order::Send { reply_to, .. } => *reply_to,
        _ => None,
    }
}

fn reacted_to(orders: &[(i64, Order)], message: i64) -> bool {
    orders.iter().any(|(_, order)| matches!(order, Order::React { message: m, .. } if *m == message))
}

#[rstest]
#[case::by_default_while_away(ChatApps::Away, true, true)]
#[case::by_default_not_at_the_desk(ChatApps::Away, false, false)]
#[case::always_at_the_desk(ChatApps::Always, false, true)]
#[case::never_when_off(ChatApps::Off, true, false)]
fn a_routines_answer_reaches_the_chat_app_as_the_bots_option_says(
    #[values(Via::Telegram, Via::Discord)] via: Via,
    #[case] chat_apps: ChatApps,
    #[case] away: bool,
    #[case] reaches: bool,
) {
    let mut b = Bots::new(via, chat_apps);
    if away {
        b.away();
    }
    b.run_routine();
    let sent = b.sent();
    assert_eq!(answer(&sent).is_some(), reaches, "{:?}", texts(&sent));
    if let Some((_, order)) = answer(&sent) {
        let text = html(order);
        assert!(text.contains("🤖 PRs") && text.contains("↻ Review PRs"), "{text}");
        assert_eq!(reply_to(order), None);
    }
}

#[test]
fn a_message_typed_in_the_window_sends_only_the_answer() {
    let mut b = Bots::new(Via::Telegram, ChatApps::Always);
    b.app.bot_say("prs", "hello from the desk", false).expect("say");
    b.settle();
    let sent = b.sent();
    assert_eq!(texts(&sent).len(), 1, "{:?}", texts(&sent));
    assert!(texts(&sent)[0].contains(BLOCKED));
    assert!(!texts(&sent)[0].contains("hello from the desk"));
}

#[rstest]
fn a_reply_to_a_bots_answer_is_taken_and_answered_as_a_reply(#[values(Via::Telegram, Via::Discord)] via: Via) {
    let mut b = Bots::new(via, ChatApps::Always);
    b.run_routine();
    let sent = b.sent();
    let (routine_answer, _) = answer(&sent).expect("the routine's answer");
    let asked = b.phone.replies(&mut b.app, *routine_answer, "which one is the oldest?");
    b.settle();
    let sent = b.sent();
    assert!(reacted_to(&sent, asked), "{sent:?}");
    assert!(b.told("which one is the oldest?"));
    let (_, again) = answer(&sent).expect("the answer");
    assert_eq!(reply_to(again), Some(asked));
    let argv = argvs(&b.claude).pop().unwrap_or_default();
    assert!(argv.contains("your routines answered"), "{argv}");
}

#[test]
fn a_message_from_the_chat_app_is_answered_there_even_when_off() {
    let mut b = Bots::new(Via::Discord, ChatApps::Off);
    let asked = b.phone.says(&mut b.app, "/bot PRs hello");
    b.settle();
    let sent = b.sent();
    assert!(b.told("hello"));
    assert!(reacted_to(&sent, asked));
    assert_eq!(answer(&sent).and_then(|(_, order)| reply_to(order)), Some(asked));
}

#[rstest]
fn bots_lists_each_bot_with_a_button_that_asks_what_to_tell_it(#[values(Via::Telegram, Via::Discord)] via: Via) {
    let mut b = Bots::new(via, ChatApps::Away);
    b.schedule(bot_routine(crate::time::now() + 3_600, "2h"));
    b.app.panels.bots.live[0].unread = 2;
    b.phone.says(&mut b.app, "/bots");
    let sent = b.sent();
    let (message, list) = sent.iter().find(|(_, order)| html(order).contains("🤖 PRs")).expect("the list");
    let text = html(list);
    assert!(text.contains("🤖 PRs · idle · 2 unread · next "), "{text}");
    let data = button(list, "✎ PRs");
    assert_eq!(data, format!("{}:bot:{}", wiring::run(&b.app, via), b.key()));
    b.phone.taps(&mut b.app, *message, &data);
    let sent = b.sent();
    let (ask, _) = sent.iter().find(|(_, order)| html(order).contains("Write to")).expect("the ask");
    b.phone.replies(&mut b.app, *ask, "look again");
    b.settle();
    assert!(b.told("look again"));
}

#[test]
fn bot_finds_the_bot_by_name_or_lists_them_or_asks_for_the_text() {
    let mut b = Bots::new(Via::Telegram, ChatApps::Away);
    b.phone.says(&mut b.app, "/bot nobody hi");
    let sent = b.sent();
    assert!(texts(&sent).iter().any(|text| text.contains("No bot has that name.") && text.contains("🤖 PRs")));
    b.phone.says(&mut b.app, "/bot prs");
    let sent = b.sent();
    assert!(texts(&sent).iter().any(|text| text.contains("Write to")), "{:?}", texts(&sent));
    b.phone.says(&mut b.app, "/bot prs hello there");
    b.settle();
    assert!(b.told("hello there"));
}

#[test]
fn bot_on_telegram_takes_a_message_of_several_lines() {
    let mut b = Bots::new(Via::Telegram, ChatApps::Away);
    b.phone.says(&mut b.app, "/bot prs hello\nand a second line");
    b.settle();
    assert!(b.told("hello\nand a second line"));
}

#[test]
fn a_reply_to_a_removed_bot_or_a_message_too_long_is_answered_with_why() {
    let mut b = Bots::new(Via::Telegram, ChatApps::Always);
    b.run_routine();
    let sent = b.sent();
    let (routine_answer, _) = answer(&sent).expect("the routine's answer");
    let long = b.phone.says(&mut b.app, &format!("/bot prs {}", "x".repeat(20_001)));
    let sent = b.sent();
    let refusal = sent.iter().find(|(_, order)| reply_to(order) == Some(long)).expect("a refusal");
    assert!(html(&refusal.1).contains("a bot takes at most 20000"), "{}", html(&refusal.1));
    b.app.bot_remove("prs").expect("removed");
    let asked = b.phone.replies(&mut b.app, *routine_answer, "still there?");
    let sent = b.sent();
    let gone = sent.iter().find(|(_, order)| reply_to(order) == Some(asked)).expect("an answer");
    assert!(html(&gone.1).contains("That bot is gone."), "{}", html(&gone.1));
}

#[test]
fn a_failed_run_is_sent_once_while_away() {
    let mut b = Bots::new(Via::Telegram, ChatApps::Away);
    b.away();
    b.app.config.agent_commands.insert("claude".into(), "/nonexistent/claude".into());
    b.schedule(bot_routine(START, "2h"));
    b.app.refresh_at(Instant::now(), START);
    let sent = b.sent();
    let failed: Vec<&str> = texts(&sent).into_iter().filter(|text| text.contains("not installed")).collect();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0].contains("✗ claude is not installed") && failed[0].contains("🤖 PRs"), "{}", failed[0]);
}

#[test]
fn an_answer_is_not_sent_to_a_chat_paired_after_the_message_was_written() {
    let mut b = Bots::new(Via::Telegram, ChatApps::Off);
    let asked = b.phone.says(&mut b.app, "/bot prs hello");
    let other = Chat { id: 77, user: USER, name: "@other".into(), paired: 2 };
    wiring::set_up(&mut b.app, Via::Telegram, "1:t", "cornercase_bot", Some(other));
    b.settle();
    assert!(b.told("hello"));
    let sent = b.sent();
    assert!(answer(&sent).is_none(), "{:?}", texts(&sent));
    assert!(!sent.iter().any(|(_, order)| reply_to(order) == Some(asked)), "{sent:?}");
}

#[test]
fn a_discord_command_names_its_bot_apart_from_its_text() {
    let mut b = Bots::new(Via::Discord, ChatApps::Off);
    let dir = b.dir.path().to_path_buf();
    for (id, name) in [("my", "My"), ("my-bot", "My bot")] {
        crate::bots::folder::create(&dir, &crate::bots::sample(id, name), "").expect("create");
    }
    b.app.set_bots_dir(&dir);
    b.phone.says(&mut b.app, "/bot My\nbot hi");
    let lines = chat_of(&dir, "my");
    assert!(lines.iter().any(|line| line.kind == Kind::You && line.text == "bot hi"), "{lines:?}");
    assert_eq!(chat_of(&dir, "my-bot"), Vec::new());
}
