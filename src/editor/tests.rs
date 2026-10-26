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
    fn other_modifiers_move_one_character(#[case] modifiers: KeyModifiers) {
        let mut e = Editor::new("fix the login");
        press(&mut e, KeyCode::Left, modifiers);
        assert_eq!(e.cursor(), 12);
    }

    #[rstest]
    #[case::alt_arrow(KeyCode::Left, KeyModifiers::ALT, KeyCode::Right, KeyModifiers::ALT)]
    #[case::esc_b_and_esc_f(KeyCode::Char('b'), KeyModifiers::ALT, KeyCode::Char('f'), KeyModifiers::ALT)]
    fn option_moves_by_word_as_ctrl_does(
        #[case] back: KeyCode,
        #[case] back_mods: KeyModifiers,
        #[case] forward: KeyCode,
        #[case] forward_mods: KeyModifiers,
    ) {
        let mut e = Editor::new("fix the login");
        press(&mut e, back, back_mods);
        assert_eq!(e.cursor(), 8);
        press(&mut e, back, back_mods);
        assert_eq!(e.cursor(), 4);
        press(&mut e, forward, forward_mods);
        assert_eq!(e.cursor(), 8);
    }

    #[rstest]
    #[case::alt_shift_b(KeyModifiers::ALT | KeyModifiers::SHIFT)]
    #[case::ctrl_alt_b(KeyModifiers::ALT | KeyModifiers::CONTROL)]
    #[case::ctrl_b(KeyModifiers::CONTROL)]
    fn other_b_combinations_insert_nothing(#[case] modifiers: KeyModifiers) {
        let mut e = Editor::new("fix");
        press(&mut e, KeyCode::Char('b'), modifiers);
        assert_eq!(e.text(), "fix");
        assert_eq!(e.cursor(), 3);
    }

    #[test]
    fn a_one_line_field_moves_by_word_too() {
        let mut e = Editor::new("fix the login");
        press_in_line(&mut e, KeyCode::Left, KeyModifiers::CONTROL);
        e.insert("x");
        assert_eq!(e.text(), "fix the xlogin");
    }
}

mod line_ends {
    use super::*;

    #[test]
    fn ctrl_a_and_ctrl_e_reach_the_ends_of_a_one_line_field() {
        let mut e = at("fix the login bug", 9);
        assert!(press_in_line(&mut e, KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(e.cursor(), 0);
        assert!(press_in_line(&mut e, KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert_eq!((e.text().as_str(), e.cursor()), ("fix the login bug", 17));
    }

    #[test]
    fn ctrl_a_and_ctrl_e_stay_on_the_line_of_a_multi_line_field() {
        let mut e = at("abc\ndef", 5);
        press(&mut e, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(e.cursor(), 4);
        press(&mut e, KeyCode::Char('e'), KeyModifiers::CONTROL);
        assert_eq!(e.cursor(), 7);
    }

    #[test]
    fn ctrl_a_drops_the_selection() {
        let mut e = Editor::new("fix the login");
        press_in_line(&mut e, KeyCode::Left, KeyModifiers::SHIFT);
        press_in_line(&mut e, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!((e.selection(), e.cursor()), (None, 0));
    }

    #[rstest]
    #[case::ctrl_alt_a(KeyCode::Char('a'), KeyModifiers::ALT | KeyModifiers::CONTROL)]
    #[case::alt_e(KeyCode::Char('e'), KeyModifiers::ALT)]
    fn other_a_and_e_combinations_do_nothing(#[case] code: KeyCode, #[case] modifiers: KeyModifiers) {
        let mut e = at("fix", 1);
        assert!(!press_in_line(&mut e, code, modifiers));
        assert_eq!((e.text().as_str(), e.cursor()), ("fix", 1));
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

mod selecting {
    use super::*;

    fn shift(e: &mut Editor, code: KeyCode, presses: usize) {
        for _ in 0..presses {
            press(e, code, KeyModifiers::SHIFT);
        }
    }

    #[test]
    fn shift_home_and_end_extend_the_selection() {
        let mut e = at("fix the login", 4);
        press_in_line(&mut e, KeyCode::End, KeyModifiers::SHIFT);
        assert_eq!(e.selection(), Some(4..13));
        press_in_line(&mut e, KeyCode::Home, KeyModifiers::SHIFT);
        assert_eq!(e.selection(), Some(0..4));
        press(&mut e, KeyCode::End, KeyModifiers::SHIFT);
        assert_eq!(e.selection(), Some(4..13));
    }

    #[test]
    fn shift_and_the_arrows_select_and_typing_replaces_the_selection() {
        let mut e = at("fix the login", 0);
        shift(&mut e, KeyCode::Right, 3);
        assert_eq!(e.selection(), Some(0..3));
        press(&mut e, KeyCode::Char('F'), KeyModifiers::SHIFT);
        assert_eq!((e.text().as_str(), e.selection()), ("F the login", None));
    }

    #[rstest]
    #[case::ctrl(KeyModifiers::CONTROL | KeyModifiers::SHIFT)]
    #[case::alt(KeyModifiers::ALT | KeyModifiers::SHIFT)]
    fn a_modifier_with_shift_selects_by_word(#[case] modifiers: KeyModifiers) {
        let mut e = Editor::new("fix the login");
        press(&mut e, KeyCode::Left, modifiers);
        press(&mut e, KeyCode::Left, modifiers);
        assert_eq!(e.selected_text().as_deref(), Some("the login"));
    }

    #[test]
    fn shift_up_selects_to_the_line_above() {
        let mut e = Editor::new("fix the login bug");
        e.key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT), 10, 4);
        assert_eq!(e.selection(), Some(7..17));
    }

    #[test]
    fn backspace_and_delete_remove_the_selection_even_at_the_end() {
        let mut e = at("fix the login", 8);
        shift(&mut e, KeyCode::Right, 5);
        press(&mut e, KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(e.text(), "fix the ");
        shift(&mut e, KeyCode::Left, 4);
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(e.text(), "fix ");
    }

    #[rstest]
    #[case::left(KeyCode::Left, 4)]
    #[case::right(KeyCode::Right, 7)]
    fn a_plain_arrow_goes_to_that_end_of_the_selection(#[case] code: KeyCode, #[case] cursor: usize) {
        let mut e = at("fix the login", 4);
        shift(&mut e, KeyCode::Right, 3);
        press(&mut e, code, KeyModifiers::NONE);
        assert_eq!((e.selection(), e.cursor()), (None, cursor));
    }

    #[rstest]
    #[case::up(KeyCode::Up)]
    #[case::down(KeyCode::Down)]
    #[case::home(KeyCode::Home)]
    #[case::end(KeyCode::End)]
    #[case::word(KeyCode::Char('b'))]
    fn other_moves_clear_the_selection(#[case] code: KeyCode) {
        let mut e = at("fix the login", 4);
        shift(&mut e, KeyCode::Right, 3);
        let modifiers = if code == KeyCode::Char('b') { KeyModifiers::ALT } else { KeyModifiers::NONE };
        press(&mut e, code, modifiers);
        assert_eq!(e.selection(), None);
    }

    #[test]
    fn enter_and_a_paste_replace_the_selection() {
        let mut e = Editor::new("fix the login");
        shift(&mut e, KeyCode::Left, 6);
        press(&mut e, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(e.text(), "fix the\n");
        shift(&mut e, KeyCode::Left, 4);
        e.insert_lines("a\nb");
        assert_eq!(e.text(), "fix a\nb");
    }

    #[test]
    fn a_one_line_field_keeps_enter_for_its_caller() {
        let mut e = Editor::new("fix");
        shift(&mut e, KeyCode::Left, 1);
        assert!(!e.one_line_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), 40));
        assert_eq!(e.selected_text().as_deref(), Some("x"));
    }
}

mod mouse {
    use super::*;

    #[test]
    fn a_drag_selects_from_the_press_to_the_pointer() {
        let mut e = Editor::new("fix the login bug");
        e.place(10, 0, 4);
        e.drag_to(10, 1, 5);
        assert_eq!(e.selected_text().as_deref(), Some("the login"));
    }

    #[test]
    fn dragging_back_to_the_press_selects_nothing() {
        let mut e = Editor::new("fix the login");
        e.place(40, 0, 4);
        e.drag_to(40, 0, 8);
        e.drag_to(40, 0, 4);
        e.settle();
        press(&mut e, KeyCode::Right, KeyModifiers::NONE);
        assert_eq!((e.selection(), e.cursor()), (None, 5));
    }

    #[test]
    fn dragging_below_the_text_selects_to_its_end() {
        let mut e = Editor::new("fix the login");
        e.place(40, 0, 4);
        e.drag_to(40, 7, 0);
        assert_eq!(e.selected_text().as_deref(), Some("the login"));
    }

    #[test]
    fn a_click_clears_the_selection() {
        let mut e = Editor::new("fix the login");
        e.select_all();
        e.place(40, 0, 2);
        assert_eq!((e.selection(), e.cursor()), (None, 2));
    }

    #[rstest]
    #[case::a_word(10, "login")]
    #[case::punctuation(14, "-")]
    #[case::spaces(3, "  ")]
    #[case::past_the_end_of_a_line(18, "bug")]
    fn a_double_click_selects_the_word_under_it(#[case] cursor: usize, #[case] word: &str) {
        let mut e = at("fix  the login-bug\nnext", cursor);
        e.select_word();
        assert_eq!(e.selected_text().as_deref(), Some(word));
    }

    #[rstest]
    #[case::inside(0, 6, true)]
    #[case::at_its_end(0, 8, false)]
    #[case::before_it(0, 2, false)]
    fn a_right_click_knows_whether_it_is_inside_the_selection(
        #[case] line: usize,
        #[case] col: usize,
        #[case] inside: bool,
    ) {
        let mut e = Editor::new("fix the login");
        e.select(4..8);
        assert_eq!(e.selected_at(40, line, col), inside);
    }

    #[test]
    fn a_double_click_on_an_empty_line_selects_nothing() {
        let mut e = at("a\n\nb", 2);
        e.select_word();
        assert_eq!(e.selection(), None);
    }
}

mod clipboard {
    use super::*;

    #[test]
    fn the_selected_text_keeps_its_line_breaks() {
        let mut e = at("ab\ncd", 1);
        press(&mut e, KeyCode::Right, KeyModifiers::SHIFT);
        press(&mut e, KeyCode::Right, KeyModifiers::SHIFT);
        press(&mut e, KeyCode::Right, KeyModifiers::SHIFT);
        assert_eq!(e.selected_text().as_deref(), Some("b\nc"));
    }

    #[test]
    fn cut_returns_the_text_and_removes_it() {
        let mut e = Editor::new("fix the login");
        e.select_all();
        assert_eq!((e.cut().as_deref(), e.text().as_str()), (Some("fix the login"), ""));
        assert_eq!(e.cut(), None);
    }

    #[test]
    fn select_all_then_typing_replaces_everything() {
        let mut e = at("ab\ncd", 1);
        e.select_all();
        e.insert("x");
        assert_eq!(e.text(), "x");
    }
}

mod drawing {
    use super::*;

    fn mark(line: usize, cells: Range<usize>, to_edge: bool) -> Highlight {
        Highlight { line, cells, to_edge }
    }

    #[test]
    fn a_selected_line_break_reaches_the_edge() {
        let mut e = at("ab\ncd", 1);
        e.select(1..4);
        assert_eq!(e.highlight(10), [mark(0, 1..2, true), mark(1, 0..1, false)]);
    }

    #[test]
    fn a_selection_across_a_wrap_reaches_the_edge_of_the_upper_line() {
        let mut e = Editor::new("fix the login bug");
        e.select(4..10);
        assert_eq!(e.highlight(10), [mark(0, 4..8, true), mark(1, 0..2, false)]);
    }

    #[test]
    fn a_selection_starting_at_a_wrap_leaves_the_upper_line_alone() {
        let mut e = Editor::new("fix the login bug");
        e.select(8..13);
        assert_eq!(e.highlight(10), [mark(1, 0..5, false)]);
    }

    #[test]
    fn wide_characters_take_two_cells() {
        let mut e = Editor::new("a你好b");
        e.select(1..3);
        assert_eq!(e.highlight(10), [mark(0, 1..5, false)]);
    }

    #[test]
    fn nothing_selected_draws_nothing() {
        assert_eq!(Editor::new("fix").highlight(10), []);
    }

    #[test]
    fn the_selection_counts_in_equality() {
        let mut e = Editor::new("fix");
        e.select(0..1);
        assert_ne!(e, Editor::new("fix"));
    }
}
