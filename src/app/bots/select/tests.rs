use super::*;

fn cell(up: usize, col: u16) -> Cell {
    Cell { up, col }
}

mod the_cells_of_a_row {
    use super::*;

    #[test]
    fn a_drag_on_one_row_takes_both_ends() {
        let selection = Selection { anchor: cell(3, 8), head: cell(3, 2) };

        assert_eq!(selection.cols(3), Some(2..9));
        assert_eq!(selection.cols(2), None);
        assert_eq!(selection.cols(4), None);
    }

    #[test]
    fn a_drag_down_runs_from_the_press_to_the_end_of_its_row_and_on_to_the_pointer() {
        let selection = Selection { anchor: cell(5, 4), head: cell(3, 6) };

        assert_eq!(selection.cols(5), Some(4..u16::MAX));
        assert_eq!(selection.cols(4), Some(0..u16::MAX));
        assert_eq!(selection.cols(3), Some(0..7));
    }

    #[test]
    fn a_drag_up_is_the_same_stream_read_from_the_pointer() {
        let down = Selection { anchor: cell(5, 4), head: cell(3, 6) };
        let up = Selection { anchor: cell(3, 6), head: cell(5, 4) };

        assert_eq!(
            (3..=5).map(|row| up.cols(row)).collect::<Vec<_>>(),
            (3..=5).map(|row| down.cols(row)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn new_rows_below_move_it_up_with_the_text() {
        let selection = Selection { anchor: cell(0, 1), head: cell(2, 3) }.shifted(4);

        assert_eq!(selection, Selection { anchor: cell(4, 1), head: cell(6, 3) });
    }
}

mod the_copied_text {
    use super::*;

    fn rows(texts: &[&str]) -> impl Iterator<Item = String> {
        texts.iter().map(ToString::to_string).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn rows_are_joined_by_line_breaks_without_their_trailing_blanks() {
        assert_eq!(joined(rows(&["prs · 09:12   ", "Two are blocked.  "])), "prs · 09:12\nTwo are blocked.");
    }

    #[test]
    fn the_indent_every_row_shares_goes_and_the_rest_stays() {
        assert_eq!(joined(rows(&["      fn main() {", "          run();", "      }"])), "fn main() {\n    run();\n}");
    }

    #[test]
    fn a_bubble_s_half_block_edges_and_the_blank_rows_around_are_left_out() {
        let bubble = ["        ▄▄▄▄▄▄▄▄▄▄", "          hi there 09:10  ", "        ▀▀▀▀▀▀▀▀▀▀", ""];

        assert_eq!(joined(rows(&bubble)), "hi there 09:10");
    }
}
