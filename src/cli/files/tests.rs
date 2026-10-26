use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::args::{Cli, Command, Control};
use crate::control::files::{EntryInfo, FolderInfo, FoundFile, FoundLine, ImageInfo, NameInfo};

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

mod parsing {
    use super::*;

    #[rstest]
    #[case::the_root(&["files", "list"])]
    #[case::several_folders(&["files", "list", "src", "~/code", "/etc", "--workspace", "3", "--json"])]
    #[case::following(&["files", "list", "src", "--follow", "--worktree", "fix"])]
    #[case::a_file(&["files", "read", "src/main.rs", "--workspace", "3", "--json"])]
    #[case::a_file_following(&["files", "read", "a.rs", "--follow"])]
    #[case::text(&["files", "search", "parse", "--text"])]
    #[case::names(&["files", "search", "main.rs", "--names", "--json"])]
    #[case::the_default_mode(&["files", "search", "parse"])]
    fn these_are_taken(#[case] args: &[&str]) {
        let parsed = parse(args).expect("parse");

        assert!(matches!(parsed.command, Some(Command::Control(Control::Files(_)))));
    }

    #[rstest]
    #[case::names_and_text(&["files", "search", "a", "--names", "--text"])]
    #[case::reading_without_a_path(&["files", "read"])]
    #[case::searching_without_a_query(&["files", "search"])]
    #[case::following_a_search(&["files", "search", "a", "--follow"])]
    #[case::two_places(&["files", "list", "--workspace", "1", "--worktree", "x"])]
    #[case::hiding_what_the_panel_shows(&["files", "list", "--hidden"])]
    fn these_are_refused(#[case] args: &[&str]) {
        assert!(parse(args).is_err());
    }
}

fn entry(name: &str, kind: EntryKind, bytes: Option<u64>, status: Option<FileStatus>) -> EntryInfo {
    EntryInfo {
        name: name.into(),
        kind,
        bytes,
        status,
        modified: Some("2026-10-26T10:00:00.000Z".into()),
        ..EntryInfo::default()
    }
}

mod the_listing {
    use super::*;

    #[test]
    fn is_a_table_with_a_header_and_a_dash_for_nothing() {
        let root = FolderInfo {
            entries: vec![
                entry("src", EntryKind::Folder, None, Some(FileStatus::Modified)),
                EntryInfo { modified: None, ..entry("notes.txt", EntryKind::File, Some(8), None) },
            ],
            ..FolderInfo::default()
        };
        let text = list_text(&Listing { workspace: Some(1), folders: vec![root] });

        assert_eq!(
            text,
            "folder\tname\tkind\tstatus\tbytes\tmodified\n\
             .\tsrc\tfolder\tmodified\t-\t2026-10-26T10:00:00.000Z\n\
             .\tnotes.txt\tfile\t-\t8\t-\n"
        );
    }

    #[test]
    fn names_each_folder_as_it_was_asked_and_cleans_control_characters() {
        let folder = FolderInfo {
            path: "~/a\tb".into(),
            entries: vec![entry("x\ny", EntryKind::Other, None, None)],
            ..FolderInfo::default()
        };
        let text = list_text(&Listing { workspace: None, folders: vec![folder] });

        assert_eq!(text.lines().nth(1), Some("~/a b\tx y\tother\t-\t-\t2026-10-26T10:00:00.000Z"));
    }

    #[test]
    fn a_folder_that_failed_is_a_partial_answer_and_the_others_still_print() {
        let failed = FolderInfo { path: "gone".into(), error: Some("no such folder".into()), ..FolderInfo::default() };
        let listing = Listing { workspace: None, folders: vec![failed] };

        assert!(matches!(notes(&listing), Err(Error::Partial)));
        assert!(notes(&Listing::default()).is_ok());
    }
}

mod a_read {
    use super::*;

    fn read(kind: ReadKind) -> Read {
        Read { path: "a.bin".into(), kind, bytes: Some(2048), ..Read::default() }
    }

    #[test]
    fn json_output_only_reads_the_path_and_kind_to_tell_an_error() {
        let gone = serde_json::json!({"path": "a.txt", "kind": "missing", "image": {"data": "x"}});
        let folder = serde_json::json!({"path": "src", "kind": "folder"});
        let image = serde_json::json!({"path": "a.png", "kind": "image", "image": {"data": "x"}});

        assert_eq!(read_text(&outline(&gone)).expect_err("missing").to_string(), "cannot read a.txt: no such file");
        assert!(read_text(&outline(&folder)).is_err());
        assert!(read_text(&outline(&image)).is_ok());
    }

    #[test]
    fn of_text_is_the_file_as_it_is() {
        let text = Read { text: Some("one\ntwo".into()), ..read(ReadKind::Text) };

        assert_eq!(read_text(&text).expect("text"), "one\ntwo");
    }

    #[test]
    fn of_an_image_says_its_format_and_size() {
        let image = ImageInfo { format: "PNG".into(), width: Some(1280), height: Some(720), data: String::new() };
        let read = Read { image: Some(image), ..read(ReadKind::Image) };

        assert_eq!(read_text(&read).expect("text"), "a.bin: PNG image, 1280×720, 2048 bytes; --json gives its data\n");
    }

    #[test]
    fn of_what_cannot_be_shown_names_the_path_without_control_characters() {
        let read = Read { path: "a\x1b[2Jb".into(), kind: ReadKind::Binary, bytes: Some(1), ..Read::default() };

        assert_eq!(read_text(&read).expect("text"), "a [2Jb: binary, 1 bytes\n");
    }

    #[rstest]
    #[case::binary(ReadKind::Binary, "a.bin: binary, 2048 bytes\n")]
    #[case::too_large(ReadKind::TooLarge, "a.bin: too large to read, 2048 bytes\n")]
    fn of_what_cannot_be_shown_says_what_it_is(#[case] kind: ReadKind, #[case] says: &str) {
        assert_eq!(read_text(&read(kind)).expect("text"), says);
    }

    #[test]
    fn of_a_missing_file_or_a_folder_is_an_error() {
        let missing = read_text(&read(ReadKind::Missing)).expect_err("missing");
        let folder = read_text(&read(ReadKind::Folder)).expect_err("folder");

        assert_eq!(missing.to_string(), "cannot read a.bin: no such file");
        assert!(folder.to_string().contains("`cornercase files list a.bin` lists it"), "{folder}");
    }
}

mod a_search {
    use super::*;

    #[test]
    fn of_names_is_one_path_per_line() {
        let found = Found {
            names: vec![
                NameInfo { path: "src/main.rs".into(), indices: vec![0] },
                NameInfo { path: "main.rs".into(), indices: vec![] },
            ],
            ..Found::default()
        };

        assert_eq!(search_text(&found), "src/main.rs\nmain.rs\n");
    }

    #[test]
    fn of_text_is_path_line_text_as_grep_prints_it() {
        let line = |number, text: &str| FoundLine { number, text: text.into(), ranges: vec![] };
        let found = Found {
            files: vec![FoundFile {
                path: "src/a.rs".into(),
                lines: vec![line(3, "let parse = 1;"), line(9, "parse()")],
            }],
            ..Found::default()
        };

        assert_eq!(search_text(&found), "src/a.rs:3:let parse = 1;\nsrc/a.rs:9:parse()\n");
    }

    #[test]
    fn never_sends_control_characters_to_the_terminal() {
        let names = Found { names: vec![NameInfo { path: "a\x1b[31mb".into(), indices: vec![] }], ..Found::default() };
        let line = FoundLine { number: 1, text: "x\x1b]0;t\x07y".into(), ranges: vec![] };
        let text = Found { files: vec![FoundFile { path: "p\x1bq".into(), lines: vec![line] }], ..Found::default() };

        assert_eq!(search_text(&names), "a [31mb\n");
        assert_eq!(search_text(&text), "p q:1:x ]0;t y\n");
    }

    #[test]
    fn with_nothing_found_says_so() {
        assert_eq!(search_text(&Found::default()), "nothing found\n");
    }
}
