use super::*;

fn resuming_view() -> View<'static> {
    let tab = TabView { bands: vec![Some(PaneBand::Resuming("claude".into()))], ..single(screen(b"$ ")) };
    View { tab: Some(tab), ..view(&["~"]) }
}

#[test]
fn a_resuming_pane_says_so_with_a_cancel_button_in_its_middle() {
    let pane = areas().pane;
    let (label, button) = resume_notice(pane, "claude").expect("the notice fits");
    let t = render(&resuming_view());

    assert_eq!(label.y, pane.y + pane.height / 2);
    assert_eq!((row_text(&t, label), row_text(&t, button)), (" resuming claude… ".into(), " cancel ".into()));
}

#[test]
fn a_pane_too_narrow_for_the_notice_has_none() {
    assert_eq!(resume_notice(Rect::new(0, 0, 20, 5), "claude"), None);
}

#[test]
fn a_pane_that_is_not_resuming_has_no_notice() {
    let (label, _) = resume_notice(areas().pane, "claude").expect("the notice fits");
    let t = render(&View { tab: Some(single(screen(b"$ "))), ..view(&["~"]) });

    assert!(!row_text(&t, label).contains("resuming"));
}

#[test]
fn a_held_message_is_a_bar_on_the_panes_last_row_with_its_buttons_at_the_right() {
    let pane = areas().pane;
    let label = "1 message waiting · goes after yours";
    let tab = TabView { bands: vec![Some(PaneBand::Held(label.into()))], ..single(screen(b"$ ")) };
    let bar = held_notice(pane, 0).expect("the bar fits");
    let hide = bar.hide.expect("a hide button");
    let t = render(&View { tab: Some(tab), ..view(&["~"]) });

    assert_eq!((bar.text.y, bar.text.x, hide.right()), (pane.bottom() - 1, pane.x, pane.right()));
    assert_eq!((row_text(&t, bar.send), row_text(&t, hide)), (" send now ".into(), " hide ".into()));
    assert!(row_text(&t, bar.text).starts_with(" 1 message waiting"), "{}", row_text(&t, bar.text));
}

#[test]
fn the_bar_leaves_the_scrollback_badge_its_cells() {
    let pane = Rect::new(0, 0, 60, 10);
    let badge = scrollback_badge(pane, 12).expect("a badge");
    let bar = held_notice(pane, 12).expect("the bar fits");
    assert_eq!(bar.hide.expect("a hide button").right() + 1, badge.x);
}

#[test]
fn a_narrow_pane_drops_the_text_then_hide_and_keeps_send_now() {
    let bar = |width| held_notice(Rect::new(0, 0, width, 5), 0).expect("the bar fits");
    assert_eq!((bar(40).text.width, bar(40).send.width), (22, 10));
    assert_eq!((bar(20).text.width, bar(20).hide.is_some()), (0, true));
    assert_eq!((bar(12).hide, bar(12).send.x), (None, 2));
    assert!(held_notice(Rect::new(0, 0, 9, 5), 0).is_none());
}
