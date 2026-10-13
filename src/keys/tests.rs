use rstest::rstest;

use super::*;

const NONE: KeyModifiers = KeyModifiers::NONE;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const ALT: KeyModifiers = KeyModifiers::ALT;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;

const NO_KITTY: KittyKeyFlags = KittyKeyFlags::DISABLED;
const DISAMBIGUATE: KittyKeyFlags = KittyKeyFlags::DISAMBIGUATE;

fn encode_with(code: KeyCode, mods: KeyModifiers, kitty: KittyKeyFlags, app_cursor: bool) -> Vec<u8> {
    let mut encoder = Encoder::new().expect("key encoder");
    encoder.set_kitty_flags(kitty).set_cursor_key_application(app_cursor);
    encode(KeyEvent::new(code, mods), &mut encoder, kitty, app_cursor)
}

fn encode_normal(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
    encode_with(code, mods, NO_KITTY, false)
}

fn encode_app_cursor(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
    encode_with(code, mods, NO_KITTY, true)
}

fn encode_kitty(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
    encode_with(code, mods, DISAMBIGUATE, false)
}

#[rstest]
#[case::lowercase('a', NONE, b"a")]
#[case::uppercase('A', SHIFT, b"A")]
#[case::multibyte('ñ', NONE, "ñ".as_bytes())]
fn plain_chars_are_sent_as_utf8(#[case] c: char, #[case] mods: KeyModifiers, #[case] expected: &[u8]) {
    assert_eq!(encode_normal(KeyCode::Char(c), mods), expected);
}

#[rstest]
#[case::ctrl_a('a', 0x01)]
#[case::ctrl_c('c', 0x03)]
#[case::ctrl_shift_c('C', 0x03)]
#[case::ctrl_z('z', 0x1a)]
#[case::ctrl_space(' ', 0x00)]
#[case::ctrl_bracket('[', 0x1b)]
#[case::ctrl_underscore('_', 0x1f)]
fn ctrl_chars_become_control_codes(#[case] c: char, #[case] expected: u8) {
    assert_eq!(encode_normal(KeyCode::Char(c), CTRL), [expected]);
}

#[rstest]
#[case::alt_char(KeyCode::Char('x'), ALT, b"\x1bx")]
#[case::alt_ctrl_char(KeyCode::Char('c'), CTRL | ALT, b"\x1b\x03")]
#[case::alt_backspace(KeyCode::Backspace, ALT, b"\x1b\x7f")]
fn alt_prefixes_escape(#[case] code: KeyCode, #[case] mods: KeyModifiers, #[case] expected: &[u8]) {
    assert_eq!(encode_normal(code, mods), expected);
}

#[rstest]
#[case::enter(KeyCode::Enter, b"\r")]
#[case::tab(KeyCode::Tab, b"\t")]
#[case::backspace(KeyCode::Backspace, b"\x7f")]
#[case::escape(KeyCode::Esc, b"\x1b")]
#[case::insert(KeyCode::Insert, b"\x1b[2~")]
#[case::delete(KeyCode::Delete, b"\x1b[3~")]
#[case::page_up(KeyCode::PageUp, b"\x1b[5~")]
#[case::page_down(KeyCode::PageDown, b"\x1b[6~")]
fn special_keys_send_their_sequence(#[case] code: KeyCode, #[case] expected: &[u8]) {
    assert_eq!(encode_normal(code, NONE), expected);
}

#[test]
fn shift_tab_sends_back_tab() {
    assert_eq!(encode_normal(KeyCode::BackTab, SHIFT), b"\x1b[Z");
}

#[rstest]
#[case::up(KeyCode::Up, b"\x1b[A")]
#[case::down(KeyCode::Down, b"\x1b[B")]
#[case::right(KeyCode::Right, b"\x1b[C")]
#[case::left(KeyCode::Left, b"\x1b[D")]
#[case::home(KeyCode::Home, b"\x1b[H")]
#[case::end(KeyCode::End, b"\x1b[F")]
fn cursor_keys_use_csi_in_normal_mode(#[case] code: KeyCode, #[case] expected: &[u8]) {
    assert_eq!(encode_normal(code, NONE), expected);
}

#[rstest]
#[case::up(KeyCode::Up, b"\x1bOA")]
#[case::home(KeyCode::Home, b"\x1bOH")]
fn cursor_keys_use_ss3_in_application_mode(#[case] code: KeyCode, #[case] expected: &[u8]) {
    assert_eq!(encode_app_cursor(code, NONE), expected);
}

#[rstest]
#[case::shift_up(KeyCode::Up, SHIFT, b"\x1b[1;2A")]
#[case::alt_right(KeyCode::Right, ALT, b"\x1b[1;3C")]
#[case::ctrl_left(KeyCode::Left, CTRL, b"\x1b[1;5D")]
#[case::ctrl_delete(KeyCode::Delete, CTRL, b"\x1b[3;5~")]
fn modified_keys_carry_xterm_modifier_param(
    #[case] code: KeyCode,
    #[case] mods: KeyModifiers,
    #[case] expected: &[u8],
) {
    assert_eq!(encode_normal(code, mods), expected);
}

#[test]
fn modified_cursor_keys_ignore_application_mode() {
    assert_eq!(encode_app_cursor(KeyCode::Up, CTRL), b"\x1b[1;5A");
}

#[rstest]
#[case::f1(1, b"\x1bOP")]
#[case::f4(4, b"\x1bOS")]
#[case::f5(5, b"\x1b[15~")]
#[case::f6(6, b"\x1b[17~")]
#[case::f10(10, b"\x1b[21~")]
#[case::f11(11, b"\x1b[23~")]
#[case::f12(12, b"\x1b[24~")]
#[case::f13_unsupported(13, b"")]
fn function_keys_send_vt220_sequences(#[case] n: u8, #[case] expected: &[u8]) {
    assert_eq!(encode_normal(KeyCode::F(n), NONE), expected);
}

#[test]
fn modified_f1_uses_csi_with_modifier() {
    assert_eq!(encode_normal(KeyCode::F(1), SHIFT), b"\x1b[1;2P");
}

#[rstest]
#[case::plain_char(KeyCode::Char('a'), NONE, b"a")]
#[case::shifted_char(KeyCode::Char('A'), SHIFT, b"A")]
#[case::shifted_symbol(KeyCode::Char('!'), SHIFT, b"!")]
#[case::ctrl_shift_char(KeyCode::Char('A'), CTRL | SHIFT, b"\x1b[97;6u")]
#[case::ctrl_digit(KeyCode::Char('1'), CTRL, b"\x1b[49;5u")]
#[case::non_ascii_char(KeyCode::Char('ñ'), NONE, "ñ".as_bytes())]
#[case::ctrl_char(KeyCode::Char('a'), CTRL, b"\x1b[97;5u")]
#[case::alt_char(KeyCode::Char('x'), ALT, b"\x1b[120;3u")]
#[case::escape(KeyCode::Esc, NONE, b"\x1b[27u")]
#[case::enter(KeyCode::Enter, NONE, b"\r")]
#[case::shift_enter(KeyCode::Enter, SHIFT, b"\x1b[13;2u")]
#[case::ctrl_tab(KeyCode::Tab, CTRL, b"\x1b[9;5u")]
#[case::up(KeyCode::Up, NONE, b"\x1b[A")]
fn kitty_disambiguate_follows_the_spec(#[case] code: KeyCode, #[case] mods: KeyModifiers, #[case] expected: &[u8]) {
    assert_eq!(encode_kitty(code, mods), expected);
}

#[rstest]
#[case::shift_enter(KeyCode::Enter, SHIFT, b"\r")]
#[case::ctrl_tab(KeyCode::Tab, CTRL, b"\t")]
fn without_kitty_extended_keys_stay_legacy(#[case] code: KeyCode, #[case] mods: KeyModifiers, #[case] expected: &[u8]) {
    assert_eq!(encode_normal(code, mods), expected);
}

mod named {
    use super::*;

    #[rstest]
    #[case::enter("enter", KeyCode::Enter, NONE)]
    #[case::any_case("Esc", KeyCode::Esc, NONE)]
    #[case::ctrl_c("ctrl+c", KeyCode::Char('c'), CTRL)]
    #[case::two_modifiers("ctrl+alt+x", KeyCode::Char('x'), CTRL.union(ALT))]
    #[case::shift_tab("shift+tab", KeyCode::BackTab, SHIFT)]
    #[case::a_capital("A", KeyCode::Char('A'), SHIFT)]
    #[case::a_shifted_letter("shift+a", KeyCode::Char('A'), SHIFT)]
    #[case::a_symbol("?", KeyCode::Char('?'), NONE)]
    #[case::a_plus("ctrl++", KeyCode::Char('+'), CTRL)]
    #[case::space("space", KeyCode::Char(' '), NONE)]
    #[case::an_arrow("up", KeyCode::Up, NONE)]
    #[case::a_function_key("f12", KeyCode::F(12), NONE)]
    fn reads_a_key(#[case] name: &str, #[case] code: KeyCode, #[case] mods: KeyModifiers) {
        assert_eq!(named(name), Some(KeyEvent::new(code, mods)));
    }

    #[rstest]
    #[case::a_word("hello")]
    #[case::a_modifier_alone("ctrl+")]
    #[case::a_missing_function_key("f13")]
    #[case::nothing("")]
    fn refuses_what_is_not_a_key(#[case] name: &str) {
        assert_eq!(named(name), None);
    }

    #[test]
    fn ctrl_c_reaches_the_program_as_an_interrupt() {
        let key = named("ctrl+c").expect("a key");
        let mut encoder = Encoder::new().expect("key encoder");

        assert_eq!(encode(key, &mut encoder, NO_KITTY, false), [0x03]);
    }
}
