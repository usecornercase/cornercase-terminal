use rstest::rstest;

use super::*;
use crate::app::messenger::By;
use crate::discord::forum::{Checklist, Progress};
use crate::issues::Secret;
use crate::messenger::{Forum, Tags};

const FORUM: i64 = 800;
pub(super) const THREAD: i64 = 900;
const WAITING: i64 = 12;

pub(super) struct Posting {
    pub(super) agent: Agent,
    orders: Receiver<Order>,
    pub(super) at: Instant,
}

impl Posting {
    pub(super) fn new() -> Self {
        let mut agent = Agent::new();
        agent.app.projects[0].name = Some("web".into());
        wiring::set_up(&mut agent.app, Via::Discord, TOKEN, "cornercase", Some(chat()));
        wiring::set_forum(&mut agent.app, Via::Discord, Some(forum()));
        agent.app.config.discord_forum = true;
        let orders = wiring::capture(&mut agent.app, Via::Discord);
        agent.app.go_away(Via::Discord, By::Command).expect("away");
        let mut posting = Self { agent, orders, at: Instant::now() };
        posting.sent();
        posting
    }

    pub(super) fn start(&mut self, workspace: Option<u64>) -> u64 {
        self.start_with(workspace, None)
    }

    fn start_with(&mut self, workspace: Option<u64>, prompt: Option<&str>) -> u64 {
        let prompt = prompt.map(str::to_string);
        let start = wire::Start { agent: Some("claude".into()), workspace, prompt, ..wire::Start::default() };
        ask(&mut self.agent.app, None, Command::Start(start));
        let id = done(self.agent.answered("the agent is ready")).ids.pane.expect("the agent's pane");
        self.agent.refreshing("the agent is seen", |a| pane(a, id).agent.status().is_some());
        id
    }

    pub(super) fn report(&mut self, pane: u64, state: &str) {
        let term = self.agent.app.projects.pane_by_mut(pane).expect("the agent's pane");
        term.feed(format!("\x1b]7501;state={state}\x07").as_bytes());
        for _ in 0..2 {
            self.at = (self.at + Duration::from_secs(2)).max(Instant::now());
            drain(&mut self.agent.app, &self.agent.rx);
            self.agent.app.refresh(self.at);
        }
    }

    pub(super) fn asks_again(&mut self, pane: u64) {
        self.report(pane, "working");
        self.report(pane, "blocked");
    }

    pub(super) fn sent(&mut self) -> Vec<Order> {
        drain(&mut self.agent.app, &self.agent.rx);
        self.orders.try_iter().collect()
    }

    pub(super) fn waits(&mut self, what: &str, pick: impl Fn(&Order) -> bool) -> Vec<Order> {
        let mut got = Vec::new();
        wait_until(what, || {
            drain(&mut self.agent.app, &self.agent.rx);
            self.agent.app.refresh(Instant::now());
            got.extend(self.orders.try_iter());
            got.iter().any(&pick)
        });
        got
    }

    pub(super) fn link(&mut self, event: LinkEvent) {
        let event = AppEvent::Messenger(MessengerEvent::Link(Via::Discord, event));
        self.agent.app.handle_event(event, AREA).expect("the link's event is handled");
    }

    pub(super) fn posted(&mut self, post: &Order) {
        let tag = post.tag().expect("the post is tagged");
        self.link(LinkEvent::Posted { link: 1, tag, thread: THREAD, message: 5000 });
    }

    pub(super) fn failed(&mut self, order: &Order) {
        let tag = order.tag().expect("the order is tagged");
        self.link(LinkEvent::Failed { link: 1, tag });
    }

    pub(super) fn gone(&mut self, order: &Order) {
        self.link(LinkEvent::Gone { link: 1, thread: THREAD, order: order.clone() });
    }

    pub(super) fn workspace(&mut self, name: &str) -> u64 {
        let new = wire::NewWorkspace { name: name.into(), ..wire::NewWorkspace::default() };
        ask(&mut self.agent.app, None, Command::NewWorkspace(new));
        done(self.agent.answered("the workspace is made")).ids.workspace.expect("the workspace")
    }

    fn place(&self, pane: u64) -> String {
        let at = self.agent.app.projects.tab_of_pane(pane).expect("the agent's tab").workspace();
        self.agent.app.place_name(at)
    }
}

pub(super) fn posts(orders: &[Order]) -> Vec<&Order> {
    orders.iter().filter(|order| matches!(order, Order::Post { .. })).collect()
}

pub(super) fn sends(orders: &[Order]) -> Vec<(i64, &str)> {
    orders
        .iter()
        .filter_map(|order| match order {
            Order::Send { chat, text, .. } => Some((*chat, text.as_str())),
            _ => None,
        })
        .collect()
}

pub(super) fn needs_you(order: &Order) -> bool {
    match order {
        Order::Post { text, .. } | Order::Send { text, .. } => text.starts_with("**! claude needs you**"),
        _ => false,
    }
}

pub(super) fn first_post(posting: &mut Posting, pane: u64) -> Order {
    posting.report(pane, "blocked");
    let got = posting.waits("the first notice", needs_you);
    let made = posts(&got);
    assert_eq!(made.len(), 1, "{got:?}");
    assert!(sends(&got).is_empty(), "{got:?}");
    made[0].clone()
}

#[test]
fn the_first_notice_of_a_workspace_makes_its_post_with_the_title_and_the_tag() {
    let mut posting = Posting::new();
    let pane = posting.start(None);

    let post = first_post(&mut posting, pane);

    let Order::Post { forum, title, tag_id, text, keyboard, .. } = &post else { panic!("not a post: {post:?}") };
    assert_eq!((*forum, *tag_id), (FORUM, WAITING));
    assert_eq!(*title, posting.place(pane));
    assert!(title.starts_with("web › "), "{title}");
    assert!(text.starts_with("**! claude needs you** · "), "{text}");
    assert!(keyboard.iter().flatten().any(|key| key.text == "screen"));
}

#[test]
fn a_notice_before_the_post_is_made_follows_it_into_the_thread() {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);

    posting.asks_again(pane);
    let waiting = posting.sent();
    posting.posted(&post);
    let after = posting.sent();
    posting.asks_again(pane);
    let next = posting.sent();

    assert!(waiting.is_empty(), "{waiting:?}");
    assert!(matches!(sends(&after)[..], [(THREAD, text)] if text.starts_with("**! claude needs you**")), "{after:?}");
    assert!(posts(&next).is_empty() && sends(&next).iter().all(|(chat, _)| *chat == THREAD), "{next:?}");
}

#[test]
fn another_workspace_gets_a_post_of_its_own() {
    let mut posting = Posting::new();
    let first = posting.start(None);
    let post = first_post(&mut posting, first);
    posting.posted(&post);
    let api = posting.workspace("api");
    let second = posting.start(Some(api));

    let other = first_post(&mut posting, second);

    let Order::Post { title, .. } = &other else { panic!("not a post: {other:?}") };
    assert!(title.ends_with(" › api"), "{title}");
}

#[test]
fn a_post_that_cannot_be_made_sends_its_messages_to_the_dm() {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.asks_again(pane);

    posting.failed(&post);
    let dm = posting.sent();

    let chats: Vec<i64> = sends(&dm).iter().map(|(chat, _)| *chat).collect();
    assert_eq!(chats, [CHAT, CHAT], "{dm:?}");
    assert!(sends(&dm).iter().all(|(_, text)| text.starts_with("**! claude needs you**")));
    let note = posting.agent.app.messenger_settings(Via::Discord).forum;
    assert_eq!(note, "home › #cornercase · a post could not be made, so its messages went to the DM");
    posting.asks_again(pane);
    assert_eq!(posts(&posting.sent()).len(), 1, "the next notice tries again");
}

#[test]
fn with_forum_mode_off_notices_go_to_the_dm_and_the_post_is_kept_for_later() {
    let server = FakeHttp::start(vec![("GET /gateway/bot", 429, r#"{"retry_after":60}"#)]);
    let mut posting = Posting::new();
    wiring::point(&mut posting.agent.app, Via::Discord, &server.url());
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);

    posting.agent.app.change_config(|config| config.discord_forum = false).expect("forum mode off");
    let restarted = posting.orders.try_recv();
    posting.orders = wiring::capture(&mut posting.agent.app, Via::Discord);
    posting.asks_again(pane);
    let off = posting.sent();
    posting.agent.app.change_config(|config| config.discord_forum = true).expect("forum mode on");
    posting.orders = wiring::capture(&mut posting.agent.app, Via::Discord);
    posting.asks_again(pane);
    let on = posting.sent();

    assert_eq!(restarted, Err(mpsc::TryRecvError::Disconnected), "the link starts again");
    let chats = |orders: &[Order]| sends(orders).iter().map(|(chat, _)| *chat).collect::<Vec<_>>();
    assert!(posts(&off).is_empty() && posts(&on).is_empty(), "{off:?} {on:?}");
    assert_eq!(chats(&off), [CHAT]);
    assert_eq!(chats(&on), [THREAD]);
}

#[test]
fn a_long_title_is_cut_to_a_hundred_characters() {
    let mut posting = Posting::new();
    posting.agent.app.projects[0].name = Some("p".repeat(300));
    let pane = posting.start(None);

    let post = first_post(&mut posting, pane);

    let Order::Post { title, .. } = &post else { panic!("not a post: {post:?}") };
    assert_eq!(title.chars().count(), 100, "{title}");
    assert!(title.ends_with('…'), "{title}");
}

#[test]
fn a_long_message_starts_the_post_and_the_rest_follows_in_the_thread() {
    let mut posting = Posting::new();
    let id = posting.start_with(None, Some("write a lot"));
    posting.agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    let long = "word ".repeat(799);
    let line = format!(
        r#"{{"type":"assistant","timestamp":"2026-10-13T10:00:00.000Z","message":{{"id":"t2","model":"claude-opus-5-5","content":[{{"type":"text","text":"{}"}}]}}}}"#,
        long.trim()
    );
    posting.agent.transcript(id, &[&line]);
    posting.agent.finish();

    let got = posting.waits("the post", |order| matches!(order, Order::Post { .. }));
    let Some(post) = posts(&got).first().copied().cloned() else { panic!("no post in {got:?}") };
    let held = sends(&got).len();
    posting.posted(&post);
    let rest = posting.sent();

    let Order::Post { text, .. } = &post else { panic!("not a post") };
    assert!(text.chars().count() <= 2000, "{}", text.chars().count());
    assert_eq!(held, 0, "{got:?}");
    let follow = sends(&rest);
    assert!(!follow.is_empty() && follow.iter().all(|(chat, text)| *chat == THREAD && text.chars().count() <= 2000));
    let words = |text: &str| text.matches("word").count();
    assert_eq!(words(text) + follow.iter().map(|(_, text)| words(text)).sum::<usize>(), 799);
}

fn forum_set_up_again(posting: &mut Posting, channel: i64) {
    let forum =
        Forum { guild: 7, guild_name: "home".into(), channel, tags: Tags { working: 11, waiting: WAITING, done: 13 } };
    let progress = Progress { checklist: Checklist::default(), forum: Some(Ok(forum)) };
    let read = MessengerEvent::Forum { via: Via::Discord, token: Secret(TOKEN.into()), read: Ok(progress) };
    posting.agent.app.open_forum(Via::Discord);
    posting.agent.app.handle_event(AppEvent::Messenger(read), AREA).expect("the forum's event is handled");
    posting.orders = wiring::capture(&mut posting.agent.app, Via::Discord);
}

#[rstest]
#[case::in_another_forum_makes_a_new_post(FORUM + 1, 1)]
#[case::in_the_same_forum_keeps_the_thread(FORUM, 0)]
fn after_the_forum_is_set_up_again_the_next_notice(#[case] channel: i64, #[case] made: usize) {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);

    forum_set_up_again(&mut posting, channel);
    posting.asks_again(pane);
    let got = posting.sent();

    assert_eq!(posts(&got).len(), made, "{got:?}");
    assert!(posts(&got).iter().all(|post| matches!(post, Order::Post { forum, .. } if *forum == channel)), "{got:?}");
}

mod persistence;
mod refused;
