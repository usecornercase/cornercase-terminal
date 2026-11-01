use std::os::unix::net::UnixStream;

use rstest::rstest;
use serde_json::json;

use super::*;
use crate::control::surface::Size;
use crate::control::terminal::{End, InputEvent, Key, Point};
use crate::machines::link::fake::{ended, request};
use crate::machines::stream::Stream;
use crate::machines::stream::tests::{BOOT, Rig, TAB, VIEW, complete, say, shown};

fn focus(tab: u64) -> Value {
    serde_json::to_value(Item::Tab(tab)).expect("an item")
}

fn answered(order: Order, reply: Result<Value, Refusal>) -> StreamEvent {
    StreamEvent::Answer(match reply {
        Ok(value) => Answer::Done { order, value },
        Err(refusal) => Answer::Refused { order, refusal },
    })
}

const PANE: u64 = 3;

fn typed(events: Vec<InputEvent>) -> Order {
    Order::Input(Input { pane: Some(PANE), size: Some(Size { cols: 80, rows: 24 }), events, ..Input::default() })
}

fn key(code: &str) -> InputEvent {
    InputEvent::Key(Key { code: code.into(), ..Key::default() })
}

fn picked(x: u16) -> Selection {
    let (from, to) = (Some(Point { x: 0, y: 0 }), Some(Point { x, y: 1 }));
    Selection { pane: Some(PANE), from, to, ..Selection::default() }
}

fn select(x: u16) -> Order {
    Order::Select(picked(x))
}

fn ratio(path: Vec<bool>, ratio: f64) -> Order {
    Order::Ratio(Ratio { tab: Some(TAB), path, ratio })
}

fn showing(rig: &Rig, tab: u64) -> UnixStream {
    rig.stream.show(tab);
    let mut orders = rig.reach();
    request(&mut orders);
    orders
}

fn sent(rig: &Rig, order: Order) -> UnixStream {
    rig.stream.send(order);
    let mut orders = rig.reach();
    request(&mut orders);
    orders
}

fn next(orders: &mut UnixStream) -> Value {
    say(orders, &json!({"ok": {}}));
    request(orders)
}

fn events(asked: &Value) -> Vec<InputEvent> {
    serde_json::from_value(asked["args"]["events"].clone()).expect("input events")
}

mod sizes {
    use super::*;

    #[test]
    fn go_to_the_view_with_its_id_and_boot() {
        let rig = Rig::new();
        let _streams = rig.opened();
        rig.stream.resize(100, 30);

        let asked = request(&mut rig.reach());

        assert_eq!(
            (&asked["command"], &asked["view"], &asked["boot"]),
            (&json!("view-set"), &json!(VIEW), &json!(BOOT))
        );
        assert_eq!((&asked["args"]["cols"], &asked["args"]["rows"]), (&json!(100), &json!(30)));
    }

    #[test]
    fn three_while_one_waits_send_the_first_and_the_last() {
        let rig = Rig::new();
        let _streams = rig.opened();
        rig.stream.resize(100, 30);
        let mut orders = rig.reach();
        let first = request(&mut orders);

        rig.stream.resize(101, 30);
        rig.stream.resize(102, 30);
        say(&mut orders, &json!({"ok": {"id": VIEW, "projection": 2}}));
        let next = request(&mut orders);

        assert_eq!((&first["args"]["cols"], &next["args"]["cols"]), (&json!(100), &json!(102)));
    }

    #[test]
    fn a_size_asked_before_the_view_opens_goes_out_once_it_does() {
        let rig = Rig::new();
        let mut view = rig.reach();
        request(&mut view);
        rig.stream.resize(90, 20);

        say(&mut view, &json!({"ok": {"revision": 1, "boot": BOOT, "view": {"id": VIEW, "projection": 1}}}));
        assert_eq!(rig.event(), StreamEvent::Opened { view: VIEW });
        let asked = [request(&mut rig.reach()), request(&mut rig.reach())];

        let sizes = asked.iter().filter(|asked| asked["command"] == "view-set").map(|asked| &asked["args"]["cols"]);
        assert_eq!(sizes.collect::<Vec<_>>(), [&json!(90)]);
    }
}

mod tabs {
    use super::*;

    #[test]
    fn showing_another_tab_focuses_it_in_the_view() {
        let rig = Rig::new();
        let _streams = rig.opened();
        rig.stream.show(TAB);
        rig.stream.show(12);

        let asked = request(&mut rig.reach());

        assert_eq!((&asked["command"], &asked["view"]), (&json!("focus"), &json!(VIEW)));
        assert_eq!(asked["args"]["item"], focus(12));
    }

    #[test]
    fn an_answer_s_projection_fences_older_frames() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();
        let mut orders = showing(&rig, 12);

        say(&mut orders, &json!({"ok": {"tab": 12, "projection": 4}}));
        assert_eq!(rig.event(), answered(Order::Show(12), Ok(json!({"tab": 12, "projection": 4}))));
        say(&mut surface, &complete(1, 3, "old"));
        say(&mut surface, &complete(2, 4, "new"));

        assert_eq!(shown(rig.event()), "new");
    }
}

mod answers {
    use super::*;

    #[rstest]
    #[case::conflict("conflict")]
    #[case::not_found("not-found")]
    fn that_say_the_place_moved_are_dropped(#[case] code: &str) {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = showing(&rig, 12);

        say(&mut orders, &json!({"error": {"code": code, "message": "no tab 12"}}));
        rig.stream.show(13);
        request(&mut orders);
        say(&mut orders, &json!({"ok": {}}));

        assert_eq!(rig.event(), answered(Order::Show(13), Ok(json!({}))));
    }

    #[test]
    fn other_refusals_are_told_cleaned() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = showing(&rig, 12);

        say(&mut orders, &json!({"error": {"code": "failed", "message": "not\u{1b} there"}}));

        assert_eq!(rig.event(), answered(Order::Show(12), Err(Refusal::new(Code::Failed, "not there"))));
    }

    #[test]
    fn a_lost_line_is_told_and_the_next_order_connects_again() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let orders = sent(&rig, typed(vec![key("a")]));

        drop(orders);
        let told = rig.event();
        rig.stream.send(typed(vec![key("b")]));

        assert_eq!(told, answered(typed(vec![key("a")]), Err(Refusal::new(Code::Unavailable, "the link ended"))));
        assert_eq!(events(&request(&mut rig.reach())), [key("b")]);
    }

    #[test]
    fn one_never_answered_is_told_late_and_the_next_connects_again() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let _opening = rig.alarm();
        rig.stream.send(typed(vec![key("a")]));
        let mut silent = rig.reach();
        request(&mut silent);

        rig.alarm().ring();
        let told = rig.event();
        rig.stream.send(typed(vec![key("b")]));

        assert_eq!(told, answered(typed(vec![key("a")]), Err(Refusal::new(Code::Unavailable, LATE))));
        assert_eq!(events(&request(&mut rig.reach())), [key("b")]);
    }

    #[test]
    fn one_whose_connection_is_stuck_before_its_banner_is_told_late() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let _opening = rig.alarm();
        rig.hang();
        rig.stream.send(typed(vec![key("a")]));

        rig.alarm().ring();

        assert_eq!(rig.event(), answered(typed(vec![key("a")]), Err(Refusal::new(Code::Unavailable, LATE))));
    }

    #[rstest]
    #[case::a_tab(|stream: &Stream| { stream.show(12); }, Order::Show(12), "tab", json!(12))]
    #[case::a_size(|stream: &Stream| stream.resize(100, 30), Order::Size { cols: 100, rows: 30 }, "cols", json!(100))]
    fn lost_with_the_orders_line_opens_the_view_again_with_it(
        #[case] change: fn(&Stream),
        #[case] order: Order,
        #[case] arg: &str,
        #[case] value: Value,
    ) {
        let rig = Rig::new();
        let (mut view, mut surface) = rig.opened();
        change(&rig.stream);
        let mut orders = rig.reach();
        request(&mut orders);

        drop(orders);

        assert_eq!(rig.event(), answered(order, Err(Refusal::new(Code::Unavailable, "the link ended"))));
        assert!(ended(&mut surface) && ended(&mut view));
        assert_eq!(rig.event(), StreamEvent::Down("the link ended".into()));
        let asked = request(&mut rig.reach());
        assert_eq!((&asked["command"], &asked["args"][arg]), (&json!("view"), &value));
    }
}

mod input {
    use super::*;

    #[test]
    fn keys_and_a_paste_while_one_waits_go_out_after_it_in_order() {
        let rig = Rig::new();
        let _streams = rig.opened();
        rig.stream.send(typed(vec![key("a")]));
        let mut orders = rig.reach();
        let first = request(&mut orders);

        rig.stream.send(typed(vec![key("b")]));
        rig.stream.send(typed(vec![key("c")]));
        rig.stream.send(typed(vec![InputEvent::Paste("two\nlines".into())]));
        say(&mut orders, &json!({"ok": {"pane": PANE}}));
        let next = request(&mut orders);

        assert_eq!((&first["command"], &first["view"], &first["boot"]), (&json!("input"), &Value::Null, &json!(BOOT)));
        assert_eq!(events(&first), [key("a")]);
        assert_eq!(events(&next), [key("b"), key("c"), InputEvent::Paste("two\nlines".into())]);
    }

    #[test]
    fn input_for_another_size_waits_for_its_own_request() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, typed(vec![key("a")]));

        rig.stream.send(typed(vec![key("b")]));
        let Order::Input(resized) = typed(vec![key("c")]) else { unreachable!("typed builds input") };
        rig.stream.send(Order::Input(Input { size: Some(Size { cols: 81, rows: 24 }), ..resized }));
        let second = next(&mut orders);
        let third = next(&mut orders);

        assert_eq!((events(&second), events(&third)), (vec![key("b")], vec![key("c")]));
        assert_eq!(third["args"]["size"], json!({"cols": 81, "rows": 24}));
    }

    #[test]
    fn a_paste_that_would_pass_the_joined_limit_goes_in_its_own_request_after_the_rest() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, typed(vec![key("a")]));
        let big = InputEvent::Paste("x".repeat(JOINED_BYTES / 2 + 1));

        rig.stream.send(typed(vec![key("b"), big.clone()]));
        rig.stream.send(typed(vec![big.clone()]));
        rig.stream.send(typed(vec![key("c")]));
        let second = next(&mut orders);
        let third = next(&mut orders);

        assert_eq!((events(&second), events(&third)), (vec![key("b"), big.clone()], vec![big, key("c")]));
    }

    #[test]
    fn busy_is_told_as_not_reading() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, typed(vec![key("a")]));

        say(&mut orders, &json!({"error": {"code": "busy", "message": "the program is not reading its input"}}));

        assert_eq!(rig.event(), StreamEvent::Answer(Answer::NotReading));
    }
}

mod selections {
    use super::*;

    #[test]
    fn several_while_one_waits_send_the_first_and_the_last() {
        let rig = Rig::new();
        let _streams = rig.opened();
        rig.stream.send(select(1));
        let mut orders = rig.reach();
        let first = request(&mut orders);

        rig.stream.send(select(2));
        rig.stream.send(typed(vec![key("a")]));
        rig.stream.send(select(3));
        say(&mut orders, &json!({"ok": {"pane": PANE}}));
        let second = request(&mut orders);
        let third = next(&mut orders);

        assert_eq!((&first["command"], &first["args"]["to"]["x"]), (&json!("selection"), &json!(1)));
        assert_eq!(
            (&second["command"], &third["command"], &third["args"]["to"]["x"]),
            (&json!("input"), &json!("selection"), &json!(3))
        );
    }

    #[test]
    fn only_the_last_one_tells_its_text_and_it_replaces_one_waiting_without_a_copy_there() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, select(1));

        rig.stream.send(select(2));
        rig.stream.send(Order::Copy(picked(3)));
        say(&mut orders, &json!({"ok": {"pane": PANE, "text": "a"}}));
        let last = request(&mut orders);

        say(&mut orders, &json!({"ok": {"pane": PANE, "text": "abc"}}));

        assert_eq!((&last["command"], &last["args"]["to"]["x"]), (&json!("selection"), &json!(3)));
        assert_eq!(
            (&last["args"]["copy"], rig.event()),
            (&json!(false), StreamEvent::Answer(Answer::Selected(Some("abc".into()))))
        );
    }

    #[rstest]
    #[case::lines_kept(json!({"pane": PANE, "text": "one\u{1b}[31m\r\ntwo\u{200b}\tend\n"}), Some("one[31m\ntwo end\n"))]
    #[case::nothing_selected(json!({"pane": PANE}), None)]
    #[case::cut_to_what_its_cells_can_hold(json!({"pane": PANE, "text": "x".repeat(200)}), Some("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"))]
    fn come_back_with_their_text_cleaned(#[case] answer: Value, #[case] expected: Option<&str>) {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, Order::Copy(Selection { size: Some(Size { cols: 1, rows: 2 }), ..picked(4) }));

        say(&mut orders, &json!({"ok": answer}));

        assert_eq!(rig.event(), StreamEvent::Answer(Answer::Selected(expected.map(String::from))));
    }
}

mod panes {
    use super::*;

    #[rstest]
    #[case::scroll(Order::Scroll(Scroll { pane: Some(PANE), lines: Some(-3), to: None }), "scroll", false)]
    #[case::to_bottom(Order::Scroll(Scroll { pane: Some(PANE), lines: None, to: Some(End::Bottom) }), "scroll", false)]
    #[case::ratio(ratio(vec![true], 0.25), "ratio", false)]
    #[case::focus(Order::Focus(PANE), "focus", true)]
    fn go_with_the_boot_and_the_view_only_to_focus(#[case] order: Order, #[case] command: &str, #[case] viewed: bool) {
        let rig = Rig::new();
        let _streams = rig.opened();
        rig.stream.send(order.clone());

        let asked = request(&mut rig.reach());

        assert_eq!((&asked["command"], &asked["boot"]), (&json!(command), &json!(BOOT)));
        assert_eq!(asked["view"], if viewed { json!(VIEW) } else { Value::Null });
        if let Order::Focus(pane) = order {
            assert_eq!(asked["args"]["item"], serde_json::to_value(Item::Pane(pane)).expect("an item"));
        }
    }

    #[test]
    fn a_ratio_replaces_one_waiting_for_the_same_divider() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, ratio(vec![true], 0.25));

        rig.stream.send(ratio(vec![true], 0.3));
        rig.stream.send(ratio(Vec::new(), 0.6));
        rig.stream.send(ratio(vec![true], 0.4));
        let second = next(&mut orders);
        let third = next(&mut orders);

        assert_eq!((&second["args"]["path"], &second["args"]["ratio"]), (&json!([]), &json!(0.6)));
        assert_eq!((&third["args"]["path"], &third["args"]["ratio"]), (&json!([true]), &json!(0.4)));
    }

    #[test]
    fn a_focused_pane_is_told_with_its_answer() {
        let rig = Rig::new();
        let _streams = rig.opened();
        let mut orders = sent(&rig, Order::Focus(PANE));

        say(&mut orders, &json!({"ok": {"pane": PANE}}));

        assert_eq!(rig.event(), answered(Order::Focus(PANE), Ok(json!({"pane": PANE}))));
    }
}
