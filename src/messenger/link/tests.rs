use std::sync::mpsc;
use std::time::Duration;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

struct Fake<const POSTS: bool> {
    fails: Option<&'static str>,
}

impl<const POSTS: bool> Transport for Fake<POSTS> {
    type Failure = String;
    type Attempt = ();

    fn perform(&self, _: &Order, (): &()) -> Result<Option<i64>, String> {
        self.fails.map_or(Ok(Some(5)), |why| Err(why.to_string()))
    }

    fn next(failure: String, (): &mut ()) -> Next<String> {
        Next::Done(Err(failure))
    }

    fn method(_: &Order) -> &'static str {
        "fake"
    }

    fn more((): &()) -> Vec<i64> {
        vec![6]
    }

    fn posted((): &()) -> Option<i64> {
        POSTS.then_some(70)
    }

    fn gone(order: &Order, failure: &String) -> Option<i64> {
        match order {
            Order::Retag { thread, .. } if failure == "lost" => Some(*thread),
            Order::Send { chat, .. } if failure == "lost" => Some(*chat),
            _ => None,
        }
    }

    fn archived(order: &Order, failure: &String) -> Option<i64> {
        match order {
            Order::Retag { thread, .. } if failure == "archived" => Some(*thread),
            _ => None,
        }
    }
}

fn post() -> Order {
    Order::Post {
        forum: 1,
        title: "app › main".into(),
        tag_id: 2,
        text: "hi".into(),
        keyboard: Vec::new(),
        tag: Some(3),
    }
}

fn events<const POSTS: bool>(fake: Fake<POSTS>, orders: Vec<Order>, count: usize) -> Vec<Event> {
    let (tx, rx) = mpsc::channel::<(Via, Event)>();
    let link = Link::start(1, Via::Discord, &tx, fake, |_| {});
    for order in orders {
        link.send(order);
    }
    (0..count).map(|_| rx.recv_timeout(WAIT).expect("an event").1).collect()
}

mod delivery {
    use super::*;

    #[test]
    fn a_post_reports_its_thread_with_the_first_message_and_the_rest_as_sent() {
        let events = events(Fake::<true> { fails: None }, vec![post()], 2);

        assert!(matches!(events[0], Event::Posted { link: 1, tag: 3, thread: 70, message: 5 }));
        assert!(matches!(events[1], Event::Sent { link: 1, tag: 3, message: 6 }));
    }

    #[test]
    fn a_send_reports_only_sent_messages() {
        let events = events(Fake::<false> { fails: None }, vec![post()], 2);

        assert!(matches!(events[0], Event::Sent { tag: 3, message: 5, .. }));
        assert!(matches!(events[1], Event::Sent { tag: 3, message: 6, .. }));
    }

    #[rstest::rstest]
    #[case::a_lost_forum("lost")]
    #[case::another_failure("refused")]
    fn a_tagged_post_that_fails_says_so(#[case] why: &'static str) {
        let events = events(Fake::<true> { fails: Some(why) }, vec![Order::Commands, post()], 1);

        assert!(matches!(events[0], Event::Failed { link: 1, tag: 3 }));
    }

    #[rstest::rstest]
    #[case::a_retag(Order::Retag { thread: 4, tag_id: 2 })]
    #[case::a_send(Order::Send { chat: 4, text: "hi".into(), keyboard: Vec::new(), reply_to: None, ask: None, tag: Some(3) })]
    fn an_order_on_a_lost_thread_gives_it_back_with_the_thread(#[case] order: Order) {
        let events = events(Fake::<true> { fails: Some("lost") }, vec![order.clone()], 1);

        assert!(matches!(&events[0], Event::Gone { link: 1, thread: 4, order: gone } if *gone == order));
    }

    #[test]
    fn a_retag_of_an_archived_thread_says_which_thread() {
        let retag = Order::Retag { thread: 4, tag_id: 2 };
        let events = events(Fake::<true> { fails: Some("archived") }, vec![retag], 1);

        assert!(matches!(events[0], Event::Archived { link: 1, thread: 4 }));
    }
}
