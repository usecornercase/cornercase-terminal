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

mod surface_state {
    use super::*;

    #[test]
    fn marks_the_cell_after_a_wide_character_as_its_spacer() {
        let snap = snapshot_of("中x".as_bytes());
        let row = &snap.rows[0];

        assert_eq!((row[0].symbol.as_str(), row[0].spacer), ("中", false));
        assert!(row[1].spacer);
        assert_eq!((row[2].symbol.as_str(), row[2].spacer), ("x", false));
    }

    #[rstest]
    #[case::bar(b"\x1b[5 q", CursorShape::Bar, true)]
    #[case::block(b"\x1b[2 q", CursorShape::Block, false)]
    #[case::underline(b"\x1b[4 q", CursorShape::Underline, false)]
    fn says_the_cursor_shape_and_whether_it_blinks(
        #[case] output: &[u8],
        #[case] shape: CursorShape,
        #[case] blinking: bool,
    ) {
        let snap = snapshot_of(output);

        assert_eq!((snap.shape, snap.blinking), (shape, blinking));
    }

    #[test]
    fn focus_events_follow_the_program() {
        let (mut emu, _) = emulator();
        assert!(!emu.focus_events());

        emu.feed(b"\x1b[?1004h");

        assert!(emu.focus_events());
    }

    #[test]
    fn kitty_flags_follow_the_program() {
        let (mut emu, _) = emulator();
        assert_eq!(emu.kitty_keyboard(), 0);

        emu.feed(b"\x1b[>1u");

        assert_eq!(emu.kitty_keyboard(), 1);
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

mod bell {
    use super::*;

    fn rung_by(output: &[u8]) -> u32 {
        let (mut emu, _) = emulator();
        emu.feed(output);
        emu.take_bells()
    }

    #[rstest]
    #[case::one(b"\x07", 1)]
    #[case::two(b"a\x07b\x07", 2)]
    #[case::none(b"hello", 0)]
    #[case::a_title_ended_by_bel(b"\x1b]0;title\x07", 0)]
    #[case::a_clipboard_write_ended_by_bel(b"\x1b]52;c;aGVsbG8=\x07", 0)]
    #[case::a_status_report_ended_by_bel(b"\x1b]7501;state=blocked:kind=question\x07", 0)]
    fn counts_only_the_bells_a_program_rings(#[case] output: &[u8], #[case] count: u32) {
        assert_eq!(rung_by(output), count);
    }

    #[test]
    fn taking_resets_the_count() {
        let (mut emu, _) = emulator();
        emu.feed(b"\x07");
        emu.take_bells();
        assert_eq!(emu.take_bells(), 0);
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

mod host_theme;

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

mod scrollback;

mod reveal;

mod driven_selection {
    use super::*;

    fn written() -> Emulator {
        let (mut emu, _) = emulator();
        emu.feed(b"\x1b[2J\x1b[Hhello world\r\nsecond line");
        emu
    }

    #[test]
    fn selects_between_two_cells_and_marks_them() {
        let mut emu = written();

        let text = emu.select(Position::new(0, 0), Position::new(4, 0), false).expect("select");

        assert_eq!(text.as_deref(), Some("hello"));
        let row = &emu.snapshot().expect("snapshot").rows[0];
        assert!(
            row[0].style.add_modifier.contains(Modifier::REVERSED)
                && !row[6].style.add_modifier.contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn a_rectangle_selects_a_block() {
        let mut emu = written();

        let text = emu.select(Position::new(0, 0), Position::new(1, 1), true).expect("select");

        assert_eq!(text.as_deref(), Some("he\nse"));
    }

    #[test]
    fn nothing_is_selected_once_cleared() {
        let mut emu = written();
        emu.select(Position::new(0, 0), Position::new(4, 0), false).expect("select");

        emu.clear_selection().expect("clear");

        assert_eq!(emu.selection_text().expect("text"), None);
    }

    #[test]
    fn the_top_of_the_history_is_offset_zero() {
        let mut emu = Emulator::new(24, 80, 100_000, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
        emu.feed("x\r\n".repeat(200).as_bytes());
        assert!(emu.scrollbar().offset > 0);

        emu.scroll_to_top();

        assert_eq!(emu.scrollbar().offset, 0);
    }
}

mod background_erase {
    use super::*;

    #[test]
    fn a_line_cleared_with_a_palette_background_keeps_it() {
        let cells = snapshot_of(b"\x1b[44mab\x1b[K").rows.remove(0);
        assert_eq!(cells[1].style.bg, Some(Color::Indexed(4)));
        assert_eq!(cells[40].style.bg, Some(Color::Indexed(4)));
    }

    #[test]
    fn cells_erased_with_an_rgb_background_keep_it() {
        let cells = snapshot_of(b"\x1b[48;2;1;2;3m\x1b[2X").rows.remove(0);
        assert_eq!(cells[1].style.bg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(cells[2].style, Style::default());
    }

    #[test]
    fn a_plain_blank_stays_default() {
        assert_eq!(snapshot_of(b"ab\x1b[K").rows[0][10].style, Style::default());
    }
}

mod cell_decor {
    use super::*;

    fn decor_of(output: &[u8]) -> Cell {
        snapshot_of(output).rows[0][0].clone()
    }

    #[rstest]
    #[case::curly(b"\x1b[4:3mx", UnderlineStyle::Curly)]
    #[case::double(b"\x1b[21mx", UnderlineStyle::Double)]
    #[case::double_sub(b"\x1b[4:2mx", UnderlineStyle::Double)]
    #[case::dotted(b"\x1b[4:4mx", UnderlineStyle::Dotted)]
    #[case::dashed(b"\x1b[4:5mx", UnderlineStyle::Dashed)]
    #[case::single(b"\x1b[4mx", UnderlineStyle::Single)]
    fn reads_the_underline_style(#[case] output: &[u8], #[case] expected: UnderlineStyle) {
        let cell = decor_of(output);
        assert_eq!(cell.decor.underline, expected);
        assert!(cell.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[rstest]
    #[case::palette(b"\x1b[4;58:5:196mx", Color::Indexed(196))]
    #[case::palette_semicolons(b"\x1b[58;5;12;4mx", Color::Indexed(12))]
    #[case::rgb(b"\x1b[4;58:2::1:2:3mx", Color::Rgb(1, 2, 3))]
    #[case::none(b"\x1b[4mx", Color::Reset)]
    fn reads_the_underline_colour(#[case] output: &[u8], #[case] expected: Color) {
        assert_eq!(decor_of(output).decor.underline_colour, expected);
    }

    #[test]
    fn reads_the_overline() {
        assert!(decor_of(b"\x1b[53mx").decor.overline);
        assert!(!decor_of(b"x").decor.overline);
    }
}

mod hyperlinks {
    use super::*;

    fn surface_of(cols: u16, output: &[u8]) -> Snapshot {
        let replies = Box::new(|_: &[u8]| {});
        let mut emu = Emulator::new(6, cols, 0, &HostTheme::default(), replies).expect("emulator");
        emu.feed(output);
        emu.surface_snapshot().expect("snapshot")
    }

    #[test]
    fn the_window_snapshot_reads_no_hyperlinks() {
        let snap = snapshot_of(b"\x1b]8;;https://example.com\x1b\\site\x1b]8;;\x1b\\");
        assert_eq!(snap.links.len(), 0);
        assert!(snap.rows.iter().flatten().all(|cell| cell.link.is_none()));
    }

    #[test]
    fn a_hyperlink_names_its_uri_on_every_cell_and_across_a_wrap() {
        let output = b"\x1b]8;;https://example.com/a\x1b\\abcdefghijklmnopqrstuvwxyz0123\x1b]8;;\x1b\\ plain";
        let snap = surface_of(20, output);
        assert_eq!(snap.links, ["https://example.com/a"]);
        assert!(snap.rows[0].iter().all(|cell| cell.link == Some(0)));
        assert!(snap.rows[1][..10].iter().all(|cell| cell.link == Some(0)));
        assert!(snap.rows[1][11..].iter().all(|cell| cell.link.is_none()));
    }

    #[test]
    fn two_addresses_are_two_entries() {
        let snap = surface_of(40, b"\x1b]8;;https://a.test\x1b\\a\x1b]8;;https://b.test\x1b\\b\x1b]8;;\x1b\\c");
        assert_eq!(snap.links, ["https://a.test", "https://b.test"]);
        let links: Vec<_> = snap.rows[0][..3].iter().map(|cell| cell.link).collect();
        assert_eq!(links, [Some(0), Some(1), None]);
    }

    #[test]
    fn a_blank_cell_in_a_link_carries_it() {
        let snap = surface_of(40, b"\x1b]8;;file:///tmp/x\x1b\\   \x1b]8;;\x1b\\<");
        assert_eq!(snap.links, ["file:///tmp/x"]);
        assert_eq!(snap.rows[0][1].link, Some(0));
        assert_eq!(snap.rows[0][3].link, None);
    }
}

mod program_colours {
    use super::*;
    use libghostty_vt::style::RgbColor;

    fn after(output: &[u8]) -> ProgramColours {
        let (mut emu, _) = emulator();
        emu.feed(output);
        emu.program_colours()
    }

    #[test]
    fn a_program_that_changed_nothing_has_none() {
        assert_eq!(after(b"hello"), ProgramColours::default());
    }

    #[test]
    fn the_palette_and_default_colours_a_program_sets_show() {
        let colours = after(
            b"\x1b]4;1;rgb:ff/00/00\x07\x1b]4;200;#102030\x07\x1b]10;#112233\x07\x1b]11;rgb:44/55/66\x07\x1b]12;#778899\x07",
        );
        assert_eq!(colours.palette, [(1, [255, 0, 0]), (200, [16, 32, 48])]);
        assert_eq!(colours.foreground, Some([0x11, 0x22, 0x33]));
        assert_eq!(colours.background, Some([0x44, 0x55, 0x66]));
        assert_eq!(colours.cursor, Some([0x77, 0x88, 0x99]));
    }

    #[test]
    fn a_host_theme_is_not_an_override_also_when_it_changes() {
        let mut theme = HostTheme { foreground: Some(RgbColor { r: 200, g: 200, b: 200 }), ..HostTheme::default() };
        theme.palette[1] = Some(RgbColor { r: 1, g: 2, b: 3 });
        let (mut emu, _) = themed_emulator(&theme);
        assert_eq!(emu.program_colours(), ProgramColours::default());
        emu.feed(b"\x1b]4;5;#abcdef\x07");
        theme.palette[1] = Some(RgbColor { r: 9, g: 9, b: 9 });
        emu.set_theme(&theme).expect("theme");
        assert_eq!(emu.program_colours().palette, [(5, [0xab, 0xcd, 0xef])]);
    }

    #[rstest]
    #[case::one_entry(b"\x1b]104;1\x07")]
    #[case::the_palette(b"\x1b]104\x07")]
    #[case::the_foreground(b"\x1b]110\x07")]
    #[case::the_background(b"\x1b]111\x07")]
    #[case::the_cursor(b"\x1b]112\x07")]
    fn resets_clear_what_they_name(#[case] reset: &[u8]) {
        let mut output = b"\x1b]4;1;#ff0000\x07\x1b]10;#112233\x07\x1b]11;#445566\x07\x1b]12;#778899\x07".to_vec();
        output.extend_from_slice(reset);
        let colours = after(&output);
        let left = usize::from(!colours.palette.is_empty())
            + usize::from(colours.foreground.is_some())
            + usize::from(colours.background.is_some())
            + usize::from(colours.cursor.is_some());
        assert_eq!(left, 3);
    }
}
