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
    #[case::ascii_that_fits("abcde", 5, Fit { chars: 0..5, width: 5, end: End::Line })]
    #[case::ascii_cut_leaves_a_cell_for_the_ellipsis("abcdef", 5, Fit { chars: 0..4, width: 4, end: End::Cut })]
    #[case::wide_chars_count_two_cells("编码实现", 8, Fit { chars: 0..4, width: 8, end: End::Line })]
    #[case::wide_chars_are_cut_by_cells("编码实现", 6, Fit { chars: 0..2, width: 4, end: End::Cut })]
    #[case::no_room("abc", 0, Fit { chars: 0..0, width: 0, end: End::Cut })]
    #[case::empty("", 4, Fit { chars: 0..0, width: 0, end: End::Line })]
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
        assert_eq!(chars_at(text, &fit(text, 20), column), expected);
    }

    #[test]
    fn the_ellipsis_stands_for_everything_it_hides() {
        let row = fit("abcdefgh", 5);
        assert_eq!(chars_at("abcdefgh", &row, 4), 4..8);
        assert_eq!(chars_at("abcdefgh", &row, 3), 3..4);
        assert_eq!(chars_at("abcdefgh", &row, 9), 8..8);
    }

    #[test]
    fn a_cell_left_of_the_code_is_the_start_of_the_line() {
        let code = Rect::new(10, 0, 30, 1);
        assert_eq!(source_at(code, ("abc", "abc"), &fit("abc", 29), 4), 0..0);
        assert_eq!(source_at(code, ("abc", "abc"), &fit("abc", 29), 11), 1..2);
    }

    #[test]
    fn a_cell_in_a_tab_stands_for_the_tab() {
        let code = Rect::new(0, 0, 30, 1);
        let row = fit("    ab", 29);
        assert_eq!(source_at(code, ("    ab", "\tab"), &row, 2), 0..1);
        assert_eq!(source_at(code, ("    ab", "\tab"), &row, 4), 1..2);
        assert_eq!(source_at(code, ("    ab", "\tab"), &row, 20), 3..3);
    }

    #[test]
    fn a_continuation_row_names_chars_offset_by_its_start() {
        let text = "abcdef ghijkl";
        let rows = wrap(text, 7);
        assert_eq!(chars_at(text, &rows[1], 0), 7..8);
        assert_eq!(chars_at(text, &rows[1], 5), 12..13);
        assert_eq!(chars_at(text, &rows[1], 9), 13..13);
    }

    #[test]
    fn past_the_text_of_a_wrapped_row_is_its_end() {
        let text = "abcdef ghijkl";
        let rows = wrap(text, 9);
        assert_eq!(rows[0].end, End::Wrap);
        assert_eq!(chars_at(text, &rows[0], 8), 7..7);
    }

    #[test]
    fn the_ellipsis_of_a_capped_line_gives_the_rest() {
        let text = "x".repeat(40);
        let rows = lay_out(&text, 5, 2);
        assert_eq!(rows[1].end, End::Cut);
        assert_eq!(chars_at(&text, &rows[1], rows[1].width), rows[1].chars.end..40);
    }

    #[test]
    fn a_cell_left_of_the_code_on_a_continuation_row_is_the_start_of_that_row() {
        let code = Rect::new(10, 0, 8, 1);
        let rows = wrap("abcdefghijklmnop", 7);
        assert_eq!(source_at(code, ("abcdefghijklmnop", "abcdefghijklmnop"), &rows[1], 3), 7..7);
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
        let edge = fit(&shown, room);
        let chars: Vec<char> = shown.chars().collect();
        for column in 0..room {
            let range = chars_at(&shown, &edge, column);
            let symbol = buffer[(column, 0)].symbol();
            if range.is_empty() || range.end > edge.chars.end {
                continue;
            }
            let glyph: String = chars[range.clone()].iter().collect();
            let wide_tail = column > 0 && chars_at(&shown, &edge, column - 1) == range;
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
                paint(f.buffer_mut(), code, (text, text), &fit(text, 9), on);
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
        paint(&mut buffer, code, ("    ab", "\tab"), &fit("    ab", 9), &(0..1, false));
        let on: Vec<bool> = (0..10).map(|x| buffer[(x, 0)].modifier.contains(Modifier::REVERSED)).collect();
        assert_eq!(on, [true, true, true, true, false, false, false, false, false, false]);
    }
}

mod wrapping {
    use super::*;

    fn breaks(text: &str, room: u16) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        wrap(text, room).iter().map(|row| chars[row.chars.clone()].iter().collect()).collect()
    }

    #[rstest]
    #[case::fits("abcde", 5, &["abcde"])]
    #[case::after_a_space("hello world again", 12, &["hello world ", "again"])]
    #[case::after_punctuation("let a = address.street.name;", 20, &["let a = address.", "street.name;"])]
    #[case::at_the_last_cell("abcdefghijkl", 5, &["abcde", "fghij", "kl"])]
    #[case::a_wide_char_moves_whole("abcd编码", 5, &["abcd", "编码"])]
    #[case::a_vs16_emoji_moves_whole("abcd❤\u{fe0f}x", 5, &["abcd", "❤\u{fe0f}x"])]
    #[case::a_zwj_family_moves_whole("ab👨\u{200d}👩\u{200d}👧x", 2, &["ab", "👨\u{200d}👩\u{200d}👧", "x"])]
    #[case::a_combining_accent_stays("abcde\u{301}fg", 5, &["abcde\u{301}", "fg"])]
    #[case::trailing_zero_width_stays("abc\u{200b}", 3, &["abc\u{200b}"])]
    #[case::empty("", 5, &[""])]
    fn rows_break(#[case] text: &str, #[case] room: u16, #[case] expected: &[&str]) {
        assert_eq!(breaks(text, room), expected);
    }

    #[test]
    fn the_last_row_of_a_line_ends_it_and_the_others_wrap() {
        let rows = wrap("abcdefghijkl", 5);
        assert_eq!(rows.iter().map(|r| r.end).collect::<Vec<_>>(), [End::Wrap, End::Wrap, End::Line]);
    }

    #[test]
    fn the_cap_ends_the_last_row_with_an_ellipsis() {
        let text = "x".repeat(10_000);
        let rows = wrap(&text, 20);
        assert_eq!(rows.len(), MAX_WRAPPED_ROWS);
        let last = &rows[MAX_WRAPPED_ROWS - 1];
        assert_eq!((last.end, last.width), (End::Cut, 19));
        assert!(rows[..MAX_WRAPPED_ROWS - 1].iter().all(|r| r.end == End::Wrap));
    }

    #[test]
    fn no_row_is_lost_or_repeated() {
        let text = "let very_long_name = some_function(arg_one, arg_two) + 编码实现 + other;";
        let rows = wrap(text, 11);
        assert_eq!(rows[0].chars.start, 0);
        assert!(rows.windows(2).all(|w| w[0].chars.end == w[1].chars.start));
        assert_eq!(rows.last().map(|r| r.chars.end), Some(text.chars().count()));
    }

    #[rstest]
    #[case::short("abc", 5)]
    #[case::exact("abcde", 5)]
    #[case::long("abcdefghijklmnopqrstuvwxyz", 5)]
    #[case::wide_chars_of_the_same_bytes("编码", 6)]
    #[case::wide_chars_over("编码实现", 6)]
    #[case::none("abc", 0)]
    fn rows_agree_with_wrap(#[case] text: &str, #[case] room: u16) {
        assert_eq!(rows(text, room), wrap(text, room).len());
    }
}

mod painting_rows {
    use super::*;

    const TEXT: &str = "abcdef ghijkl mnopqr";

    fn painted(on: &(Range<usize>, bool)) -> Vec<Vec<bool>> {
        wrap(TEXT, 8)
            .iter()
            .map(|row| {
                let code = Rect::new(2, 0, 9, 1);
                let mut buffer = Buffer::empty(Rect::new(0, 0, 12, 1));
                paint(&mut buffer, code, (TEXT, TEXT), row, on);
                (0..12).map(|x| buffer[(x, 0)].modifier.contains(Modifier::REVERSED)).collect()
            })
            .collect()
    }

    fn cells(from: usize, to: usize) -> Vec<bool> {
        (0..12).map(|x| (from..to).contains(&x)).collect()
    }

    #[test]
    fn a_selection_over_three_rows_reverses_edge_whole_and_end() {
        let rows = painted(&(2..17, false));
        assert_eq!(rows[0], cells(4, 11));
        assert_eq!(rows[1], cells(2, 11));
        assert_eq!(rows[2], cells(2, 5));
    }

    #[test]
    fn a_selection_starting_at_the_next_row_leaves_the_row_above_alone() {
        let rows = painted(&(7..9, false));
        assert_eq!(rows[0], cells(0, 0));
        assert_eq!(rows[1], cells(2, 4));
    }

    #[test]
    fn a_selected_line_break_reverses_only_the_last_row_from_its_end() {
        let rows = painted(&(20..20, true));
        assert_eq!(rows[0], cells(0, 0));
        assert_eq!(rows[1], cells(0, 0));
        assert_eq!(rows[2], cells(8, 11));
    }

    #[test]
    fn the_ellipsis_of_a_capped_line_is_reversed_when_the_hidden_rest_is_selected() {
        let text = "x".repeat(40);
        let rows = lay_out(&text, 8, 2);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 12, 1));
        paint(&mut buffer, Rect::new(2, 0, 9, 1), (&text, &text), &rows[1], &(30..40, false));
        assert!(buffer[(2 + rows[1].width, 0)].modifier.contains(Modifier::REVERSED));
    }
}
