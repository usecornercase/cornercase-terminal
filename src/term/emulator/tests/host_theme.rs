use libghostty_vt::style::RgbColor;

use super::*;

const DARK: RgbColor = RgbColor { r: 0x12, g: 0x34, b: 0x56 };
const LIGHT: RgbColor = RgbColor { r: 0xfd, g: 0xf6, b: 0xe3 };
const RED: RgbColor = RgbColor { r: 0xcc, g: 0x11, b: 0x22 };

fn themed_replies_to(theme: &HostTheme, output: &[u8]) -> String {
    let (mut emu, replies) = themed_emulator(theme);
    emu.feed(output);
    String::from_utf8_lossy(&replies.borrow()).into_owned()
}

fn with_background(background: RgbColor) -> HostTheme {
    HostTheme { background: Some(background), ..HostTheme::default() }
}

#[test]
fn background_query_gets_the_host_background() {
    assert!(themed_replies_to(&with_background(DARK), b"\x1b]11;?\x07").contains("rgb:1212/3434/5656"));
}

#[test]
fn foreground_query_gets_the_host_foreground() {
    let theme = HostTheme { foreground: Some(LIGHT), ..HostTheme::default() };
    assert!(themed_replies_to(&theme, b"\x1b]10;?\x07").contains("rgb:fdfd/f6f6/e3e3"));
}

#[test]
fn palette_query_gets_the_host_palette() {
    let mut theme = HostTheme::default();
    theme.palette[1] = Some(RED);
    assert!(themed_replies_to(&theme, b"\x1b]4;1;?\x07").contains("rgb:cccc/1111/2222"));
}

#[test]
fn background_query_is_unanswered_without_host_colors() {
    assert_eq!(replies_to(b"\x1b]11;?\x07"), "");
}

#[rstest]
#[case::dark(DARK, "\x1b[?997;1n")]
#[case::light(LIGHT, "\x1b[?997;2n")]
fn color_scheme_query_follows_the_host_background(#[case] background: RgbColor, #[case] expected: &str) {
    assert_eq!(themed_replies_to(&with_background(background), b"\x1b[?996n"), expected);
}

#[test]
fn a_theme_set_later_answers_the_queries() {
    let (mut emu, replies) = emulator();
    let mut theme = with_background(DARK);
    theme.foreground = Some(LIGHT);
    theme.palette[1] = Some(RED);

    emu.set_theme(&theme).expect("theme");
    emu.feed(b"\x1b]11;?\x07\x1b]10;?\x07\x1b]4;1;?\x07\x1b[?996n");

    let answered = String::from_utf8_lossy(&replies.borrow()).into_owned();
    assert!(answered.contains("rgb:1212/3434/5656"), "{answered:?}");
    assert!(answered.contains("rgb:fdfd/f6f6/e3e3"), "{answered:?}");
    assert!(answered.contains("rgb:cccc/1111/2222"), "{answered:?}");
    assert!(answered.ends_with("\x1b[?997;1n"), "{answered:?}");
}

#[test]
fn a_theme_set_again_replaces_the_colour_scheme() {
    let (mut emu, replies) = themed_emulator(&with_background(DARK));

    emu.set_theme(&with_background(LIGHT)).expect("theme");
    emu.feed(b"\x1b[?996n");

    assert_eq!(String::from_utf8_lossy(&replies.borrow()), "\x1b[?997;2n");
}

fn dark_with_reports() -> (Emulator, Rc<RefCell<Vec<u8>>>) {
    let (mut emu, replies) = themed_emulator(&with_background(DARK));
    emu.feed(b"\x1b[?2031h");
    (emu, replies)
}

#[test]
fn a_program_that_turned_on_scheme_reports_is_told_when_light_and_dark_switch() {
    let (mut emu, replies) = dark_with_reports();
    emu.set_theme(&with_background(LIGHT)).expect("theme");
    assert_eq!(String::from_utf8_lossy(&replies.borrow()), "\x1b[?997;2n");
}

#[test]
fn a_program_is_told_the_first_scheme_a_later_theme_gives() {
    let (mut emu, replies) = emulator();
    emu.feed(b"\x1b[?2031h");
    emu.set_theme(&with_background(DARK)).expect("theme");
    assert_eq!(String::from_utf8_lossy(&replies.borrow()), "\x1b[?997;1n");
}

#[test]
fn a_program_that_did_not_ask_is_not_told() {
    let (mut emu, replies) = themed_emulator(&with_background(DARK));
    emu.set_theme(&with_background(LIGHT)).expect("theme");
    assert_eq!(String::from_utf8_lossy(&replies.borrow()), "");
}

#[test]
fn the_same_scheme_again_tells_nothing() {
    let (mut emu, replies) = dark_with_reports();
    emu.set_theme(&with_background(RED)).expect("theme");
    assert_eq!(String::from_utf8_lossy(&replies.borrow()), "");
}

#[test]
fn a_theme_set_again_drops_what_the_new_one_leaves_out() {
    let mut theme = with_background(DARK);
    theme.foreground = Some(LIGHT);
    theme.palette[1] = Some(RED);
    let (mut emu, replies) = themed_emulator(&theme);
    emu.set_theme(&HostTheme::default()).expect("theme");
    emu.feed(b"\x1b]10;?\x07\x1b]11;?\x07\x1b]4;1;?\x07\x1b[?996n");
    let answered = String::from_utf8_lossy(&replies.borrow()).into_owned();
    assert!(!answered.contains("rgb:cccc/1111/2222"), "{answered:?}");
    assert!(!answered.contains("rgb:1212/3434/5656"), "{answered:?}");
    assert!(!answered.contains("997"), "{answered:?}");
}

#[test]
fn palette_colors_still_render_as_indices() {
    let mut theme = HostTheme::default();
    theme.palette[1] = Some(RED);
    let (mut emu, _) = themed_emulator(&theme);
    emu.feed(b"\x1b[31mx");
    assert_eq!(emu.snapshot().expect("snapshot").rows[0][0].style.fg, Some(Color::Indexed(1)));
}
