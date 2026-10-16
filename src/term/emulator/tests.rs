use std::cell::RefCell;
use std::rc::Rc;

use rstest::rstest;

use super::*;

fn emulator() -> (Emulator, Rc<RefCell<Vec<u8>>>) {
    themed_emulator(&HostTheme::default())
}

fn themed_emulator(theme: &HostTheme) -> (Emulator, Rc<RefCell<Vec<u8>>>) {
    let replies = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&replies);
    let reply = Box::new(move |bytes: &[u8]| sink.borrow_mut().extend_from_slice(bytes));
    (Emulator::new(24, 80, 0, theme, reply).expect("emulator"), replies)
}

fn snapshot_of(output: &[u8]) -> Snapshot {
    let (mut emu, _) = emulator();
    emu.feed(output);
    emu.snapshot().expect("snapshot")
}

fn replies_to(output: &[u8]) -> String {
    let (mut emu, replies) = emulator();
    emu.feed(output);
    String::from_utf8_lossy(&replies.borrow()).into_owned()
}

fn style_of(output: &[u8]) -> Style {
    snapshot_of(output).rows[0][0].style
}

mod program_status {
    use super::*;
    use crate::agents::activity::Activity;
    use crate::term::status::REPLY;

    #[test]
    fn every_split_of_a_sequence_is_accepted_without_drawing_it() {
        let bytes = b"before\x1b]7501;state=blocked:kind=question\x1b\\after";
        for at in 0..=bytes.len() {
            let (mut emu, _) = emulator();
            emu.feed(&bytes[..at]);
            emu.feed(&bytes[at..]);
            assert_eq!(emu.program_activity(), Some(Activity::Waiting), "split at {at}");
            assert!(emu.snapshot().expect("snapshot").contents().starts_with("beforeafter"));
        }
    }

    #[test]
    fn query_replies_stay_in_order_with_ghostty_replies() {
        assert_eq!(replies_to(b"\x1b]7501;?\x1b\\\x1b[5n"), format!("{}\x1b[0n", String::from_utf8_lossy(REPLY)));
        assert_eq!(replies_to(b"\x1b[5n\x1b]7501;?\x07"), format!("\x1b[0n{}", String::from_utf8_lossy(REPLY)));
    }

    #[test]
    fn a_query_split_byte_by_byte_gets_one_reply() {
        let (mut emu, replies) = emulator();
        for byte in b"\x1b]7501;?\x1b\\" {
            emu.feed(&[*byte]);
        }
        assert_eq!(replies.borrow().as_slice(), REPLY);
        assert_eq!(emu.program_activity(), None);
    }
}

mod snapshot {
    use super::*;

    #[test]
    fn has_the_text() {
        assert!(snapshot_of(b"hello\r\nworld").contents().starts_with("hello\nworld"));
    }

    #[test]
    fn has_the_cursor() {
        assert_eq!(snapshot_of(b"hello\r\nworld").cursor, Some(Position::new(5, 1)));
    }

    #[test]
    fn never_holds_control_characters() {
        let snap = snapshot_of(b"x\x7fy");
        let controls: Vec<&str> =
            snap.rows.iter().flatten().map(|c| c.symbol.as_str()).filter(|s| s.contains(char::is_control)).collect();
        assert_eq!(controls, Vec::<&str>::new());
    }

    #[test]
    fn hides_the_cursor_when_asked() {
        assert_eq!(snapshot_of(b"\x1b[?25l").cursor, None);
    }

    #[test]
    fn has_one_row_per_line_of_the_terminal() {
        assert_eq!(snapshot_of(b"").rows.len(), 24);
    }

    #[test]
    fn rewraps_lines_on_resize() {
        let (mut emu, _) = emulator();
        emu.feed(&[b'x'; 30]);
        emu.resize(24, 20).expect("resize");
        assert!(emu.snapshot().expect("snapshot").contents().starts_with(&format!(
            "{}\n{}",
            "x".repeat(20),
            "x".repeat(10)
        )));
    }
}

mod clipboard {
    use super::*;

    fn copied_by(output: &[u8]) -> Vec<String> {
        let (mut emu, _) = emulator();
        emu.feed(output);
        emu.take_copied()
    }

    #[test]
    fn an_osc52_write_is_handed_over() {
        assert_eq!(copied_by(b"\x1b]52;c;aGVsbG8=\x07"), ["hello"]);
    }

    #[rstest]
    #[case::a_read_request(b"\x1b]52;c;?\x07")]
    #[case::the_primary_selection(b"\x1b]52;p;aGVsbG8=\x07")]
    #[case::an_empty_write(b"\x1b]52;c;\x07")]
    #[case::plain_output(b"hello")]
    fn hands_over_nothing_for(#[case] output: &[u8]) {
        assert_eq!(copied_by(output), Vec::<String>::new());
    }

    #[test]
    fn taking_empties_the_queue() {
        let (mut emu, _) = emulator();
        emu.feed(b"\x1b]52;c;aGVsbG8=\x07");
        emu.take_copied();
        assert_eq!(emu.take_copied(), Vec::<String>::new());
    }
}

mod title {
    use super::*;

    fn title_after(output: &[u8]) -> String {
        let (mut emu, _) = emulator();
        emu.feed(output);
        emu.title()
    }

    #[rstest]
    #[case::osc_0("\x1b]0;✳ Claude Code\x07", "✳ Claude Code")]
    #[case::osc_2("\x1b]2;◐ fix the login\x1b\\", "◐ fix the login")]
    #[case::the_latest("\x1b]0;one\x07\x1b]0;two\x07", "two")]
    #[case::none_set("hello", "")]
    fn is_what_the_program_set(#[case] output: &str, #[case] expected: &str) {
        assert_eq!(title_after(output.as_bytes()), expected);
    }
}

mod selection {
    use super::*;

    fn small(rows: u16, cols: u16) -> Emulator {
        Emulator::new(rows, cols, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator")
    }

    fn select(emu: &mut Emulator, from: (u16, u16), to: (u16, u16)) {
        emu.start_selection(Position::new(from.0, from.1)).expect("start");
        emu.extend_selection(Position::new(to.0, to.1)).expect("extend");
    }

    fn copied(output: &[u8], from: (u16, u16), to: (u16, u16)) -> Option<String> {
        let (mut emu, _) = emulator();
        emu.feed(output);
        select(&mut emu, from, to);
        emu.finish_selection().expect("finish")
    }

    #[rstest]
    #[case::forwards(b"hello world", (0, 0), (4, 0), "hello")]
    #[case::backwards(b"hello world", (10, 0), (6, 0), "world")]
    #[case::across_lines(b"ab\r\ncd", (0, 0), (1, 1), "ab\ncd")]
    #[case::without_trailing_spaces(b"ab   \r\ncd", (0, 0), (9, 0), "ab")]
    fn copies_the_text_between_both_points(
        #[case] output: &[u8],
        #[case] from: (u16, u16),
        #[case] to: (u16, u16),
        #[case] expected: &str,
    ) {
        assert_eq!(copied(output, from, to).as_deref(), Some(expected));
    }

    #[test]
    fn joins_a_line_the_terminal_wrapped() {
        let mut emu = small(4, 10);
        emu.feed(b"0123456789abcde");
        select(&mut emu, (0, 0), (4, 1));
        assert_eq!(emu.finish_selection().expect("finish").as_deref(), Some("0123456789abcde"));
    }

    #[test]
    fn keeps_its_start_on_the_text_when_the_screen_scrolls() {
        let mut emu = small(3, 10);
        emu.feed(b"a\r\nb\r\nc");
        emu.start_selection(Position::new(0, 2)).expect("start");
        emu.feed(b"\r\nd");
        emu.extend_selection(Position::new(0, 2)).expect("extend");
        assert_eq!(emu.finish_selection().expect("finish").as_deref(), Some("c\nd"));
    }

    #[test]
    fn a_click_without_dragging_copies_nothing() {
        let (mut emu, _) = emulator();
        emu.feed(b"hello");
        emu.start_selection(Position::new(1, 0)).expect("start");
        assert_eq!(emu.finish_selection().expect("finish"), None);
    }

    #[test]
    fn marks_the_selected_cells_reversed() {
        let (mut emu, _) = emulator();
        emu.feed(b"hello world");
        select(&mut emu, (0, 0), (4, 0));
        let row = &emu.snapshot().expect("snapshot").rows[0];
        assert!(row[4].style.add_modifier.contains(Modifier::REVERSED));
        assert!(!row[5].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn reversed_text_turns_back_to_normal_when_selected() {
        let (mut emu, _) = emulator();
        emu.feed(b"\x1b[7mhi");
        select(&mut emu, (0, 0), (1, 0));
        let style = emu.snapshot().expect("snapshot").rows[0][0].style;
        assert!(!style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn finishing_clears_the_highlight() {
        let (mut emu, _) = emulator();
        emu.feed(b"hello");
        select(&mut emu, (0, 0), (4, 0));
        emu.finish_selection().expect("finish");
        let row = &emu.snapshot().expect("snapshot").rows[0];
        assert!(row.iter().all(|c| !c.style.add_modifier.contains(Modifier::REVERSED)));
    }
}

mod text {
    use super::*;

    fn after(rows: u16, cols: u16, output: &[u8]) -> Emulator {
        let mut emu = Emulator::new(rows, cols, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
        emu.feed(output);
        emu
    }

    #[test]
    fn the_screen_joins_lines_the_terminal_wrapped() {
        let emu = after(4, 10, b"0123456789abcde\r\nnext");

        assert_eq!(emu.screen_text().expect("text"), "0123456789abcde\nnext");
    }

    #[test]
    fn empty_lines_at_the_end_are_left_out() {
        let emu = after(6, 20, b"hello   \r\n\r\nworld\r\n\r\n");

        assert_eq!(emu.screen_text().expect("text"), "hello\n\nworld");
    }

    #[test]
    fn the_screen_is_only_what_shows() {
        let emu = after(3, 10, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");

        assert_eq!(emu.screen_text().expect("text"), "three\nfour\nfive");
    }

    #[test]
    fn the_last_lines_reach_into_the_scrollback() {
        let emu = after(3, 10, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");

        assert_eq!(emu.last_lines(4).expect("text"), "two\nthree\nfour\nfive");
    }

    #[test]
    fn more_lines_than_there_are_give_them_all() {
        let emu = after(3, 10, b"one\r\ntwo");

        assert_eq!(emu.last_lines(50).expect("text"), "one\ntwo");
    }
}

mod style {
    use super::*;

    #[rstest]
    #[case::palette_stays_indexed(b"\x1b[31mx", Style::default().fg(Color::Indexed(1)).bg(Color::Reset))]
    #[case::extended_palette(b"\x1b[38;5;208mx", Style::default().fg(Color::Indexed(208)).bg(Color::Reset))]
    #[case::rgb(b"\x1b[38;2;10;200;30mx", Style::default().fg(Color::Rgb(10, 200, 30)).bg(Color::Reset))]
    #[case::background(b"\x1b[44mx", Style::default().fg(Color::Reset).bg(Color::Indexed(4)))]
    #[case::bold(b"\x1b[1mx", Style::default().fg(Color::Reset).bg(Color::Reset).add_modifier(Modifier::BOLD))]
    #[case::plain(b"x", Style::default())]
    fn maps_to_ratatui(#[case] output: &[u8], #[case] expected: Style) {
        assert_eq!(style_of(output), expected);
    }
}

mod replies {
    use super::*;

    #[rstest]
    #[case::cursor_position(b"\x1b[3;5H\x1b[6n", "\x1b[3;5R")]
    #[case::status(b"\x1b[5n", "\x1b[0n")]
    #[case::plain_output(b"hello", "")]
    fn answers_terminal_queries(#[case] output: &[u8], #[case] expected: &str) {
        assert_eq!(replies_to(output), expected);
    }

    #[test]
    fn answers_device_attributes() {
        assert!(replies_to(b"\x1b[c").starts_with("\x1b[?"));
    }
}

mod host_theme {
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

    #[test]
    fn palette_colors_still_render_as_indices() {
        let mut theme = HostTheme::default();
        theme.palette[1] = Some(RED);
        let (mut emu, _) = themed_emulator(&theme);
        emu.feed(b"\x1b[31mx");
        assert_eq!(emu.snapshot().expect("snapshot").rows[0][0].style.fg, Some(Color::Indexed(1)));
    }
}

mod modes {
    use super::*;

    fn after(output: &[u8]) -> Emulator {
        let (mut emu, _) = emulator();
        emu.feed(output);
        emu
    }

    #[rstest]
    #[case::none(b"", MouseMode::None)]
    #[case::x10(b"\x1b[?9h", MouseMode::Press)]
    #[case::normal(b"\x1b[?1000h", MouseMode::PressRelease)]
    #[case::button(b"\x1b[?1002h", MouseMode::ButtonMotion)]
    #[case::any(b"\x1b[?1003h", MouseMode::AnyMotion)]
    fn mouse_mode_follows_the_program(#[case] output: &[u8], #[case] expected: MouseMode) {
        assert_eq!(after(output).mouse_mode(), expected);
    }

    #[rstest]
    #[case::default(b"", MouseEncoding::Default)]
    #[case::utf8(b"\x1b[?1005h", MouseEncoding::Utf8)]
    #[case::sgr(b"\x1b[?1006h", MouseEncoding::Sgr)]
    fn mouse_encoding_follows_the_program(#[case] output: &[u8], #[case] expected: MouseEncoding) {
        assert_eq!(after(output).mouse_encoding(), expected);
    }

    #[rstest]
    #[case::off(b"", false)]
    #[case::on(b"\x1b[?1h", true)]
    fn application_cursor_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
        assert_eq!(after(output).application_cursor(), expected);
    }

    #[rstest]
    #[case::off(b"", false)]
    #[case::on(b"\x1b[?2004h", true)]
    fn bracketed_paste_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
        assert_eq!(after(output).bracketed_paste(), expected);
    }
}

mod scrollback {
    use super::*;

    fn numbered(lines: usize) -> Emulator {
        let mut emu = Emulator::new(4, 20, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
        let text: Vec<String> = (1..=lines).map(|i| format!("line-{i}")).collect();
        emu.feed(text.join("\r\n").as_bytes());
        emu
    }

    fn shown(emu: &mut Emulator) -> String {
        emu.snapshot().expect("snapshot").contents()
    }

    #[test]
    fn at_the_bottom_nothing_is_below() {
        let mut emu = numbered(10);

        assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
    }

    #[test]
    fn scrolling_up_shows_earlier_lines() {
        let mut emu = numbered(10);

        emu.scroll(-3);

        assert_eq!(shown(&mut emu), "line-4\nline-5\nline-6\nline-7");
    }

    #[test]
    fn the_snapshot_says_how_many_rows_are_below_the_view() {
        let mut emu = numbered(10);

        emu.scroll(-3);

        assert_eq!((emu.below(), emu.snapshot().expect("snapshot").below), (3, 3));
    }

    #[test]
    fn scrolling_past_the_top_stops_at_the_first_line() {
        let mut emu = numbered(10);

        emu.scroll(-100);

        assert_eq!(shown(&mut emu), "line-1\nline-2\nline-3\nline-4");
    }

    #[test]
    fn new_output_keeps_the_view_and_grows_below() {
        let mut emu = numbered(10);
        emu.scroll(-3);

        emu.feed(b"\r\nline-11\r\nline-12");

        assert_eq!((shown(&mut emu), emu.below()), ("line-4\nline-5\nline-6\nline-7".to_string(), 5));
    }

    #[test]
    fn scrolling_down_to_the_end_leaves_nothing_below() {
        let mut emu = numbered(10);
        emu.scroll(-3);

        emu.scroll(3);

        assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
    }

    #[test]
    fn scrolling_to_the_bottom_shows_the_last_lines_again() {
        let mut emu = numbered(10);
        emu.scroll(-3);

        emu.scroll_to_bottom();

        assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
    }

    #[test]
    fn launches_read_the_active_screen_even_when_scrolled() {
        let mut emu = numbered(10);
        emu.scroll(-3);

        let active = emu.active_contents().expect("text");

        assert_eq!(active, "line-7\nline-8\nline-9\nline-10");
        assert_eq!(shown(&mut emu), "line-4\nline-5\nline-6\nline-7", "the view stays where it was");
    }

    #[test]
    fn a_program_that_turns_on_the_mouse_brings_the_view_back_down() {
        let mut emu = numbered(10);
        emu.scroll(-3);

        emu.feed(b"\x1b[?1000h");

        let snap = emu.snapshot().expect("snapshot");
        assert_eq!((snap.below, snap.contents()), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
    }

    #[test]
    fn the_alternate_screen_has_nothing_below() {
        let mut emu = numbered(10);
        emu.scroll(-3);

        emu.feed(b"\x1b[?1049h");

        assert_eq!((emu.alternate_screen(), emu.snapshot().expect("snapshot").below), (true, 0));
    }

    #[rstest]
    #[case::on_by_default(b"", true)]
    #[case::turned_off(b"\x1b[?1007l", false)]
    fn alternate_scroll_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
        let mut emu = numbered(1);
        emu.feed(output);

        assert_eq!(emu.alt_scroll(), expected);
    }
}

mod reveal {
    use super::*;

    fn after(rows: u16, cols: u16, output: &[u8]) -> Emulator {
        let mut emu = Emulator::new(rows, cols, 100_000, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
        emu.feed(output);
        emu
    }

    fn needle(text: &str) -> Needle {
        Needle::new(text).expect("a needle")
    }

    fn selected(emu: &mut Emulator) -> Option<String> {
        emu.finish_selection().expect("finish")
    }

    #[test]
    fn the_text_holds_the_scrollback_with_wrapped_lines_joined() {
        let emu = after(3, 10, b"one\r\ntwo\r\n0123456789abcde\r\nfour\r\nfive");

        assert_eq!(
            emu.text().expect("text").lines().collect::<Vec<_>>(),
            ["one", "two", "0123456789abcde", "four", "five"]
        );
    }

    #[test]
    fn logical_lines_start_where_the_wrap_continuations_do_even_after_blank_lines() {
        let emu = after(10, 10, b"a\r\n\r\n\r\n0123456789abcdefghij0123\r\nlast\r\n\r\nend");
        let lines = emu.text().expect("text").lines().count();

        let rows: Vec<Option<usize>> = (0..lines).map(|line| emu.line_row(line).expect("row")).collect();

        assert_eq!(rows, [Some(0), Some(1), Some(2), Some(3), Some(6), Some(7), Some(8)]);
    }

    #[test]
    fn selects_a_match_that_a_soft_wrap_cut_in_two() {
        let mut emu = after(4, 10, b"0123456789abcdefneedle\r\nnext");

        assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

        assert_eq!(selected(&mut emu).as_deref(), Some("needle"));
    }

    #[test]
    fn selects_a_match_after_wide_characters() {
        let mut emu = after(4, 10, "你好世界 needle".as_bytes());

        assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

        assert_eq!(selected(&mut emu).as_deref(), Some("needle"));
    }

    #[test]
    fn a_wide_character_that_does_not_fit_wraps_early() {
        let mut emu = after(4, 10, "012345678你好 needle".as_bytes());

        assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

        assert_eq!(selected(&mut emu).as_deref(), Some("needle"));
    }

    #[test]
    fn scrolls_to_an_old_line_and_marks_its_cells() {
        let mut emu = after(5, 20, b"first needle\r\n");
        for i in 0..40 {
            emu.feed(format!("filler {i}\r\n").as_bytes());
        }
        assert_eq!(emu.below(), 0);

        assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

        assert!(emu.below() > 0);
        let snap = emu.snapshot().expect("snapshot");
        let row =
            snap.rows.iter().find(|row| row.iter().map(|c| c.symbol.as_str()).collect::<String>().contains("needle"));
        let row = row.expect("the line shows");
        let at = row.iter().position(|c| c.symbol == "n").expect("a cell");
        assert!(row[at].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn picks_the_occurrence_nearest_the_line_it_was_found_on() {
        let mut emu = after(4, 20, b"needle one\r\nmiddle\r\nneedle two\r\ntail");
        let marked = |emu: &mut Emulator| {
            let snap = emu.snapshot().expect("snapshot");
            let row =
                snap.rows.iter().find(|row| row.iter().any(|c| c.style.add_modifier.contains(Modifier::REVERSED)));
            row.map(|row| row.iter().map(|c| c.symbol.as_str()).collect::<String>().trim_end().to_string())
        };

        assert!(emu.reveal(&needle("needle"), Some(0)).expect("reveal"));
        assert_eq!(marked(&mut emu).as_deref(), Some("needle one"));
        assert!(emu.reveal(&needle("needle"), Some(3)).expect("reveal"));
        assert_eq!(marked(&mut emu).as_deref(), Some("needle two"));
        assert!(emu.reveal(&needle("needle"), None).expect("reveal"));
        assert_eq!(marked(&mut emu).as_deref(), Some("needle two"));
    }

    #[test]
    fn without_a_match_it_does_nothing() {
        let mut emu = after(4, 20, b"nothing here");

        assert!(!emu.reveal(&needle("needle"), None).expect("reveal"));
        assert_eq!(selected(&mut emu), None);
    }
}
