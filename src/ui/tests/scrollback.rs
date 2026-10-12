use super::*;

fn scrolled_view() -> View<'static> {
    let screen = Snapshot { below: 42, ..screen(b"$ cargo build\r\n   Compiling foo v0.1.0") };
    View { tab: Some(single(screen)), ..view(&["~"]) }
}

#[test]
fn a_scrolled_pane_says_how_far_up_it_is() {
    insta::assert_snapshot!(render(&scrolled_view()).backend());
}

#[test]
fn the_badge_sits_at_the_bottom_right_of_the_pane() {
    let pane = areas().pane;

    let badge = scrollback_badge(pane, 42).expect("a badge");

    assert_eq!((badge.y, badge.right()), (pane.bottom() - 1, pane.right() - 1));
    assert_eq!(row_text(&render(&scrolled_view()), badge), " ↓ 42 more ");
}

#[test]
fn a_pane_at_the_bottom_has_no_badge() {
    assert_eq!(scrollback_badge(areas().pane, 0), None);
}

#[test]
fn a_pane_too_narrow_for_the_badge_has_none() {
    assert_eq!(scrollback_badge(Rect::new(0, 0, 11, 5), 42), None);
}

#[test]
fn the_hovered_badge_is_lit() {
    let badge = scrollback_badge(areas().pane, 42).expect("a badge");
    let v = View { hover: Some(badge.as_position()), ..scrolled_view() };

    let style = render(&v).backend().buffer()[(badge.x + 1, badge.y)].style();

    assert_eq!(style.bg, Some(Color::Cyan));
}
