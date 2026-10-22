use rstest::rstest;

use super::*;

fn at(line: u32, from: usize, to: usize) -> Stretch<u32> {
    Stretch::within(line, from..to)
}

fn across(from: (u32, usize), to: (u32, usize)) -> Stretch<u32> {
    Stretch { from: Point { line: from.0, offset: from.1 }, to: Point { line: to.0, offset: to.1 } }
}

mod union {
    use super::*;

    #[rstest]
    #[case::anchor_first(at(1, 2, 3), at(1, 5, 6), at(1, 2, 6))]
    #[case::pointer_first(at(1, 5, 6), at(1, 2, 3), at(1, 2, 6))]
    #[case::across_lines(at(3, 4, 5), at(1, 2, 3), across((1, 2), (3, 5)))]
    #[case::inside(at(1, 2, 8), at(1, 3, 4), at(1, 2, 8))]
    fn orders_the_anchor_and_the_pointer(#[case] a: Stretch<u32>, #[case] b: Stretch<u32>, #[case] hull: Stretch<u32>) {
        assert_eq!(a.union(b), hull);
        assert_eq!(b.union(a), hull);
    }
}

mod on {
    use super::*;

    #[rstest]
    #[case::before(1, 10, None)]
    #[case::first(2, 10, Some((3..10, true)))]
    #[case::middle(3, 10, Some((0..10, true)))]
    #[case::last(4, 10, Some((0..2, false)))]
    #[case::after(5, 10, None)]
    #[case::clamped_to_the_length(2, 2, Some((2..2, true)))]
    fn gives_the_chars_of_one_line(
        #[case] line: u32,
        #[case] len: usize,
        #[case] expected: Option<(Range<usize>, bool)>,
    ) {
        assert_eq!(across((2, 3), (4, 2)).on(line, len), expected);
    }

    #[test]
    fn a_single_line_has_no_line_break() {
        assert_eq!(at(7, 1, 4).on(7, 9), Some((1..4, false)));
    }

    #[test]
    fn a_last_line_with_nothing_selected_is_left_out() {
        assert_eq!(across((2, 3), (4, 0)).on(4, 9), None);
    }

    #[test]
    fn an_empty_middle_line_keeps_its_break() {
        assert_eq!(across((2, 3), (4, 1)).on(3, 0), Some((0..0, true)));
    }
}

mod ends_before {
    use super::*;

    #[rstest]
    #[case::at_the_start_of_a_later_line(across((1, 2), (3, 0)), true)]
    #[case::inside_a_later_line(across((1, 2), (3, 1)), false)]
    #[case::one_line_from_the_start(at(1, 0, 0), false)]
    fn is_true_only_for_a_stretch_ending_at_a_line_start(#[case] stretch: Stretch<u32>, #[case] expected: bool) {
        assert_eq!(stretch.ends_before(), expected);
    }

    #[test]
    fn lines_drop_the_last_line_that_holds_nothing() {
        assert_eq!(across((1, 2), (3, 0)).lines(), (1, 2));
        assert_eq!(across((1, 2), (3, 4)).lines(), (1, 3));
        assert_eq!(at(5, 1, 3).lines(), (5, 5));
    }
}

mod text {
    use super::*;

    #[rstest]
    #[case::one_line(at(0, 1, 4), "bcd")]
    #[case::lines_joined(across((0, 3), (2, 2)), "de\nfghi\njk")]
    #[case::ending_at_a_line_start(across((0, 3), (2, 0)), "de\nfghi\n")]
    #[case::an_end_past_the_length_takes_the_rest(at(0, 3, 99), "de")]
    #[case::a_start_past_the_length_takes_nothing(at(0, 99, 120), "")]
    fn takes_the_chars(#[case] stretch: Stretch<u32>, #[case] expected: &str) {
        assert_eq!(stretch.text(["abcde", "fghi", "jklm"].into_iter().take((stretch.to.line + 1) as usize)), expected);
    }

    #[test]
    fn counts_chars_not_bytes() {
        assert_eq!(at(0, 1, 3).text(["éa漢b"]), "a漢");
    }

    #[test]
    fn a_tab_stays_a_tab() {
        assert_eq!(at(0, 0, 2).text(["\tx"]), "\tx");
    }
}

mod dragged {
    use super::*;

    fn dragged(mut current: Option<Stretch<u32>>, anchor: Stretch<u32>, pointer: Stretch<u32>) -> Option<Stretch<u32>> {
        Stretch::dragged(&mut current, anchor, pointer);
        current
    }

    #[test]
    fn a_pointer_still_on_the_anchor_changes_nothing() {
        assert_eq!(dragged(None, at(1, 2, 3), at(1, 2, 3)), None);
    }

    #[test]
    fn moving_selects_the_hull() {
        assert_eq!(dragged(None, at(1, 2, 3), at(1, 5, 6)), Some(at(1, 2, 6)));
    }

    #[test]
    fn coming_back_to_the_anchor_keeps_its_char() {
        assert_eq!(dragged(Some(at(1, 2, 6)), at(1, 2, 3), at(1, 2, 3)), Some(at(1, 2, 3)));
    }

    #[test]
    fn an_empty_hull_goes_back_to_the_line() {
        assert_eq!(dragged(Some(at(1, 4, 6)), at(1, 4, 4), at(1, 4, 4)), None);
    }

    #[test]
    fn an_empty_hull_with_nothing_selected_changes_nothing() {
        assert_eq!(dragged(None, at(1, 4, 4), at(1, 4, 4)), None);
    }
}

mod tabs {
    use super::*;

    #[rstest]
    #[case::a_cell_inside_a_tab("\tab", 2..3, 0..1)]
    #[case::the_char_after_a_tab("\tab", 4..5, 1..2)]
    #[case::the_whole_line("\tab", 0..6, 0..3)]
    #[case::an_empty_range_at_a_tab_start("a\tb", 1..1, 1..1)]
    #[case::an_empty_range_inside_a_tab("a\tb", 3..3, 1..1)]
    #[case::past_the_end("ab", 5..9, 2..2)]
    fn source_range_maps_shown_chars_to_the_file_s_chars(
        #[case] raw: &str,
        #[case] shown: Range<usize>,
        #[case] expected: Range<usize>,
    ) {
        assert_eq!(source_range(raw, shown), expected);
    }

    #[rstest]
    #[case("\tab", 0..1, 0..4)]
    #[case("\tab", 1..3, 4..6)]
    #[case("ab", 0..2, 0..2)]
    fn shown_range_is_the_reverse(#[case] raw: &str, #[case] chars: Range<usize>, #[case] expected: Range<usize>) {
        assert_eq!(shown_range(raw, chars.clone()), expected);
        assert_eq!(source_range(raw, expected), chars);
    }
}
