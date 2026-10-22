use ratatui::backend::TestBackend;
use ratatui::{Terminal, style::Color};
use rstest::rstest;

use super::*;

fn drawn(text: &str, width: u16) -> Buffer {
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, 1));
    draw(&mut buffer, Color::DarkGray, Rect::new(0, 0, width, 1), &[(text.to_string(), Style::default())]);
    buffer
}

fn row(buffer: &Buffer) -> String {
    (0..buffer.area.width).map(|x| buffer[(x, 0)].symbol().to_string()).collect()
}

mod fitting {
    use super::*;

    #[rstest]
    #[case::ascii_that_fits("abcde", 5, Fit { shown: 5, width: 5, cut: false })]
    #[case::ascii_cut_leaves_a_cell_for_the_ellipsis("abcdef", 5, Fit { shown: 4, width: 4, cut: true })]
    #[case::wide_chars_count_two_cells("编码实现", 8, Fit { shown: 4, width: 8, cut: false })]
    #[case::wide_chars_are_cut_by_cells("编码实现", 6, Fit { shown: 2, width: 4, cut: true })]
    #[case::no_room("abc", 0, Fit { shown: 0, width: 0, cut: true })]
    #[case::empty("", 4, Fit { shown: 0, width: 0, cut: false })]
    fn cuts_by_cells(#[case] text: &str, #[case] room: u16, #[case] expected: Fit) {
        assert_eq!(fit(text, room), expected);
    }
}

mod standing_for {
    use super::*;

    #[rstest]
    #[case::a_glyph("abc", 1, 1..2)]
    #[case::a_wide_char_s_first_cell("a编b", 1, 1..2)]
    #[case::a_wide_char_s_second_cell("a编b", 2, 1..2)]
    #[case::the_char_after_a_wide_one("a编b", 3, 2..3)]
    #[case::past_the_text("abc", 9, 3..3)]
    #[case::a_vs16_emoji_is_one_glyph("a❤\u{fe0f}b", 2, 1..3)]
    #[case::the_cell_after_it("a❤\u{fe0f}b", 3, 3..4)]
    #[case::a_zwj_family_is_one_glyph("👨\u{200d}👩\u{200d}👧x", 1, 0..5)]
    #[case::a_combining_accent_joins_its_letter("e\u{301}x", 0, 0..2)]
    fn chars_are_found_by_cell(#[case] text: &str, #[case] column: u16, #[case] expected: Range<usize>) {
        assert_eq!(chars_at(text, 20, column), expected);
    }

    #[test]
    fn the_ellipsis_stands_for_everything_it_hides() {
        assert_eq!(chars_at("abcdefgh", 5, 4), 4..8);
        assert_eq!(chars_at("abcdefgh", 5, 3), 3..4);
        assert_eq!(chars_at("abcdefgh", 5, 9), 8..8);
    }

    #[test]
    fn a_cell_left_of_the_code_is_the_start_of_the_line() {
        let code = Rect::new(10, 0, 30, 1);
        assert_eq!(source_at(code, "abc", "abc", 4), 0..0);
        assert_eq!(source_at(code, "abc", "abc", 11), 1..2);
    }

    #[test]
    fn a_cell_in_a_tab_stands_for_the_tab() {
        let code = Rect::new(0, 0, 30, 1);
        assert_eq!(source_at(code, "    ab", "\tab", 2), 0..1);
        assert_eq!(source_at(code, "    ab", "\tab", 4), 1..2);
        assert_eq!(source_at(code, "    ab", "\tab", 20), 3..3);
    }
}

mod drawing {
    use super::*;

    #[rstest]
    #[case::tabs("\tlet x = 1;")]
    #[case::cjk("let s = \"编码实现编码\";")]
    #[case::a_combining_accent("let e\u{301}t\u{e9} = 1;")]
    #[case::emoji("ok ❤\u{fe0f} 👨\u{200d}👩\u{200d}👧 done")]
    #[case::cut_by_the_edge("let very_long_name = some_function_with_a_long_name(argument_one, argument_two);")]
    #[case::cut_wide_chars("编码实现编码实现编码实现编码实现编码实现编码实现")]
    fn every_cell_shows_the_glyph_chars_at_names(#[case] raw: &str) {
        let text = crate::text::TAB;
        let shown = raw.replace('\t', text);
        let width = 24;
        let buffer = drawn(&shown, width);
        let room = width - 1;
        let chars: Vec<char> = shown.chars().collect();
        for column in 0..room {
            let range = chars_at(&shown, room, column);
            let symbol = buffer[(column, 0)].symbol();
            if range.is_empty() || range.end > fit(&shown, room).shown {
                continue;
            }
            let glyph: String = chars[range.clone()].iter().collect();
            let wide_tail = column > 0 && chars_at(&shown, room, column - 1) == range;
            assert!(wide_tail || symbol == glyph, "column {column}: {symbol:?} vs {glyph:?} in {}", row(&buffer));
        }
    }

    #[test]
    fn a_cut_line_shows_the_ellipsis_where_fit_says() {
        let buffer = drawn("abcdefghij", 6);
        assert_eq!(row(&buffer), "abcd… ");
    }

    #[test]
    fn a_wide_char_that_does_not_fit_gives_way_to_the_ellipsis() {
        let buffer = drawn("abcd编e", 6);
        assert_eq!(row(&buffer), "abcd… ");
    }
}

mod painting {
    use super::*;

    fn painted(text: &str, on: &(Range<usize>, bool)) -> Vec<bool> {
        let code = Rect::new(2, 0, 10, 1);
        let mut terminal = Terminal::new(TestBackend::new(14, 1)).expect("terminal");
        terminal
            .draw(|f| {
                draw(f.buffer_mut(), Color::DarkGray, code, &[(text.to_string(), Style::default())]);
                paint(f.buffer_mut(), code, (text, text), on);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        (0..14).map(|x| buffer[(x, 0)].modifier.contains(Modifier::REVERSED)).collect()
    }

    fn cells(from: usize, to: usize) -> Vec<bool> {
        (0..14).map(|x| (from..to).contains(&x)).collect()
    }

    #[test]
    fn a_middle_line_reverses_its_whole_code_rect() {
        assert_eq!(painted("abcdef", &(0..6, true)), cells(2, 12));
    }

    #[test]
    fn the_first_line_reverses_from_its_anchor_to_the_edge() {
        assert_eq!(painted("abcdef", &(2..6, true)), cells(4, 12));
    }

    #[test]
    fn a_single_line_reverses_only_its_cells() {
        assert_eq!(painted("abcdef", &(1..3, false)), cells(3, 5));
    }

    #[test]
    fn the_ellipsis_is_reversed_when_the_hidden_rest_is_selected() {
        assert_eq!(painted("abcdefghijklmn", &(7..14, false)), cells(9, 11));
        assert_eq!(painted("abcdefghijklmn", &(2..3, false)), cells(4, 5));
    }

    #[test]
    fn a_line_break_alone_reverses_from_the_end_of_the_text() {
        assert_eq!(painted("abc", &(3..3, true)), cells(5, 12));
    }

    #[test]
    fn a_selected_tab_reverses_its_four_cells() {
        let code = Rect::new(0, 0, 10, 1);
        let mut buffer = Buffer::empty(code);
        draw(&mut buffer, Color::DarkGray, code, &[("    ab".to_string(), Style::default())]);
        paint(&mut buffer, code, ("    ab", "\tab"), &(0..1, false));
        let on: Vec<bool> = (0..10).map(|x| buffer[(x, 0)].modifier.contains(Modifier::REVERSED)).collect();
        assert_eq!(on, [true, true, true, true, false, false, false, false, false, false]);
    }
}
