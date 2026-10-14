use rstest::rstest;

use super::*;

mod display_path;
mod folder_name;
mod truncate_cells;
mod truncate_left;
mod truncate_right;

#[test]
fn one_line_joins_lines_and_spaces() {
    assert_eq!(one_line(" a\n  b\tc "), "a b c");
}
