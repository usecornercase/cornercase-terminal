use std::path::PathBuf;

use super::*;
use crate::agents::headless::Access;
use crate::app::tests::bots::{ANSWER, install_bot, settle};
use crate::bots::ChatApps;
use crate::bots::chat::{self, Kind};
use crate::bots::folder;

const THREAD: i64 = 950;
const GUILD: i64 = 7;
const BLOCKED: &str = "Two are blocked.";

struct BotForum {
    app: App,
    rx: Receiver<AppEvent>,
    orders: Receiver<Order>,
    config: TempDir,
    bots: TempDir,
    claude: PathBuf,
    _bin: TempDir,
    next: i64,
}

fn on_the_forum(app: &mut App) -> Receiver<Order> {
    wiring::set_forum(app, Via::Discord, Some(forum()));
    app.config.discord_forum = true;
    wiring::capture(app, Via::Discord)
}

impl BotForum {
    fn new() -> Self {
        let config = TempDir::new();
        let (mut app, rx) = new_app(config.path().join("config.json"));
        let (bots, bin, claude) = install_bot(&mut app, ANSWER, Access::Cornercase);
        app.panels.bots.live[0].bot.chat_apps = ChatApps::Always;
        wiring::set_up(&mut app, Via::Discord, TOKEN, "cornercase", Some(chat()));
        let orders = on_the_forum(&mut app);
        Self { app, rx, orders, config, bots, claude, _bin: bin, next: 7000 }
    }

    fn answered(&mut self, text: &str) -> Vec<Order> {
        self.app.bot_say("prs", text, false).expect("say");
        self.settled()
    }

    fn settled(&mut self) -> Vec<Order> {
        settle(&mut self.app, &self.rx);
        self.orders.try_iter().collect()
    }

    fn posted(&mut self) -> Order {
        let orders = self.answered("hello");
        let post = orders.into_iter().find(|order| matches!(order, Order::Post { .. })).expect("a post");
        let tag = post.tag().expect("a tagged post");
        let event = LinkEvent::Posted { link: 1, tag, thread: THREAD, message: 5000 };
        self.app.handle_event(AppEvent::Messenger(MessengerEvent::Link(Via::Discord, event)), AREA).expect("posted");
        post
    }

    fn says_in_the_post(&mut self, text: &str) -> i64 {
        self.next += 1;
        let message = Message {
            id: self.next,
            chat: THREAD,
            user: USER,
            private: false,
            guild: Some(GUILD),
            date: 0,
            name: "@ada".into(),
            text: text.into(),
            reply_to: None,
        };
        update(&mut self.app, Via::Discord, Update::Message(message));
        self.next
    }

    fn told(&self, text: &str) -> bool {
        let lines = chat::read(&folder::of(self.bots.path(), "prs").join(folder::CHAT_FILE));
        lines.iter().any(|line| line.kind == Kind::You && line.text == text)
    }
}

fn sent_to(orders: &[Order], thread: i64) -> Vec<(&str, Option<i64>)> {
    orders
        .iter()
        .filter_map(|order| match order {
            Order::Send { chat, text, reply_to, .. } if *chat == thread => Some((text.as_str(), *reply_to)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bots_first_answer_makes_its_post_and_the_next_go_in_it() {
    let mut f = BotForum::new();
    let Order::Post { title, text, tag_id, .. } = f.posted() else { panic!("a post") };
    assert_eq!((title.as_str(), tag_id), ("🤖 PRs", 13));
    assert!(text.contains(BLOCKED), "{text}");
    let orders = f.answered("again");
    let sent = sent_to(&orders, THREAD);
    assert!(sent.iter().any(|(text, reply_to)| text.contains(BLOCKED) && reply_to.is_none()), "{orders:?}");
}

#[test]
fn a_message_in_the_bots_post_reaches_the_bot_and_its_answer_replies_there() {
    let mut f = BotForum::new();
    f.posted();
    let asked = f.says_in_the_post("what about mine?");
    let orders = f.settled();
    assert!(f.told("what about mine?"));
    assert!(
        orders.iter().any(|order| matches!(order, Order::React { chat: THREAD, message, .. } if *message == asked))
    );
    assert!(sent_to(&orders, THREAD).iter().any(|(text, reply_to)| text.contains(BLOCKED) && *reply_to == Some(asked)));
    f.says_in_the_post("/bots");
    let orders: Vec<Order> = f.orders.try_iter().collect();
    assert!(sent_to(&orders, THREAD).iter().any(|(text, _)| text.contains("Commands work in the direct message")));
}

#[test]
fn removing_the_bot_closes_its_post() {
    let mut f = BotForum::new();
    f.posted();
    f.app.bot_remove("prs").expect("removed");
    f.app.clean_gone_posts();
    let orders: Vec<Order> = f.orders.try_iter().collect();
    assert_eq!(
        orders,
        [
            Order::Reopen { thread: THREAD },
            Order::Retag { thread: THREAD, tag_id: 13 },
            Order::Close { thread: THREAD, lock: true }
        ]
    );
    assert!(!std::fs::read_to_string(f.config.path().join("discord.json")).unwrap_or_default().contains("bot_posts"));
}

#[test]
fn the_bots_post_is_kept_across_a_restart() {
    let mut f = BotForum::new();
    f.posted();
    let saved = std::fs::read_to_string(f.config.path().join("discord.json")).expect("discord.json");
    assert!(saved.contains(&format!(r#""bot_posts":{{"prs":{THREAD}}}"#)), "{saved}");

    let (mut again, rx) = new_app(f.config.path().join("config.json"));
    wiring::set_token(&mut again, Via::Discord, TOKEN);
    again.config.agent_commands.insert("claude".into(), f.claude.display().to_string());
    again.set_bots_dir(f.bots.path());
    let _orders = on_the_forum(&mut again);
    f.app = again;
    f.rx = rx;
    f.says_in_the_post("still here?");
    settle(&mut f.app, &f.rx);
    assert!(f.told("still here?"));
}
