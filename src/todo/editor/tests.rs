use rstest::rstest;

use super::*;

fn at(text: &str, cursor: usize) -> Editor {
    Editor { cursor, ..Editor::new(text) }
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
