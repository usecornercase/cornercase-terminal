use rstest::rstest;

use super::*;

fn no_env(_: &str) -> Option<String> {
    None
}

mod detecting {
    use super::*;

    #[rstest]
    #[case::ghostty("ghostty 1.3.1", Some(Terminal::Ghostty))]
    #[case::ghostty_first_with_it("ghostty 1.0.0", Some(Terminal::Ghostty))]
    #[case::ghostty_before_it("ghostty 0.9.0", None)]
    #[case::kitty("kitty(0.45.0)", Some(Terminal::Kitty))]
    #[case::kitty_first_with_it("kitty(0.31.0)", Some(Terminal::Kitty))]
    #[case::kitty_before_it("kitty(0.30.1)", None)]
    #[case::foot("foot(1.25.0)", Some(Terminal::Foot))]
    #[case::foot_before_it("foot(1.11.0)", None)]
    #[case::wezterm("WezTerm 20240203-110809-5046fc22", None)]
    #[case::iterm2("iTerm2 3.6.6", None)]
    #[case::konsole("Konsole 25.12.0", None)]
    #[case::xterm("XTerm(407)", None)]
    #[case::tmux("tmux 3.6", None)]
    fn only_terminals_known_to_honour_osc_22(#[case] version: &str, #[case] expected: Option<Terminal>) {
        assert_eq!(detect(Some(version), no_env), expected);
    }

    #[test]
    fn nothing_without_an_xtversion_reply() {
        let env = |name: &str| (name == "TERM_PROGRAM").then(|| "ghostty".to_string());

        assert_eq!(detect(None, env), None);
    }

    #[rstest]
    #[case::tmux("TMUX", "/tmp/tmux-501/default,1,0")]
    #[case::screen("STY", "1234.pts-0.host")]
    #[case::zellij("ZELLIJ", "0")]
    fn nothing_inside_a_multiplexer(#[case] var: &str, #[case] value: &str) {
        let env = |name: &str| (name == var).then(|| value.to_string());

        assert_eq!(detect(Some("ghostty 1.3.1"), env), None);
    }
}

mod encode {
    use super::*;

    #[rstest]
    #[case::columns(Terminal::Kitty, Shape::Columns, "\x1b]22;ew-resize\x1b\\")]
    #[case::rows(Terminal::Foot, Shape::Rows, "\x1b]22;ns-resize\x1b\\")]
    #[case::kitty_default(Terminal::Kitty, Shape::Default, "\x1b]22;\x1b\\")]
    #[case::foot_default(Terminal::Foot, Shape::Default, "\x1b]22;\x1b\\")]
    #[case::ghostty_default(Terminal::Ghostty, Shape::Default, "\x1b]22;text\x1b\\")]
    fn asks_for_a_css_pointer_name(#[case] terminal: Terminal, #[case] shape: Shape, #[case] expected: &str) {
        assert_eq!(String::from_utf8(terminal.encode(shape)).expect("utf-8"), expected);
    }
}
