use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::args::{Cli, Command, Control};
use crate::control::changes::{HunkInfo, LineInfo, WorkspaceChanges};

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

mod parsing {
    use super::*;

    #[rstest]
    #[case::here(&["changes", "list"])]
    #[case::every_following(&["changes", "list", "--every", "--follow"])]
    #[case::a_branch_in_commits(&["changes", "list", "--worktree", "fix/login", "--mode", "commits", "--json"])]
    #[case::a_file(&["changes", "file", "src/main.rs", "--mode", "all"])]
    #[case::a_gap(&["changes", "gap", "src/main.rs", "--hunk", "2"])]
    #[case::bases(&["changes", "base"])]
    #[case::a_base(&["changes", "base", "origin/main", "--workspace", "3"])]
    #[case::the_default_base(&["changes", "base", "--default"])]
    #[case::viewed(&["changes", "viewed", "src/main.rs"])]
    #[case::unviewed(&["changes", "viewed", "src/main.rs", "--clear"])]
    #[case::discard(&["changes", "discard", "src/main.rs"])]
    #[case::discard_without_asking(&["changes", "discard", "src/main.rs", "--yes", "--workspace", "3"])]
    #[case::discard_in_a_branch(&["changes", "discard", "src/main.rs", "--worktree", "x"])]
    #[case::stage(&["changes", "stage", "a.rs"])]
    #[case::a_merge_check(&["changes", "merge-check"])]
    #[case::a_merge_check_with_a_workspace(&["changes", "merge-check", "--with", "12", "--worktree", "x", "--json"])]
    #[case::a_merge_check_into_a_branch(&["changes", "merge-check", "--base", "main"])]
    #[case::shared_files(&["changes", "shared", "--workspace", "3", "--json"])]
    #[case::unstage_in_a_workspace(&["changes", "unstage", "a.rs", "--workspace", "3"])]
    #[case::stage_in_a_branch(&["changes", "stage", "a.rs", "--worktree", "x"])]
    #[case::viewed_as_seen(&["changes", "viewed", "src/main.rs", "--digest", "550a7a517a389403"])]
    #[case::stage_as_seen(&["changes", "stage", "a.rs", "--digest", "550A7A517A389403"])]
    #[case::unstage_as_seen(&["changes", "unstage", "a.rs", "--digest", "550a7a517a389403", "--worktree", "x"])]
    #[case::discard_as_seen(&["changes", "discard", "a.rs", "--digest", "550a7a517a389403", "--yes"])]
    fn these_are_taken(#[case] args: &[&str]) {
        let parsed = parse(args).expect("parse");

        assert!(matches!(parsed.command, Some(Command::Control(Control::Changes(_)))));
    }

    #[rstest]
    #[case::every_with_a_workspace(&["changes", "list", "--every", "--workspace", "1"])]
    #[case::every_with_a_mode(&["changes", "list", "--every", "--mode", "commits"])]
    #[case::two_places(&["changes", "list", "--workspace", "1", "--worktree", "x"])]
    #[case::default_with_a_branch(&["changes", "base", "main", "--default"])]
    #[case::a_gap_without_its_hunk(&["changes", "gap", "a.rs"])]
    #[case::a_merge_check_with_and_base(&["changes", "merge-check", "--with", "12", "--base", "main"])]
    #[case::a_merge_check_with_a_word(&["changes", "merge-check", "--with", "x"])]
    #[case::a_mode_command(&["changes", "mode", "--stream", "1", "commits"])]
    #[case::an_unknown_mode(&["changes", "list", "--mode", "some"])]
    #[case::discard_in_a_mode(&["changes", "discard", "a.rs", "--mode", "commits"])]
    #[case::discard_without_a_path(&["changes", "discard"])]
    #[case::stage_without_a_path(&["changes", "stage"])]
    #[case::stage_in_a_mode(&["changes", "stage", "a.rs", "--mode", "commits"])]
    #[case::stage_in_two_places(&["changes", "stage", "a.rs", "--workspace", "1", "--worktree", "x"])]
    #[case::discard_in_two_places(&["changes", "discard", "a.rs", "--workspace", "1", "--worktree", "x"])]
    #[case::a_short_digest(&["changes", "stage", "a.rs", "--digest", "550a7a"])]
    #[case::a_digest_not_in_hex(&["changes", "discard", "a.rs", "--digest", "zzzzzzzzzzzzzzzz", "--yes"])]
    #[case::a_digest_without_a_value(&["changes", "unstage", "a.rs", "--digest"])]
    #[case::clearing_with_a_digest(&["changes", "viewed", "a.rs", "--clear", "--digest", "550a7a517a389403"])]
    #[case::a_digest_in_a_mode(&["changes", "viewed", "a.rs", "--mode", "commits", "--digest", "550a7a517a389403"])]
    fn these_are_refused(#[case] args: &[&str]) {
        assert!(parse(args).is_err());
    }
}

fn file(path: &str, status: FileStatus, fold: FoldClass, viewed: bool) -> FileInfo {
    FileInfo { path: path.into(), status, fold, viewed, added: 4, removed: 3, ..FileInfo::default() }
}

mod text {
    use super::*;

    #[test]
    fn a_list_names_each_file_with_its_marks() {
        let renamed = FileInfo {
            old_path: Some("src/old.rs".into()),
            ..file("src/new.rs", FileStatus::Renamed, FoldClass::Open, false)
        };
        let content = Content {
            state: State::Ready,
            workspaces: vec![WorkspaceChanges {
                project: 1,
                workspace: 2,
                base: None,
                added: 12,
                removed: 9,
                files: vec![
                    file("Cargo.lock", FileStatus::Modified, FoldClass::Lockfile, true),
                    renamed,
                    file("notes.txt", FileStatus::Untracked, FoldClass::Open, false),
                ],
            }],
            ..Content::default()
        };

        let text = list_text(&content, None);

        assert_eq!(
            text,
            "workspace 2  uncommitted  3 files +12 −9\n\
             M  Cargo.lock  +4 −3  lockfile, viewed\n\
             R  src/old.rs → src/new.rs  +4 −3\n\
             A  notes.txt  +4 −3  untracked\n"
        );
    }

    #[test]
    fn a_list_tags_staged_files() {
        let staged =
            |staged| FileInfo { staged: Some(staged), ..file("a.rs", FileStatus::Modified, FoldClass::Open, true) };
        let content = Content {
            state: State::Ready,
            workspaces: vec![WorkspaceChanges {
                workspace: 2,
                files: vec![staged(Staging::Yes), staged(Staging::Partly), staged(Staging::No)],
                ..WorkspaceChanges::default()
            }],
            ..Content::default()
        };

        let text = list_text(&content, None);

        assert!(text.contains("+4 −3  staged, viewed\n"), "{text}");
        assert!(text.contains("+4 −3  partly staged, viewed\n"), "{text}");
        assert_eq!(text.matches("+4 −3  viewed\n").count(), 1, "{text}");
    }

    #[test]
    fn a_failed_list_is_an_error_naming_the_cause_and_a_ready_one_is_not() {
        let failed = Content { state: State::Failed, error: Some("not a git repository".into()), ..Content::default() };

        let error = failure(&failed).expect("an error");

        assert_eq!(error.to_string(), "cannot read the changes: not a git repository");
        assert!(failure(&Content { state: State::Ready, ..Content::default() }).is_none());
    }

    #[test]
    fn an_empty_ready_list_says_so() {
        assert_eq!(list_text(&Content { state: State::Ready, ..Content::default() }, None), "no changes\n");
    }

    fn line(kind: LineKind, text: &str) -> LineInfo {
        LineInfo { kind, text: text.into(), ..LineInfo::default() }
    }

    #[test]
    fn a_patch_prints_as_a_unified_diff() {
        let patch = Patch {
            file: file("src/a.rs", FileStatus::Modified, FoldClass::Open, false),
            hunks: vec![HunkInfo {
                old_start: 3,
                new_start: 3,
                context: "fn main() {".into(),
                lines: vec![line(LineKind::Context, "a"), line(LineKind::Removed, "b"), line(LineKind::Added, "c")],
            }],
            ..Patch::default()
        };

        assert_eq!(patch_text(&patch), "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -3,2 +3,2 @@ fn main() {\n a\n-b\n+c\n");
    }

    #[test]
    fn a_binary_or_large_file_says_why_it_has_no_lines() {
        let binary = Patch { file: file("p.bin", FileStatus::Added, FoldClass::Binary, false), ..Patch::default() };
        let large = Patch { file: file("big.sql", FileStatus::Added, FoldClass::Large, false), ..Patch::default() };

        assert_eq!(patch_text(&binary), "p.bin: binary file\n");
        assert_eq!(patch_text(&large), "big.sql: too large to show (7 lines changed)\n");
    }
}

mod the_help {
    use serde_json::Value;

    use super::*;
    use crate::test_util::fixture;

    fn of(action: &str) -> String {
        parse(&["changes", action, "--help"]).expect_err("the help").to_string()
    }

    #[rstest]
    #[case::of_a_file("file", fixture!("control/changes/v1/patch.json"), "/hunks/0/lines")]
    #[case::of_a_gap("gap", fixture!("control/changes/v1/gap.json"), "/lines")]
    fn names_every_field_of_a_line(#[case] action: &str, #[case] pinned: &str, #[case] lines: &str) {
        let pinned: Value = serde_json::from_str(pinned).expect("the answer");
        let lines = pinned.pointer(lines).and_then(Value::as_array).expect("its lines");
        let help = of(action);

        for field in lines.iter().flat_map(|line| line.as_object().expect("a line").keys()) {
            assert!(help.contains(&format!("\"{field}\"")), "{field} in {help}");
        }
    }

    #[test]
    fn of_a_file_says_what_to_copy() {
        assert!(of("file").contains("take its raw, or its text when it has none"));
    }
}

mod the_merge_check_text {
    use super::*;
    use crate::control::changes::{ConflictInfo, MergeChecked, MergeInfo};
    use crate::control::{ProjectInfo, WorkspaceInfo};

    fn conflict(path: &str, kind: &str) -> ConflictInfo {
        ConflictInfo { path: path.into(), kind: kind.into(), message: String::new() }
    }

    fn report() -> Report {
        let workspace = WorkspaceInfo { id: 12, name: "feat/dark-mode".into(), ..WorkspaceInfo::default() };
        Report {
            projects: vec![ProjectInfo { workspaces: vec![workspace], ..ProjectInfo::default() }],
            ..Report::default()
        }
    }

    #[test]
    fn says_a_clean_merge_into_the_base() {
        let clean = MergeInfo { base: Some("origin/main".into()), clean: true, ..MergeInfo::default() };

        assert_eq!(
            merge_text(&MergeChecked { workspace: 11, checks: vec![clean] }, None),
            "into origin/main  merges cleanly\n"
        );
    }

    #[test]
    fn names_each_conflicting_file_under_the_workspace() {
        let found =
            MergeInfo { with: Some(12), conflicts: vec![conflict("src/theme.rs", "contents")], ..MergeInfo::default() };

        let text = merge_text(&MergeChecked { workspace: 11, checks: vec![found] }, Some(&report()));

        assert_eq!(text, "with workspace 12 feat/dark-mode  1 conflict\n  src/theme.rs  contents\n");
    }

    #[test]
    fn says_why_a_check_could_not_run_and_what_it_left_out() {
        let failed =
            MergeInfo { base: Some("nope".into()), error: Some("no branch nope".into()), ..MergeInfo::default() };
        let large = MergeInfo { with: Some(12), clean: true, left_out: 2, ..MergeInfo::default() };

        let text = merge_text(&MergeChecked { workspace: 11, checks: vec![failed, large] }, None);

        assert_eq!(
            text,
            "into nope  could not check: no branch nope\nwith workspace 12  merges cleanly\n  2 untracked left out (over 1 MiB, or past 8 MiB in all)\n"
        );
    }
}
