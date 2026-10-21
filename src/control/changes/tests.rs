use serde_json::json;

use super::*;
use crate::changes::diff::{Fold, Hunk, Kind, Line, Status};
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
        syntax: Some(vec![(text.into(), Look::default().fg(ratatui::style::Color::Indexed(5)))]),
        emphasis: Vec::new(),
    };
    let mut removed = line(Kind::Removed, Some(2), None, "    say(1);");
    removed.emphasis = std::iter::once(8..9).collect();
    let mut added = line(Kind::Added, None, Some(2), "    say(2);");
    added.emphasis = std::iter::once(8..9).collect();
    added.syntax = None;
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
            lines: vec![line(Kind::Context, Some(1), Some(1), "fn main() {"), removed, added],
        }],
        digest: 7,
        viewed_digest: 0x00c0_ffee_1234_5678,
        staged: None,
    }
}

fn patch() -> Patch {
    Patch::of(2, Mode::Uncommitted, &file(), false)
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
