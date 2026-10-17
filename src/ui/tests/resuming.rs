use super::*;

fn resuming_view() -> View<'static> {
    let tab = TabView { resuming: vec![Some("claude".into())], ..single(screen(b"$ ")) };
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
