use super::posts::{Posting, THREAD, first_post, posts, sends};
use super::*;

const URL: &str = "https://discord.com/channels/7/900";

fn started(order: &Order) -> bool {
    match order {
        Order::Post { text, .. } | Order::Send { text, .. } => text.starts_with("Started **claude**"),
        _ => false,
    }
}

fn says(posting: &mut Posting, id: i64, text: &str) {
    let message = Message {
        id,
        chat: CHAT,
        user: USER,
        private: true,
        guild: None,
        date: 0,
        name: "@ada".into(),
        text: text.into(),
        reply_to: None,
    };
    update(&mut posting.agent.app, Via::Discord, Update::Message(message));
}

fn new_agent(posting: &mut Posting) -> Vec<Order> {
    posting.agent.app.config.agent = "claude".into();
    says(posting, 7001, "/new");
    posting.waits("the prompt question", |order| html(order).contains("What should **claude** do"));
    says(posting, 7002, "write the docs");
    posting.waits("the agent starts", started)
}

fn pointer(orders: &[Order]) -> Option<(i64, &str, Option<i64>)> {
    orders.iter().find_map(|order| match order {
        Order::Send { chat, text, reply_to, .. } if text.contains(URL) => Some((*chat, text.as_str(), *reply_to)),
        _ => None,
    })
}

#[test]
fn new_makes_the_workspace_post_and_the_dm_links_to_it_once_it_is_made() {
    let mut posting = Posting::new();
    posting.start(None);

    let got = new_agent(&mut posting);
    let made = posts(&got);
    assert_eq!(made.len(), 1, "{got:?}");
    assert!(matches!(made[0], Order::Post { text, .. } if text.starts_with("Started **claude** in web › ")));
    assert!(pointer(&got).is_none(), "the dm waits for the post: {got:?}");
    let post = made[0].clone();
    posting.posted(&post);
    let after = posting.sent();

    let (chat, text, reply_to) = pointer(&after).expect("the dm's line");
    assert_eq!((chat, reply_to), (CHAT, Some(7002)));
    assert!(text.starts_with("Started **claude** in web › ") && text.ends_with(&format!(": {URL}")), "{text}");
}

#[test]
fn new_in_a_workspace_with_a_post_answers_in_its_thread_and_links_it_at_once() {
    let mut posting = Posting::new();
    let pane = posting.start(None);
    let post = first_post(&mut posting, pane);
    posting.posted(&post);
    posting.sent();

    let got = new_agent(&mut posting);

    assert!(posts(&got).is_empty(), "{got:?}");
    let thread = sends(&got).into_iter().filter(|(_, text)| text.starts_with("Started **claude**")).collect::<Vec<_>>();
    assert_eq!(thread.len(), 2, "{got:?}");
    assert!(thread.iter().any(|(chat, text)| *chat == THREAD && text.contains("Reply to this message")), "{got:?}");
    let (chat, text, _) = pointer(&got).expect("the dm's line");
    assert_eq!(chat, CHAT);
    assert!(text.starts_with("Started **claude** in web › ") && text.ends_with(URL), "{text}");
}

#[test]
fn new_whose_post_cannot_be_made_answers_in_the_dm_without_a_link() {
    let mut posting = Posting::new();
    posting.start(None);
    let got = new_agent(&mut posting);
    let post = posts(&got)[0].clone();

    posting.failed(&post);
    let after = posting.sent();

    let to_dm = sends(&after);
    assert_eq!(to_dm.len(), 1, "{after:?}");
    assert!(to_dm[0].0 == CHAT && to_dm[0].1.contains("Reply to this message"), "{after:?}");
    assert!(pointer(&after).is_none(), "{after:?}");
}
