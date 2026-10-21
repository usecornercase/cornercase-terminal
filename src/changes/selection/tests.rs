use rstest::rstest;

use super::*;
use crate::changes::diff;

const PATCH: &str = "diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,3 +1,3 @@
 one
-\ttwo
+\tTWO
 three
@@ -10,2 +10,3 @@
 ten
+eleven
 twelve
@@ -30,2 +30,1 @@
-thirty
 thirty-one
";

fn file() -> File {
    let mut file = diff::parse(PATCH).remove(0);
    diff::finish(&mut file);
    file
}

fn gap_lines(file: &File) -> Vec<GapLine> {
    let side: Vec<String> = (1..=40).map(|n| format!("\tline {n}")).collect();
    crate::changes::gap_lines(file, 1, &side)
}

fn at(hunk: usize, line: usize) -> Spot {
    Spot { hunk, at: At::Line(line) }
}

fn between(file: &File, from: Spot, to: Spot) -> Vec<(Kind, Option<u32>, Option<u32>)> {
    picked(file, |_| None, (from, to)).iter().map(|l| (l.kind, l.old, l.new)).collect()
}

mod spots {
    use super::*;

    #[test]
    fn a_hunks_unchanged_lines_come_before_its_lines_and_hunks_in_order() {
        let gap = Spot { hunk: 1, at: At::Gap(5) };
        assert!(at(0, 9) < gap);
        assert!(gap < at(1, 0));
        assert!(at(1, 7) < Spot { hunk: 2, at: At::Gap(0) });
    }

    #[test]
    fn the_range_is_ordered_whichever_end_was_pressed_first() {
        let selection = Selection { workspace: 1, path: "a.rs".into(), digest: 0, anchor: at(1, 2), end: at(0, 1) };
        assert_eq!(selection.range(), (at(0, 1), at(1, 2)));
    }
}

mod picking {
    use super::*;

    #[test]
    fn follows_the_order_shown_across_hunks() {
        let file = file();
        let found = between(&file, at(0, 1), at(1, 1));
        let expected = [
            (Kind::Removed, Some(2), None),
            (Kind::Added, None, Some(2)),
            (Kind::Context, Some(3), Some(3)),
            (Kind::Context, Some(10), Some(10)),
            (Kind::Added, None, Some(11)),
        ];
        assert_eq!(found, expected);
    }

    #[test]
    fn takes_the_unchanged_lines_between_hunks_only_when_they_are_shown() {
        let file = file();
        let gap = gap_lines(&file);
        let shown = picked(&file, |h| (h == 1).then_some(gap.as_slice()), (at(0, 3), at(1, 0)));
        assert_eq!(shown.len(), 1 + gap.len() + 1);
        assert_eq!(between(&file, at(0, 3), at(1, 0)).len(), 2);
    }

    #[test]
    fn starts_inside_a_gap() {
        let file = file();
        let gap = gap_lines(&file);
        let from = Spot { hunk: 1, at: At::Gap(2) };
        let found = picked(&file, |h| (h == 1).then_some(gap.as_slice()), (from, at(1, 0)));
        assert_eq!(found.len(), gap.len() - 2 + 1);
    }
}

mod text {
    use super::*;

    #[test]
    fn keeps_tabs_and_drops_signs_and_numbers() {
        let file = file();
        let lines = picked(&file, |_| None, (at(0, 1), at(0, 2)));
        assert_eq!(text(&lines), "\ttwo\n\tTWO");
    }

    #[test]
    fn takes_an_unchanged_lines_own_tab() {
        let file = file();
        let gap = gap_lines(&file);
        let from = Spot { hunk: 1, at: At::Gap(0) };
        let lines = picked(&file, |h| (h == 1).then_some(gap.as_slice()), (from, from));
        assert_eq!(text(&lines), "\tline 4");
    }

    #[test]
    fn has_no_newline_at_the_end() {
        let file = file();
        assert_eq!(text(&picked(&file, |_| None, (at(0, 0), at(0, 0)))), "one");
    }
}

mod reference {
    use super::*;

    #[rstest]
    #[case::one_line(at(0, 2), at(0, 2), "a.rs:2 ")]
    #[case::a_range(at(0, 0), at(0, 3), "a.rs:1-3 ")]
    #[case::removed_lines_are_ignored_for_the_numbers(at(0, 1), at(0, 3), "a.rs:2-3 ")]
    #[case::across_hunks(at(0, 3), at(1, 1), "a.rs:3-11 ")]
    fn uses_the_new_numbers(#[case] from: Spot, #[case] to: Spot, #[case] expected: &str) {
        let file = file();
        let lines = picked(&file, |_| None, (from, to));
        assert_eq!(reference(&file, from.hunk, &lines), expected);
    }

    #[test]
    fn only_removed_lines_get_the_hunks_reference() {
        let file = file();
        let lines = picked(&file, |_| None, (at(0, 1), at(0, 1)));
        assert_eq!(reference(&file, 0, &lines), "a.rs:2 ");
        let lines = picked(&file, |_| None, (at(2, 0), at(2, 0)));
        assert_eq!(reference(&file, 2, &lines), "a.rs:30 ");
    }

    #[rstest]
    #[case::new_numbers(at(0, 0), at(0, 3), Some((1, 3, false)))]
    #[case::removed_only(at(0, 1), at(0, 1), Some((2, 2, true)))]
    fn numbers_name_the_lines(#[case] from: Spot, #[case] to: Spot, #[case] expected: Option<(u32, u32, bool)>) {
        let file = file();
        assert_eq!(numbers(&picked(&file, |_| None, (from, to))), expected);
    }

    #[test]
    fn places_a_range() {
        assert_eq!((place("a", 3, 3), place("a", 3, 5)), ("a:3 ".to_string(), "a:3-5 ".to_string()));
    }
}
