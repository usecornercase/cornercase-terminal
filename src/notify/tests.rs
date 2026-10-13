use rstest::rstest;

use super::*;

mod encode {
    use super::*;

    #[rstest]
    #[case::osc777(Channel::Osc777, "\x1b]777;notify;cornercase;claude finished in shop › main\x07")]
    #[case::osc9(Channel::Osc9, "\x1b]9;claude finished in shop › main\x07")]
    #[case::osc99(Channel::Osc99, "\x1b]99;o=unfocused;claude finished in shop › main\x1b\\")]
    #[case::bell(Channel::Bell, "\x07")]
    fn asks_the_terminal_for_a_notification(#[case] channel: Channel, #[case] expected: &str) {
        assert_eq!(String::from_utf8(channel.encode("claude finished in shop › main")).expect("utf-8"), expected);
    }

    #[test]
    fn strips_control_characters_from_the_text() {
        let bytes = Channel::Osc9.encode("claude finished in evil\x1b]0pwned\x07\u{9c} › main\r\n");

        assert_eq!(bytes, "\x1b]9;claude finished in evil]0pwned › main\x07".as_bytes());
    }

    #[test]
    fn keeps_semicolons_from_ending_the_text() {
        let bytes = Channel::Osc777.encode("claude finished in a;b › main");

        assert_eq!(bytes, "\x1b]777;notify;cornercase;claude finished in a,b › main\x07".as_bytes());
    }
}

mod setting {
    use super::*;

    #[rstest]
    #[case::auto(AUTO, None)]
    #[case::forced("osc9", Some(Channel::Osc9))]
    #[case::any_case(" Bell ", Some(Channel::Bell))]
    #[case::unknown_means_auto("growl", None)]
    fn picks_the_channel(#[case] setting: &str, #[case] expected: Option<Channel>) {
        assert_eq!(Notification::new("hi", setting).expect("on").channel, expected);
    }

    #[test]
    fn off_sends_nothing() {
        assert_eq!(Notification::new("hi", OFF), None);
    }

    #[test]
    fn auto_uses_the_channel_of_each_terminal() {
        let notification = Notification::new("hi", AUTO).expect("on");

        assert_eq!(
            (notification.encode(Channel::Osc9), notification.encode(Channel::Bell)),
            (b"\x1b]9;hi\x07".to_vec(), b"\x07".to_vec())
        );
    }

    #[test]
    fn a_forced_channel_wins_over_the_terminal() {
        let notification = Notification::new("hi", "osc777").expect("on");

        assert_eq!(notification.encode(Channel::Bell), b"\x1b]777;notify;cornercase;hi\x07");
    }
}

mod detect {
    use super::*;

    fn with(version: Option<&str>, vars: &[(&str, &str)]) -> Channel {
        detect(version, |name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| (*v).to_string()))
    }

    #[rstest]
    #[case::ghostty(Some("ghostty 1.3.1"), &[], Channel::Osc777)]
    #[case::wezterm(Some("WezTerm 20240203-110809-5046fc22"), &[], Channel::Osc777)]
    #[case::foot(Some("foot(1.28.0)"), &[], Channel::Osc777)]
    #[case::konsole(Some("Konsole 23.08.5"), &[], Channel::Osc777)]
    #[case::warp(Some("Warp(v0.2026.08.26.17.59.stable_01)"), &[], Channel::Osc777)]
    #[case::rio(Some("Rio 0.5.27"), &[], Channel::Osc777)]
    #[case::iterm(Some("iTerm2 3.7.2"), &[], Channel::Osc9)]
    #[case::kitty(Some("kitty(0.48.2)"), &[], Channel::Osc99)]
    #[case::contour(Some("contour 0.7.1-master-9f0850ce"), &[], Channel::Osc99)]
    #[case::vscode(Some("xterm.js(6.1.0-beta.302)"), &[], Channel::Osc99)]
    #[case::vte(Some("VTE(8400)"), &[], Channel::Bell)]
    #[case::tmux(Some("tmux 3.4"), &[], Channel::Bell)]
    #[case::the_terminal_knows_best(Some("XTerm(411)"), &[("TERM_PROGRAM", "ghostty")], Channel::Bell)]
    #[case::term_program(None, &[("TERM_PROGRAM", "ghostty")], Channel::Osc777)]
    #[case::iterm_app(None, &[("TERM_PROGRAM", "iTerm.app")], Channel::Osc9)]
    #[case::iterm_over_ssh(None, &[("LC_TERMINAL", "iTerm2")], Channel::Osc9)]
    #[case::vscode_without_a_name(None, &[("TERM_PROGRAM", "vscode")], Channel::Osc99)]
    #[case::kitty_term(None, &[("TERM", "xterm-kitty")], Channel::Osc99)]
    #[case::foot_term(None, &[("TERM", "foot-extra")], Channel::Osc777)]
    #[case::kitty_window(None, &[("KITTY_WINDOW_ID", "1")], Channel::Osc99)]
    #[case::konsole_version(None, &[("KONSOLE_VERSION", "230805")], Channel::Osc777)]
    #[case::screen_term(None, &[("TERM", "screen.xterm-256color")], Channel::Bell)]
    #[case::apple_terminal(None, &[("TERM_PROGRAM", "Apple_Terminal")], Channel::Bell)]
    #[case::inside_tmux(Some("ghostty 1.3.1"), &[("TMUX", "/tmp/tmux-1000/default,1,0")], Channel::Bell)]
    #[case::inside_zellij(None, &[("ZELLIJ", "0"), ("TERM_PROGRAM", "ghostty")], Channel::Bell)]
    #[case::inside_screen(None, &[("STY", "1234.pts-0.host"), ("TERM_PROGRAM", "ghostty")], Channel::Bell)]
    #[case::nothing_known(None, &[("TERM", "xterm-256color")], Channel::Bell)]
    fn picks_what_the_terminal_understands(
        #[case] version: Option<&str>,
        #[case] vars: &[(&str, &str)],
        #[case] expected: Channel,
    ) {
        assert_eq!(with(version, vars), expected);
    }
}
