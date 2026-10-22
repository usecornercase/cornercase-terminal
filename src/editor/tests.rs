use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rstest::rstest;

use super::*;

fn at(text: &str, cursor: usize) -> Editor {
    let mut e = Editor::new(text);
    assert!(e.jump(cursor), "the cursor fits");
    e
}

fn press(e: &mut Editor, code: KeyCode, modifiers: KeyModifiers) -> bool {
    e.key(KeyEvent::new(code, modifiers), 40, 4)
}

fn press_in_line(e: &mut Editor, code: KeyCode, modifiers: KeyModifiers) -> bool {
    e.one_line_key(KeyEvent::new(code, modifiers), 40)
}

mod wrapping {
    use super::*;

    #[rstest]
    #[case::fits("fix login", 20, vec![0..9])]
    #[case::empty("", 20, vec![0..0])]
    #[case::breaks_after_the_last_space("fix the login bug", 10, vec![0..8, 8..17])]
    #[case::cuts_a_word_longer_than_the_line("abcdefghij", 4, vec![0..4, 4..8, 8..10])]
    #[case::keeps_runs_of_spaces("a    b", 3, vec![0..3, 3..6])]
    fn lines(#[case] text: &str, #[case] width: usize, #[case] expected: Vec<Range<usize>>) {
        assert_eq!(wrap(text, width), expected);
    }

    #[test]
    fn a_zero_width_still_makes_progress() {
        assert_eq!(wrap("ab", 0), vec![0..1, 1..2]);
    }

    #[test]
    fn a_full_last_line_gets_an_empty_one_for_the_cursor() {
        assert_eq!(Editor::new("abcd").lines(4), vec![0..4, 4..4]);
    }
}

mod typing {
    use super::*;

    #[test]
    fn a_new_editor_puts_the_cursor_at_the_end() {
        assert_eq!(Editor::new("añb").cursor(), 3);
    }

    #[test]
    fn typing_goes_where_the_cursor_is() {
        let mut e = at("fix logn", 7);
        e.insert("i");
        assert_eq!((e.text().as_str(), e.cursor()), ("fix login", 8));
    }

    #[test]
    fn a_pasted_newline_becomes_a_space() {
        let mut e = Editor::new("a");
        e.insert("\nb\tc");
        assert_eq!(e.text(), "a b c");
    }

    #[test]
    fn backspace_removes_the_character_before_the_cursor() {
        let mut e = at("abc", 2);
        e.backspace();
        assert_eq!((e.text().as_str(), e.cursor()), ("ac", 1));
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut e = at("abc", 0);
        e.backspace();
        assert_eq!((e.text().as_str(), e.cursor()), ("abc", 0));
    }

    #[test]
    fn delete_removes_the_character_under_the_cursor() {
        let mut e = at("abc", 1);
        e.delete();
        assert_eq!((e.text().as_str(), e.cursor()), ("ac", 1));
    }

    #[test]
    fn delete_at_the_end_does_nothing() {
        let mut e = Editor::new("abc");
        e.delete();
        assert_eq!(e.text(), "abc");
    }
}

mod moving {
    use super::*;

    #[test]
    fn left_and_right_stop_at_the_edges() {
        let mut e = at("ab", 0);
        e.left();
        assert_eq!(e.cursor(), 0);
        e.right();
        e.right();
        e.right();
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn home_and_end_go_to_the_ends_of_the_text() {
        let mut e = at("fix the login bug", 9);
        e.home();
        assert_eq!(e.cursor(), 0);
        e.end();
        assert_eq!(e.cursor(), 17);
    }

    #[rstest]
    #[case::first_line(3, (0, 3))]
    #[case::start_of_the_second_line(8, (1, 0))]
    #[case::end(17, (1, 9))]
    fn the_position_follows_the_wrapped_lines(#[case] cursor: usize, #[case] expected: (usize, usize)) {
        assert_eq!(at("fix the login bug", cursor).position(10), expected);
    }

    #[test]
    fn the_end_of_a_full_line_sits_on_the_next_one() {
        assert_eq!(Editor::new("abcd").position(4), (1, 0));
    }

    #[test]
    fn down_keeps_the_column() {
        let mut e = at("fix the login bug", 2);
        e.down(10);
        assert_eq!(e.cursor(), 10);
    }

    #[test]
    fn down_stops_at_the_end_of_a_shorter_line() {
        let mut e = at("fix the login", 7);
        e.down(10);
        assert_eq!(e.cursor(), 13);
    }

    #[test]
    fn up_stays_on_the_line_it_lands_on() {
        let mut e = at("fix the login bug", 16);
        e.up(10);
        assert_eq!((e.cursor(), e.position(10).0), (7, 0));
    }

    #[test]
    fn up_on_the_first_line_and_down_on_the_last_do_nothing() {
        let mut e = at("fix the login bug", 2);
        e.up(10);
        assert_eq!(e.cursor(), 2);
        e.end();
        e.down(10);
        assert_eq!(e.cursor(), 17);
    }

    #[rstest]
    #[case::inside_a_line(1, 2, 10)]
    #[case::past_the_end_of_a_line(0, 30, 7)]
    #[case::past_the_end_of_the_text(1, 30, 17)]
    #[case::below_the_text(5, 0, 3)]
    fn a_click_places_the_cursor(#[case] line: usize, #[case] col: usize, #[case] expected: usize) {
        let mut e = at("fix the login bug", 3);
        e.place(10, line, col);
        assert_eq!(e.cursor(), expected);
    }
}

mod line_breaks {
    use super::*;

    fn typed(text: &str) -> Editor {
        let mut e = Editor::default();
        e.insert_lines(text);
        e
    }

    #[test]
    fn a_break_starts_a_new_line() {
        assert_eq!(typed("ab\ncd").lines(10), vec![0..2, 3..5]);
    }

    #[test]
    fn each_line_wraps_on_its_own() {
        assert_eq!(typed("fix the login\nbug").lines(10), vec![0..8, 8..13, 14..17]);
    }

    #[test]
    fn empty_lines_are_kept() {
        assert_eq!(typed("a\n\nb\n").lines(10), vec![0..1, 2..2, 3..4, 5..5]);
    }

    #[test]
    fn a_full_line_before_a_break_gets_an_empty_one_for_the_cursor() {
        assert_eq!(typed("abcd\nef").lines(4), vec![0..4, 4..4, 5..7]);
    }

    #[test]
    fn enter_inserts_a_break_where_the_cursor_is() {
        let mut e = at("abcd", 2);
        e.newline();
        assert_eq!((e.text().as_str(), e.position(10)), ("ab\ncd", (1, 0)));
    }

    #[test]
    fn a_paste_keeps_its_line_breaks() {
        let mut e = Editor::new("a");
        e.insert_lines("\r\nb\tc\rd");
        assert_eq!(e.text(), "a\nb c\nd");
    }

    #[test]
    fn the_end_of_a_line_sits_before_its_break() {
        assert_eq!(at("ab\ncd", 2).position(10), (0, 2));
    }

    #[test]
    fn a_click_past_the_end_of_a_line_stops_before_its_break() {
        let mut e = at("ab\ncd", 5);
        e.place(10, 0, 30);
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn up_and_down_cross_the_breaks() {
        let mut e = at("abc\nde\nfgh", 2);
        e.down(10);
        assert_eq!(e.cursor(), 6);
        e.down(10);
        assert_eq!(e.cursor(), 9);
        e.up(10);
        e.up(10);
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn home_and_end_of_a_line_stay_on_it() {
        let mut e = at("abc\ndef", 5);
        e.line_home(10);
        assert_eq!(e.cursor(), 4);
        e.line_end(10);
        assert_eq!(e.cursor(), 7);
        e.up(10);
        e.line_end(10);
        assert_eq!(e.cursor(), 3);
    }

    #[test]
    fn backspace_at_the_start_of_a_line_joins_it_to_the_one_above() {
        let mut e = at("ab\ncd", 3);
        e.backspace();
        assert_eq!((e.text().as_str(), e.cursor()), ("abcd", 2));
    }
}

mod scrolling {
    use super::*;

    fn tall(lines: usize) -> Editor {
        let mut e = Editor::default();
        e.insert_lines(&vec!["line"; lines].join("\n"));
        e
    }

    #[test]
    fn revealing_brings_the_cursor_into_view() {
        let mut e = tall(10);
        e.reveal(20, 4);
        assert_eq!(e.scroll(), 6);
        e.home();
        e.reveal(20, 4);
        assert_eq!(e.scroll(), 0);
    }

    #[test]
    fn a_cursor_in_view_does_not_scroll() {
        let mut e = tall(10);
        e.scroll_by(3, 20, 4);
        e.place(20, 4, 0);
        e.reveal(20, 4);
        assert_eq!(e.scroll(), 3);
    }

    #[test]
    fn the_wheel_stops_at_the_last_screenful() {
        let mut e = tall(10);
        e.scroll_by(50, 20, 4);
        assert_eq!(e.scroll(), 6);
        e.scroll_by(-50, 20, 4);
        assert_eq!(e.scroll(), 0);
    }

    #[test]
    fn text_shorter_than_the_view_never_scrolls() {
        let mut e = tall(3);
        e.scroll_by(5, 20, 4);
        assert_eq!(e.scroll(), 0);
    }

    #[test]
    fn the_wheel_leaves_the_cursor_where_it_was() {
        let mut e = tall(10);
        e.scroll_by(-2, 20, 4);
        assert_eq!(e.cursor(), 49);
    }
}

mod wide_characters {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use super::*;

    #[test]
    fn lines_wrap_by_the_columns_their_characters_take() {
        assert_eq!(Editor::new("你好世界").lines(5), vec![0..2, 2..4]);
    }

    #[test]
    fn the_cursor_column_counts_display_columns() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 1));
        buf.set_stringn(0, 0, "a你好", 8, Style::default());
        let e = at("a你好", 2);
        let (_, col) = e.position(8);
        assert_eq!((col, buf[(u16::try_from(col).expect("column"), 0)].symbol()), (3, "好"));
    }

    #[test]
    fn a_click_on_either_half_of_a_wide_character_lands_before_it() {
        let mut e = Editor::new("你好");
        let mut cursors = Vec::new();
        for col in 0..5 {
            e.place(8, 0, col);
            cursors.push(e.cursor());
        }
        assert_eq!(cursors, [0, 0, 1, 1, 2]);
    }

    #[test]
    fn a_line_full_of_wide_characters_gets_an_empty_one_for_the_cursor() {
        let e = Editor::new("你好");
        assert_eq!((e.lines(4), e.position(4)), (vec![0..2, 2..2], (1, 0)));
    }

    #[test]
    fn up_and_down_keep_the_display_column() {
        let mut e = at("你好\nabcd", 1);
        e.down(10);
        assert_eq!(e.cursor(), 5);
        e.up(10);
        assert_eq!(e.cursor(), 1);
    }
}

mod the_model {
    use super::*;

    #[test]
    fn a_text_keeps_its_carriage_returns_and_its_last_line_break() {
        assert_eq!(Editor::new("a\r\nb\n").text(), "a\r\nb\n");
    }

    #[test]
    fn a_new_editor_puts_the_cursor_at_the_end_of_its_last_line() {
        assert_eq!(Editor::new("ab\ncd").cursor(), 5);
    }

    #[test]
    fn delete_at_the_end_of_a_line_joins_the_next_one() {
        let mut e = at("ab\ncd", 2);
        e.delete();
        assert_eq!((e.text().as_str(), e.cursor()), ("abcd", 2));
    }

    #[test]
    fn delete_before_an_accent_at_the_end_of_a_line_removes_the_accent() {
        let mut e = at("e\u{301}\nx", 1);
        e.delete();
        assert_eq!(e.text(), "e\nx");
    }

    #[test]
    fn left_from_the_start_of_a_line_lands_after_an_accent_ending_the_one_above() {
        let mut e = at("e\u{301}\nx", 3);
        e.left();
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn editors_with_the_same_text_cursor_and_scroll_are_equal_whatever_their_history() {
        let mut typed = Editor::default();
        typed.insert("ab");
        assert_eq!(typed, Editor::new("ab"));
    }
}

mod words {
    use super::*;

    const TEXT: &str = "fix the login-bug\nnext";

    fn stops(mut e: Editor, code: KeyCode, presses: usize) -> Vec<usize> {
        (0..presses)
            .map(|_| {
                press(&mut e, code, KeyModifiers::CONTROL);
                e.cursor()
            })
            .collect()
    }

    #[test]
    fn ctrl_right_stops_at_each_word_start_and_at_the_next_line() {
        assert_eq!(stops(at(TEXT, 0), KeyCode::Right, 7), [4, 8, 13, 14, 18, 22, 22]);
    }

    #[test]
    fn ctrl_left_stops_at_each_word_start_and_at_the_end_of_the_line_above() {
        assert_eq!(stops(Editor::new(TEXT), KeyCode::Left, 8), [18, 17, 14, 13, 8, 4, 0, 0]);
    }

    #[rstest]
    #[case::plain(KeyModifiers::NONE)]
    #[case::shift(KeyModifiers::SHIFT)]
    #[case::alt(KeyModifiers::ALT)]
    fn other_modifiers_move_one_character(#[case] modifiers: KeyModifiers) {
        let mut e = Editor::new("fix the login");
        press(&mut e, KeyCode::Left, modifiers);
        assert_eq!(e.cursor(), 12);
    }

    #[test]
    fn a_one_line_field_moves_by_word_too() {
        let mut e = Editor::new("fix the login");
        press_in_line(&mut e, KeyCode::Left, KeyModifiers::CONTROL);
        e.insert("x");
        assert_eq!(e.text(), "fix the xlogin");
    }
}

mod one_line {
    use super::*;

    #[test]
    fn enter_is_left_to_the_caller() {
        let mut e = Editor::new("fix");
        assert!(!press_in_line(&mut e, KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(e.text(), "fix");
    }

    #[test]
    fn home_and_end_go_to_the_ends_of_the_text() {
        let mut e = at("fix the login bug", 9);
        press_in_line(&mut e, KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(e.cursor(), 0);
        press_in_line(&mut e, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(e.cursor(), 17);
    }

    #[test]
    fn a_paste_turns_line_breaks_tabs_and_control_characters_into_spaces() {
        let mut e = Editor::new("a");
        e.insert("\r\nb\tc\u{1b}d");
        assert_eq!(e.text(), "a  b c d");
    }
}

mod past_the_cursor_limit {
    use super::*;

    const LONG: usize = 70_000;

    #[test]
    fn left_and_right_still_move_one_character() {
        let mut e = Editor::new(&"x".repeat(LONG));
        e.left();
        e.left();
        e.right();
        assert_eq!(e.cursor(), LONG - 1);
    }

    #[test]
    fn a_click_there_leaves_the_cursor_where_it_was() {
        let mut e = Editor::new(&"x".repeat(LONG));
        e.place(LONG + 1, 0, LONG - 10);
        assert_eq!(e.cursor(), LONG);
    }

    #[test]
    fn delete_there_removes_one_character() {
        let mut e = Editor::new(&"x".repeat(LONG));
        e.left();
        e.delete();
        assert_eq!((e.text().len(), e.cursor()), (LONG - 1, LONG - 1));
    }
}
