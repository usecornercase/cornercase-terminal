use ratatui::style::Color;
use rstest::rstest;

use super::*;

const PATCH: &str = "diff --git a/src/address.rs b/src/address.rs
index 1111111..2222222 100644
--- a/src/address.rs
+++ b/src/address.rs
@@ -9,4 +9,4 @@ fn first_line
 /// The first line of the address.
-pub fn first_line(lines: &[String]) -> &str {
-    lines.first().unwrap()
+pub fn first_line(lines: &[String]) -> Option<&str> {
+    lines.first().map(String::as_str)
 }
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
index 3333333..4444444 100644
--- a/old name.txt\t
+++ b/new name.txt\t
@@ -1 +1,2 @@
 kept
+added
diff --git a/logo.png b/logo.png
index 5555555..6666666 100644
Binary files a/logo.png and b/logo.png differ
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 7777777..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-one
-two
";

fn parsed() -> Vec<File> {
    parse(PATCH)
}

mod parse {
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn reads_every_file_of_the_patch() {
        let paths: Vec<(String, Status)> = parsed().into_iter().map(|f| (f.path, f.status)).collect();
        assert_eq!(
            paths,
            [
                ("src/address.rs".to_string(), Status::Modified),
                ("new name.txt".to_string(), Status::Renamed),
                ("logo.png".to_string(), Status::Modified),
                ("gone.txt".to_string(), Status::Deleted),
            ]
        );
    }

    #[test]
    fn counts_added_and_removed_lines() {
        let counts: Vec<(usize, usize)> = parsed().iter().map(|f| (f.added, f.removed)).collect();
        assert_eq!(counts, [(2, 2), (1, 0), (0, 0), (0, 2)]);
    }

    #[test]
    fn numbers_lines_on_both_sides() {
        let numbers: Vec<(Option<u32>, Option<u32>)> =
            parsed()[0].hunks[0].lines.iter().map(|l| (l.old, l.new)).collect();
        assert_eq!(
            numbers,
            [
                (Some(9), Some(9)),
                (Some(10), None),
                (Some(11), None),
                (None, Some(10)),
                (None, Some(11)),
                (Some(12), Some(12))
            ]
        );
    }

    #[test]
    fn keeps_the_function_of_each_hunk() {
        assert_eq!(parsed()[0].hunks[0].context, "fn first_line");
    }

    #[test]
    fn a_renamed_file_keeps_its_old_name() {
        assert_eq!(parsed()[1].old_path.as_deref(), Some("old name.txt"));
    }

    #[test]
    fn binary_files_are_folded() {
        assert_eq!(parsed()[2].fold, Fold::Binary);
    }

    #[test]
    fn lockfiles_are_folded_but_keep_their_lines() {
        let patch =
            "diff --git a/Cargo.lock b/Cargo.lock\n--- a/Cargo.lock\n+++ b/Cargo.lock\n@@ -1 +1 @@\n-a\n+b\n";
        let file = &parse(patch)[0];
        assert_eq!((file.fold, file.hunks.len()), (Fold::Lockfile, 1));
    }

    #[test]
    fn huge_files_drop_their_lines() {
        let lines = (0..=MAX_LINES).fold(String::new(), |mut lines, i| {
            let _ = writeln!(lines, "+{i}");
            lines
        });
        let patch =
            format!("diff --git a/big b/big\n--- /dev/null\n+++ b/big\n@@ -0,0 +1,{} @@\n{lines}", MAX_LINES + 1);
        let file = &parse(&patch)[0];
        assert_eq!((file.fold, file.added, file.hunks.len()), (Fold::Large, MAX_LINES + 1, 0));
    }

    #[test]
    fn expands_tabs_and_drops_carriage_returns() {
        let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-\tx\r\n+\ty\r\n";
        assert_eq!(parse(patch)[0].hunks[0].lines[1].text, "    y");
    }

    #[test]
    fn reads_quoted_paths() {
        let patch = "diff --git \"a/caf\\303\\251.txt\" \"b/caf\\303\\251.txt\"\n--- \"a/caf\\303\\251.txt\"\n+++ \"b/caf\\303\\251.txt\"\n@@ -1 +1 @@\n-a\n+b\n";
        assert_eq!(parse(patch)[0].path, "café.txt");
    }

    #[test]
    fn takes_the_path_from_the_header_when_there_are_no_lines() {
        let patch = "diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n";
        assert_eq!(parse(patch)[0].path, "run.sh");
    }

    #[test]
    fn the_same_section_has_the_same_digest() {
        assert_eq!(parse(PATCH)[0].digest, parsed()[0].digest);
        assert_ne!(parsed()[0].digest, parsed()[1].digest);
    }
}

mod untracked {
    use super::*;

    #[test]
    fn every_line_is_added() {
        let file = untracked("notes.md", 4, Some(b"a\nb\n"));
        let numbers: Vec<Option<u32>> = file.hunks[0].lines.iter().map(|l| l.new).collect();
        assert_eq!((file.status, file.added, numbers), (Status::Untracked, 2, vec![Some(1), Some(2)]));
    }

    #[test]
    fn a_file_with_nul_bytes_is_binary() {
        assert_eq!(untracked("blob", 3, Some(b"a\0b")).fold, Fold::Binary);
    }

    #[test]
    fn a_big_file_is_not_read() {
        assert_eq!(untracked("dump.sql", MAX_UNTRACKED_BYTES + 1, None).fold, Fold::Large);
    }
}

mod words {
    use super::*;

    #[rstest]
    #[case::return_type("fn f() -> &str {", "fn f() -> Option<&str> {", vec![], vec![10..17, 21..22])]
    #[case::method("lines.first().unwrap()", "lines.first().map(as_str)", vec![14..20], vec![14..17, 18..24])]
    fn marks_the_words_that_changed(
        #[case] old: &str,
        #[case] new: &str,
        #[case] old_ranges: Vec<Range<usize>>,
        #[case] new_ranges: Vec<Range<usize>>,
    ) {
        let (o, n) = words(old, new).expect("similar lines");
        assert_eq!((o, n), (old_ranges, new_ranges));
    }

    #[test]
    fn different_lines_are_not_paired() {
        assert_eq!(words("let total = 1;", "fn render(frame: &mut Frame) {}"), None);
    }

    #[test]
    fn pairs_each_removed_line_with_a_similar_added_line() {
        let mut file = parsed().remove(0);
        finish(&mut file);
        let emphasis: Vec<bool> = file.hunks[0].lines.iter().map(|l| !l.emphasis.is_empty()).collect();
        assert_eq!(emphasis, [false, false, true, true, true, false]);
    }
}

#[test]
fn highlights_both_sides_of_a_hunk() {
    let mut file = parsed().remove(0);
    finish(&mut file);
    let keyword = |line: &Line| {
        line.syntax.as_ref().and_then(|s| s.iter().find(|(t, _)| t == "fn")).and_then(|(_, style)| style.fg)
    };
    let lines = &file.hunks[0].lines;
    assert_eq!((keyword(&lines[1]), keyword(&lines[3])), (Some(Color::Magenta), Some(Color::Magenta)));
}

mod hunk {
    use super::*;

    #[test]
    fn knows_where_it_ends_on_each_side() {
        let hunk = &parsed()[0].hunks[0];
        assert_eq!((hunk.old_end(), hunk.new_end()), (13, 13));
    }

    #[test]
    fn changed_lines_are_the_added_ones() {
        assert_eq!(parsed()[0].hunks[0].changed(), (10, 11));
    }

    #[test]
    fn a_pure_removal_points_at_where_the_lines_were() {
        assert_eq!(parsed()[3].hunks[0].changed(), (1, 1));
    }

    #[test]
    fn copies_as_a_patch() {
        let hunk = &parse(PATCH)[1].hunks[0];
        assert_eq!(hunk.patch(), "@@ -1,1 +1,2 @@\n kept\n+added\n");
    }
}
