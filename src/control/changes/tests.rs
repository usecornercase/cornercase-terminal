use serde_json::json;

use super::*;
use crate::changes::diff::{Fold, Hunk, Kind, Line, Segments, Status, finish, parse};
use crate::changes::gap_lines;
use crate::test_util::fixture;

fn info(path: &str, status: FileStatus, fold: FoldClass, viewed: bool) -> FileInfo {
    FileInfo {
        path: path.into(),
        old_path: None,
        status,
        added: 4,
        removed: 3,
        fold,
        viewed,
        staged: None,
        digest: digest(0x00c0_ffee_1234_5678),
    }
}

fn summary() -> Summary {
    let renamed = FileInfo {
        old_path: Some("src/old.rs".into()),
        ..info("src/new.rs", FileStatus::Renamed, FoldClass::Open, false)
    };
    let files = vec![
        info("Cargo.lock", FileStatus::Modified, FoldClass::Lockfile, true),
        renamed,
        info("notes.txt", FileStatus::Untracked, FoldClass::Open, false),
    ];
    Summary {
        boot: "4242-1700000000".into(),
        stream: 1,
        revision: 2,
        content: Content {
            scope: Scope::Project,
            mode: Mode::Commits,
            state: State::Ready,
            error: None,
            workspaces: vec![WorkspaceChanges {
                project: 1,
                workspace: 2,
                base: Some("main".into()),
                added: 12,
                removed: 9,
                files,
            }],
        },
    }
}

fn every() -> Summary {
    let one = |project, workspace, path: &str, staged| WorkspaceChanges {
        project,
        workspace,
        base: None,
        added: 4,
        removed: 3,
        files: vec![FileInfo { staged: Some(staged), ..info(path, FileStatus::Modified, FoldClass::Open, false) }],
    };
    Summary {
        boot: "4242-1700000000".into(),
        stream: 2,
        revision: 5,
        content: Content {
            scope: Scope::Every,
            mode: Mode::Uncommitted,
            state: State::Ready,
            error: None,
            workspaces: vec![
                one(1, 2, "src/a.rs", Staging::Yes),
                one(1, 3, "src/b.rs", Staging::Partly),
                one(4, 5, "lib.rs", Staging::No),
            ],
        },
    }
}

fn file() -> File {
    let line = |kind, old, new, text: &str| Line {
        kind,
        old,
        new,
        text: text.into(),
        raw: None,
        syntax: Some(vec![(text.into(), Look::default().fg(ratatui::style::Color::Indexed(5)))]),
        emphasis: Vec::new(),
    };
    let mut removed = line(Kind::Removed, Some(2), None, "    say(1);");
    removed.emphasis = std::iter::once(8..9).collect();
    let mut added = line(Kind::Added, None, Some(2), "    say(2);");
    added.emphasis = std::iter::once(8..9).collect();
    added.syntax = None;
    let mut tabbed = line(Kind::Context, Some(3), Some(3), "    done();");
    tabbed.raw = Some("\tdone();".into());
    File {
        path: "src/a.rs".into(),
        old_path: None,
        status: Status::Modified,
        added: 1,
        removed: 1,
        fold: Fold::Open,
        hunks: vec![Hunk {
            old_start: 1,
            new_start: 1,
            context: "fn main() {".into(),
            lines: vec![line(Kind::Context, Some(1), Some(1), "fn main() {"), removed, added, tabbed],
        }],
        digest: 7,
        viewed_digest: 0x00c0_ffee_1234_5678,
        staged: None,
    }
}

fn patch() -> Patch {
    Patch::of(2, Mode::Uncommitted, &file(), false)
}

fn gap() -> Gap {
    let plain = GapLine { old: 3, new: 4, text: "let x = 1;".into(), raw: None, syntax: None };
    let look = Look::default().fg(ratatui::style::Color::Indexed(5));
    let tabbed = GapLine {
        old: 4,
        new: 5,
        text: "    run();".into(),
        raw: Some("\trun();".into()),
        syntax: Some(vec![("    run();".into(), look)]),
    };
    Gap::of(2, "src/a.rs", 1, &[plain, tabbed])
}

mod fixtures {
    use super::*;

    #[test]
    fn a_summary_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&summary()).expect("json");
        let read: Summary = serde_json::from_str(fixture!("control/changes/v1/summary.json")).expect("a summary");

        assert_eq!(written, fixture!("control/changes/v1/summary.json").trim_end());
        assert_eq!(read, summary());
    }

    #[test]
    fn an_every_project_summary_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&every()).expect("json");
        let read: Summary = serde_json::from_str(fixture!("control/changes/v1/every.json")).expect("a summary");

        assert_eq!(written, fixture!("control/changes/v1/every.json").trim_end());
        assert_eq!(read, every());
    }

    #[test]
    fn a_patch_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&patch()).expect("json");
        let read: Patch = serde_json::from_str(fixture!("control/changes/v1/patch.json")).expect("a patch");

        assert_eq!(written, fixture!("control/changes/v1/patch.json").trim_end());
        assert_eq!(read, patch());
    }

    #[test]
    fn a_gap_is_written_as_its_fixture_and_reads_back() {
        let pinned = fixture!("control/changes/v1/gap.json");

        assert_eq!(serde_json::to_string_pretty(&gap()).expect("json"), pinned.trim_end());
        assert_eq!(serde_json::from_str::<Gap>(pinned).expect("a gap"), gap());
    }

    #[test]
    fn a_summary_is_streamed_as_an_ok_line() {
        let text = summary().streamed().expect("a line");
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");

        assert_eq!(value["ok"]["stream"], 1);
        assert_eq!(value["ok"]["mode"], "commits");
        assert_eq!(value["ok"]["workspaces"][0]["files"][0]["fold"], "lockfile");
    }
}

mod requests {
    use super::*;

    #[test]
    fn every_request_serializes_all_its_keys() {
        let keys = |value: serde_json::Value| -> Vec<String> {
            value.as_object().expect("an object").keys().cloned().collect()
        };

        assert_eq!(keys(serde_json::to_value(Changes::default()).expect("json")), ["mode", "scope", "workspace"]);
        assert_eq!(keys(serde_json::to_value(ChangesMode::default()).expect("json")), ["mode", "stream"]);
        assert_eq!(keys(serde_json::to_value(ChangesFile::default()).expect("json")), ["mode", "path", "workspace"]);
        assert_eq!(
            keys(serde_json::to_value(ChangesGap::default()).expect("json")),
            ["hunk", "mode", "path", "workspace"]
        );
        assert_eq!(keys(serde_json::to_value(ChangesBase::default()).expect("json")), ["base", "clear", "workspace"]);
        assert_eq!(
            keys(serde_json::to_value(ChangesViewed::default()).expect("json")),
            ["clear", "digest", "path", "workspace"]
        );
        assert_eq!(
            keys(serde_json::to_value(ChangesDiscard::default()).expect("json")),
            ["digest", "path", "workspace"]
        );
        assert_eq!(
            keys(serde_json::to_value(ChangesStage::default()).expect("json")),
            ["digest", "path", "staged", "workspace"]
        );
    }

    #[test]
    fn staging_is_the_default_of_a_stage_request() {
        let request: ChangesStage = serde_json::from_value(json!({})).expect("a request");

        assert!(request.staged);
        assert!(!ChangesStage::default().staged);
    }

    #[test]
    fn the_defaults_are_the_project_scope_and_uncommitted_changes() {
        let request: Changes = serde_json::from_value(json!({})).expect("a request");

        assert_eq!((request.scope, request.mode, request.workspace), (Scope::Project, Mode::Uncommitted, None));
    }
}

mod discarded {
    use super::*;

    #[test]
    fn the_answer_survives_a_round_trip() {
        let answer = Discarded {
            workspace: 3,
            path: "b.txt".into(),
            old_path: Some("a.txt".into()),
            status: FileStatus::Renamed,
        };

        let text = serde_json::to_string(&answer).expect("json");

        assert_eq!(serde_json::from_str::<Discarded>(&text).expect("answer"), answer);
        assert_eq!(Status::from(FileStatus::Renamed), Status::Renamed);
    }
}

mod staged_file {
    use super::*;

    #[test]
    fn the_answer_survives_a_round_trip() {
        let answer = StagedFile {
            workspace: 3,
            path: "b.txt".into(),
            old_path: Some("a.txt".into()),
            status: FileStatus::Renamed,
            staged: true,
        };

        let text = serde_json::to_string(&answer).expect("json");

        assert_eq!(serde_json::from_str::<StagedFile>(&text).expect("answer"), answer);
    }

    #[test]
    fn a_file_names_its_staging_only_when_it_is_known() {
        let known =
            FileInfo { staged: Some(Staging::Partly), ..info("a", FileStatus::Modified, FoldClass::Open, false) };

        assert_eq!(serde_json::to_value(&known).expect("json")["staged"], "partly");
        let unknown = serde_json::to_value(info("a", FileStatus::Modified, FoldClass::Open, false)).expect("json");
        assert!(unknown.get("staged").is_none());
    }
}

mod digests {
    use super::*;

    #[test]
    fn a_digest_is_sixteen_hex_digits_and_round_trips() {
        assert_eq!(digest(255), "00000000000000ff");
        assert_eq!(parse_digest("00000000000000ff"), Some(255));
    }

    #[rstest::rstest]
    #[case::short("0123456789abcde")]
    #[case::long("0123456789abcdef0")]
    #[case::not_hex("0123456789abcdeg")]
    #[case::signed("+123456789abcdef")]
    fn a_malformed_digest_is_refused(#[case] text: &str) {
        assert_eq!(parse_digest(text), None);
    }
}

mod patches {
    use super::*;

    #[test]
    fn lines_keep_their_kind_numbers_and_emphasis_in_characters() {
        let patch = patch();

        let lines = &patch.hunks[0].lines;
        assert_eq!((lines[0].kind, lines[0].old, lines[0].new), (LineKind::Context, Some(1), Some(1)));
        assert_eq!((lines[1].kind, lines[1].old, lines[1].new), (LineKind::Removed, Some(2), None));
        assert_eq!(lines[1].emphasis, [[8, 9]]);
        assert_eq!(lines[2].emphasis, [[8, 9]]);
    }

    #[test]
    fn syntax_runs_go_through_a_style_table_whose_first_entry_is_plain() {
        let patch = patch();

        assert_eq!(patch.styles[0], Style::default());
        assert_eq!(patch.styles.len(), 2);
        let first = patch.hunks[0].lines[0].syntax.as_ref().expect("syntax");
        assert_eq!(first[0].style, 1);
        assert!(patch.hunks[0].lines[2].syntax.is_none());
    }
}

mod a_line_s_own_text {
    use super::*;

    const EDIT: &str = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,3 @@\n fn main() {\n\
                        -\tsay(1);\n+\tsay(2);\n+\tsay(3);\r\n";
    const TWO_HUNKS: &str =
        "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-one\n+ONE\n@@ -4 +4 @@\n-four\n+FOUR\n";

    fn lines() -> Vec<LineInfo> {
        let mut file = parse(EDIT).remove(0);
        finish(&mut file);
        Patch::of(2, Mode::Uncommitted, &file, false).hunks.remove(0).lines
    }

    #[test]
    fn comes_beside_the_text_only_on_a_line_with_a_tab() {
        let lines = lines();

        assert_eq!((lines[0].text.as_str(), lines[0].raw.as_deref()), ("fn main() {", None));
        assert_eq!((lines[1].text.as_str(), lines[1].raw.as_deref()), ("    say(1);", Some("\tsay(1);")));
        assert_eq!((lines[2].text.as_str(), lines[2].raw.as_deref()), ("    say(2);", Some("\tsay(2);")));
    }

    #[test]
    fn holds_no_line_ending() {
        assert_eq!(lines()[3].raw.as_deref(), Some("\tsay(3);"));
    }

    #[test]
    fn leaves_the_emphasis_and_the_syntax_on_the_text() {
        let lines = lines();

        let added = &lines[2];
        let drawn: String = added.syntax.iter().flatten().map(|run| run.text.as_str()).collect();
        assert_eq!((added.emphasis.as_slice(), drawn.as_str()), ([[8, 9]].as_slice(), added.text.as_str()));
    }

    #[test]
    fn comes_with_the_unchanged_lines_of_a_gap_too() {
        let side = ["ONE", "\ttwo", "three", "FOUR"].map(String::from);

        let gap = Gap::of(2, "a", 1, &gap_lines(&parse(TWO_HUNKS)[0], 1, &side));

        assert_eq!((gap.lines[0].text.as_str(), gap.lines[0].raw.as_deref()), ("    two", Some("\ttwo")));
        assert_eq!((gap.lines[1].text.as_str(), gap.lines[1].raw.as_deref()), ("three", None));
    }
}

mod read_back {
    use ratatui::style::Color;

    use super::*;

    fn painted(looks: Option<Segments>) -> Option<Segments> {
        looks.map(|segments| segments.into_iter().map(|(text, look)| (text, look.bg(Color::Reset))).collect())
    }

    fn line(text: &str, emphasis: Vec<[usize; 2]>, style: usize) -> Line {
        let line = LineInfo {
            text: text.into(),
            emphasis,
            syntax: Some(vec![Segment { style, text: text.into() }]),
            ..LineInfo::default()
        };
        let patch = Patch {
            styles: vec![Style::default()],
            hunks: vec![HunkInfo { lines: vec![line], ..HunkInfo::default() }],
            ..Patch::default()
        };
        patch.file().hunks.remove(0).lines.remove(0)
    }

    #[test]
    fn a_patch_is_the_file_it_came_from() {
        let mut sent = file();
        for line in sent.hunks.iter_mut().flat_map(|hunk| hunk.lines.iter_mut()) {
            line.syntax = painted(line.syntax.take());
        }

        assert_eq!(patch().file(), File { digest: sent.viewed_digest, ..sent });
    }

    #[test]
    fn a_hunk_without_lines_stays_empty() {
        let patch =
            Patch { hunks: vec![HunkInfo { old_start: 4, new_start: 5, ..HunkInfo::default() }], ..Patch::default() };

        let hunks = patch.file().hunks;

        assert_eq!((hunks[0].old_start, hunks[0].new_start, hunks[0].lines.len()), (4, 5, 0));
    }

    #[rstest::rstest]
    #[case::inverted("say(1);", vec![[5, 2]], vec![])]
    #[case::empty("say(1);", vec![[3, 3]], vec![])]
    #[case::past_the_end("say(1);", vec![[0, 10_000]], vec![0..7])]
    #[case::beyond_the_text("say(1);", vec![[9, 12]], vec![])]
    #[case::multibyte("añb", vec![[1, 2], [2, 4]], vec![1..2, 2..3])]
    fn emphasis_stays_within_the_text_s_characters(
        #[case] text: &str,
        #[case] sent: Vec<[usize; 2]>,
        #[case] kept: Vec<Range<usize>>,
    ) {
        assert_eq!(line(text, sent, 0).emphasis, kept);
    }

    #[test]
    fn a_style_missing_from_the_table_is_the_plain_one() {
        let syntax = line("x", Vec::new(), 999).syntax.expect("its syntax");

        assert_eq!(syntax, [("x".to_string(), Look::default())]);
    }

    #[test]
    fn a_file_keeps_its_staging_and_fold_and_reads_its_digest() {
        let info = FileInfo {
            staged: Some(Staging::Partly),
            ..info("Cargo.lock", FileStatus::Renamed, FoldClass::Lockfile, true)
        };

        let file = info.file();

        assert_eq!((file.staged, file.fold, file.status), (Some(Staged::Partly), Fold::Lockfile, Status::Renamed));
        assert_eq!((file.digest, file.viewed_digest), (0x00c0_ffee_1234_5678, 0x00c0_ffee_1234_5678));
        assert_eq!(file.hunks, Vec::new());
    }

    #[test]
    fn an_unreadable_digest_is_zero() {
        let info = FileInfo { digest: "not a digest".into(), ..FileInfo::default() };

        assert_eq!(info.file().viewed_digest, 0);
    }

    #[test]
    fn a_gap_is_the_lines_it_came_from() {
        let plain = GapLine { old: 3, new: 4, text: "let x = 1;".into(), raw: None, syntax: None };
        let coloured =
            GapLine { syntax: Some(vec![("let".into(), Look::default().fg(Color::Indexed(5)))]), ..plain.clone() };
        let tabbed = GapLine { text: "    x".into(), raw: Some("\tx".into()), ..plain.clone() };

        let lines = Gap::of(2, "src/a.rs", 0, &[plain.clone(), coloured.clone(), tabbed.clone()]).lines();

        assert_eq!(lines, [plain, GapLine { syntax: painted(coloured.syntax.clone()), ..coloured }, tabbed]);
    }
}
