use super::*;

fn copied() -> Toast<'static> {
    Toast { message: "copied to clipboard", icon: ToastIcon::Check, button: None }
}

#[rstest]
#[case::undo("deleted", ToastIcon::Check, ToastButton::Undo, " undo ")]
#[case::cancel("restart pending", ToastIcon::Restart, ToastButton::Cancel, " cancel ")]
fn a_button_sits_at_its_end(
    #[case] message: &str,
    #[case] icon: ToastIcon,
    #[case] button: ToastButton,
    #[case] label: &str,
) {
    let toast = Toast { message, icon, button: Some(button) };
    let (r, at) = (toast_area(AREA, toast), toast_button(AREA, toast));
    let t = render(&View { toast: Some(toast), ..view(&["~"]) });
    let text: String = (at.x..at.right()).map(|x| t.backend().buffer()[(x, at.y)].symbol()).collect();
    assert_eq!((text.as_str(), at.right(), at.y), (label, r.right() - 2, r.y + 1));
}

#[test]
fn without_a_button_there_is_none() {
    assert_eq!(toast_button(AREA, copied()), Rect::default());
}

#[test]
fn sits_in_the_bottom_right_corner() {
    let r = toast_area(AREA, copied());
    assert_eq!((r.right(), r.bottom(), r.height), (W - 1, H - 1, 3));
}

#[test]
fn fits_a_small_screen() {
    let small = Rect::new(0, 0, 10, 2);
    assert_eq!(toast_area(small, copied()), small);
}

#[test]
fn renders_over_the_pane() {
    let snap = screen(b"$ echo hello\r\nhello\r\n$ ");
    let v = View { tab: Some(single(snap)), toast: Some(copied()), ..view(&["~"]) };
    insta::assert_snapshot!(render(&v).backend());
}

#[rstest]
#[case::agent_waiting(ToastIcon::Agent(Status::Waiting), "!", WAITING_COLOR)]
#[case::agent_done(ToastIcon::Agent(Status::Done), "✓", Color::Green)]
#[case::bug(ToastIcon::Bug, "✗", Color::Red)]
#[case::restart(ToastIcon::Restart, "↻", Color::Cyan)]
fn its_icon_and_border_say_what_it_is_about(#[case] icon: ToastIcon, #[case] glyph: &str, #[case] colour: Color) {
    let toast = Toast { message: "something happened", icon, button: None };
    let r = toast_area(AREA, toast);
    let v = View { toast: Some(toast), ..view(&["~"]) };
    let t = render(&v);
    let (icon, border) = (&t.backend().buffer()[(r.x + 2, r.y + 1)], &t.backend().buffer()[(r.x, r.y)]);
    assert_eq!((icon.symbol(), icon.fg, border.fg), (glyph, colour, colour));
}
