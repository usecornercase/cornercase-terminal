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
fn a_pane_with_a_held_message_shows_the_band_with_its_two_buttons() {
    let pane = areas().pane;
    let label = "1 message waiting";
    let tab = TabView { bands: vec![Some(PaneBand::Held(label.into()))], ..single(screen(b"$ ")) };
    let (text, send, discard) = held_notice(pane, label).expect("the band fits");
    let t = render(&View { tab: Some(tab), ..view(&["~"]) });

    assert_eq!(text.y, pane.y + pane.height / 2);
    assert_eq!(
        (row_text(&t, text), row_text(&t, send), row_text(&t, discard)),
        (format!(" {label} "), " send now ".into(), " discard ".into())
    );
    assert_eq!((send.x, discard.x), (text.right() + 1, send.right() + 1));
}

#[test]
fn a_narrow_pane_cuts_the_bands_text_and_keeps_its_buttons() {
    let pane = Rect::new(0, 0, 40, 5);
    let (label, send, discard) = held_notice(pane, "1 message waiting from pane 40").expect("the band fits");
    assert_eq!((label.width, send.width, discard.width, discard.right()), (19, 10, 9, 40));
    assert_eq!(held_notice(Rect::new(0, 0, 28, 5), "1 message waiting"), None);
}
