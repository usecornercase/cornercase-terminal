use std::time::{Duration, UNIX_EPOCH};

use rstest::rstest;

use super::*;
use crate::control::Command;
use crate::control::contract;
use crate::files::disk::{Kind, Listed, Loaded, Stamp as DiskStamp};
use crate::test_util::{TempDir, fixture};

const BOOT: &str = "4242-1700000000";
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAADCAYAAAC09K7GAAAAPklEQVR4AQEzAMz/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADMAAcS8COkAAAAASUVORK5CYII=";

fn at(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn listing() -> Line<Listing> {
    let entry = |name: &str, kind, bytes, status| {
        let listed = Listed { name: name.into(), kind, link: false, bytes, modified: Some(at(1_700_000_000)) };
        EntryInfo::of(&listed, status)
    };
    let root = FolderInfo {
        path: String::new(),
        dir: "/work/shop".into(),
        entries: vec![
            entry("src", Kind::Folder, None, Some(Status::Modified)),
            entry(".env", Kind::File, Some(12), None),
            entry("notes.txt", Kind::File, Some(8), Some(Status::Untracked)),
        ],
        more: false,
        error: None,
    };
    let gone = FolderInfo::failed("missing", Path::new("/work/shop/missing"), "no such folder");
    Line {
        boot: BOOT.into(),
        stream: Some(1),
        revision: 2,
        content: Listing { workspace: Some(3), folders: vec![root, gone] },
    }
}

fn loaded(raw: Raw) -> Loaded {
    Loaded { stamp: stamp_of(&raw), raw }
}

fn stamp_of(raw: &Raw) -> DiskStamp {
    let dir = TempDir::new();
    let path = dir.path().join("f");
    match raw {
        Raw::Text(text) => std::fs::write(&path, text).expect("write"),
        Raw::Image { bytes, .. } => std::fs::write(&path, bytes).expect("write"),
        _ => std::fs::write(&path, [0, 1]).expect("write"),
    }
    disk::load(&path, None).expect("loaded").stamp
}

fn text_read() -> Line<Read> {
    let read = Read::of("src/main.rs", Path::new("/work/shop/src/main.rs"), loaded(Raw::Text("fn main() {}\n".into())));
    Line {
        boot: BOOT.into(),
        stream: Some(2),
        revision: 1,
        content: Read { workspace: Some(3), modified: Some("2026-10-26T10:00:00.000Z".into()), ..read },
    }
}

fn image_read() -> Read {
    let bytes = STANDARD.decode(PNG).expect("base64");
    let raw = Raw::Image { format: "PNG", bytes };
    Read {
        workspace: Some(3),
        modified: Some("2026-10-26T10:00:00.000Z".into()),
        ..Read::of("logo.png", Path::new("/work/shop/logo.png"), loaded(raw))
    }
}

fn found() -> Found {
    Found {
        workspace: 3,
        query: "main".into(),
        mode: SearchMode::Text,
        names: Vec::new(),
        files: vec![FoundFile {
            path: "src/main.rs".into(),
            lines: vec![FoundLine { number: 1, text: "fn main() {}".into(), ranges: vec![[3, 7]] }],
        }],
        matches: 1,
        capped: false,
    }
}

mod fixtures {
    use super::*;

    #[test]
    fn a_listing_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&listing()).expect("json");
        let read: Line<Listing> = serde_json::from_str(fixture!("control/files/v1/listing.json")).expect("a listing");

        assert_eq!(written, fixture!("control/files/v1/listing.json").trim_end());
        assert_eq!(read, listing());
    }

    #[test]
    fn a_text_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&text_read()).expect("json");
        let read: Line<Read> = serde_json::from_str(fixture!("control/files/v1/text.json")).expect("a read");

        assert_eq!(written, fixture!("control/files/v1/text.json").trim_end());
        assert_eq!(read, text_read());
    }

    #[test]
    fn an_image_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&image_read()).expect("json");
        let read: Read = serde_json::from_str(fixture!("control/files/v1/image.json")).expect("a read");

        assert_eq!(written, fixture!("control/files/v1/image.json").trim_end());
        assert_eq!(read, image_read());
    }

    #[test]
    fn a_search_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&found()).expect("json");
        let read: Found = serde_json::from_str(fixture!("control/files/v1/search.json")).expect("a search");

        assert_eq!(written, fixture!("control/files/v1/search.json").trim_end());
        assert_eq!(read, found());
    }

    #[test]
    fn the_stream_line_wraps_the_answer_as_ok() {
        let text = listing().streamed().expect("a line");
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");

        assert_eq!(value["ok"]["revision"], 2);
        assert_eq!(value["ok"]["folders"][0]["entries"][0]["name"], "src");
    }
}

mod requests {
    use super::*;

    #[test]
    fn leave_out_what_they_do_not_say() {
        let list: FilesList = serde_json::from_str("{}").expect("a request");
        let read: FilesRead = serde_json::from_str(r#"{"path": "a"}"#).expect("a request");
        let search: FilesSearch = serde_json::from_str(r#"{"query": "a"}"#).expect("a request");

        assert_eq!(list, FilesList { workspace: None, folders: Vec::new(), follow: false });
        assert_eq!((read.follow, read.workspace), (false, None));
        assert_eq!(search.mode, SearchMode::Text);
    }

    #[test]
    fn a_request_names_no_field_it_does_not_know() {
        let raw = serde_json::json!({"workspace": 1, "folders": ["a"], "follow": true, "extra": 1});
        let parsed: FilesList = serde_json::from_value(raw.clone()).expect("a request");

        assert_eq!(contract::unknown_fields(&raw, &parsed).as_deref(), Some("extra"));
        let known = serde_json::json!({"workspace": null, "folders": [], "follow": false});
        let parsed: FilesList = serde_json::from_value(known.clone()).expect("a request");
        assert_eq!(contract::unknown_fields(&known, &parsed), None);
    }

    #[rstest]
    #[case::list(Command::FilesList(FilesList { workspace: Some(3), folders: vec!["a".into(), "b".into()], follow: true }), "files-list", "workspace=3 folders=2 follow=true")]
    #[case::read(Command::FilesRead(FilesRead { workspace: None, path: "a.rs".into(), follow: false }), "files-read", "path=a.rs follow=false")]
    fn are_logged_by_name_and_fields(#[case] command: Command, #[case] name: &str, #[case] fields: &str) {
        let shown: Vec<String> = command.fields().into_iter().map(|(key, value)| format!("{key}={value}")).collect();

        assert_eq!(command.name(), name);
        assert_eq!(shown.join(" "), fields);
    }

    #[test]
    fn a_search_logs_its_size_and_never_its_query() {
        let command =
            Command::FilesSearch(FilesSearch { workspace: Some(1), query: "secret".into(), mode: SearchMode::Names });
        let shown: Vec<String> = command.fields().into_iter().map(|(key, value)| format!("{key}={value}")).collect();

        assert_eq!(command.name(), "files-search");
        assert_eq!(shown.join(" "), "workspace=1 mode=names bytes=6");
    }
}

mod an_answer_of_a_read {
    use super::*;

    fn read(raw: Raw) -> Read {
        Read::of("x", Path::new("/w/x"), loaded(raw))
    }

    #[test]
    fn gives_a_text_its_language_and_size() {
        let read = Read::of("a.rs", Path::new("/w/a.rs"), loaded(Raw::Text("fn a() {}\n".into())));

        assert_eq!((read.kind, read.language.as_deref(), read.bytes), (ReadKind::Text, Some("rust"), Some(10)));
        assert_eq!(read.text.as_deref(), Some("fn a() {}\n"));
    }

    #[test]
    fn gives_an_image_its_format_size_and_bytes_in_base64() {
        let read = image_read();
        let image = read.image.expect("an image");

        assert_eq!(
            (read.kind, image.format.as_str(), image.width, image.height),
            (ReadKind::Image, "PNG", Some(4), Some(3))
        );
        assert_eq!(read.text, None);
        assert!(STANDARD.decode(&image.data).expect("base64").starts_with(b"\x89PNG"));
    }

    #[test]
    fn gives_an_image_that_cannot_be_measured_no_size() {
        let image =
            read(Raw::Image { format: "PNG", bytes: b"\x89PNG\r\n\x1a\nbroken".to_vec() }).image.expect("image");

        assert_eq!((image.width, image.height), (None, None));
    }

    #[rstest]
    #[case::binary(Raw::Binary, ReadKind::Binary)]
    #[case::folder(Raw::Folder, ReadKind::Folder)]
    #[case::missing(Raw::Missing, ReadKind::Missing)]
    fn names_what_it_is_without_content(#[case] raw: Raw, #[case] kind: ReadKind) {
        let read = read(raw);

        assert_eq!((read.kind, read.text, read.image), (kind, None, None));
    }

    #[test]
    fn says_how_large_what_is_too_large_is() {
        let read = read(Raw::TooLarge(9_000_000));

        assert_eq!((read.kind, read.bytes), (ReadKind::TooLarge, Some(9_000_000)));
    }

    #[test]
    fn a_folder_and_a_missing_file_have_no_size() {
        assert_eq!((read(Raw::Folder).bytes, read(Raw::Missing).bytes), (None, None));
    }
}

mod an_entry {
    use super::*;

    fn of(name: &str, kind: Kind) -> EntryInfo {
        EntryInfo::of(&Listed { name: name.into(), kind, link: true, bytes: None, modified: None }, None)
    }

    #[test]
    fn is_hidden_when_its_name_starts_with_a_dot() {
        assert_eq!((of(".env", Kind::File).hidden, of("env", Kind::File).hidden), (true, false));
    }

    #[test]
    fn keeps_its_kind_and_link() {
        let entry = of("x", Kind::Other);

        assert_eq!((entry.kind, entry.link, entry.modified), (EntryKind::Other, true, None));
    }

    #[test]
    fn writes_its_time_in_utc() {
        let listed = Listed {
            name: "a".into(),
            kind: Kind::File,
            link: false,
            bytes: Some(1),
            modified: Some(at(1_700_000_000)),
        };

        assert_eq!(EntryInfo::of(&listed, None).modified.as_deref(), Some("2023-11-14T22:13:20.000Z"));
    }
}

mod a_search_answer {
    use super::*;
    use crate::files::search::{self, MAX_NAMES};

    #[test]
    fn of_names_is_capped_at_the_most_the_panel_shows() {
        let name = |i: usize| search::Name { path: format!("f{i}"), indices: vec![0] };
        let few = Found::names(1, "f", &[name(1)]);
        let many = Found::names(1, "f", &(0..MAX_NAMES).map(name).collect::<Vec<_>>());

        assert_eq!((few.capped, few.names[0].indices.clone(), few.matches), (false, vec![0], 1));
        assert!(many.capped);
    }

    #[test]
    fn of_text_turns_ranges_into_pairs() {
        let line = search::Line { number: 4, text: "a b a".into(), ranges: vec![0..1, 4..5] };
        let text = search::Text {
            files: vec![search::Found { path: "p".into(), lines: vec![line] }],
            matches: 2,
            capped: true,
        };
        let found = Found::text(1, "a", &text);

        assert_eq!(found.files[0].lines[0].ranges, [[0, 1], [4, 5]]);
        assert_eq!((found.matches, found.capped, found.mode), (2, true, SearchMode::Text));
    }
}

mod read_back {
    use super::*;
    use crate::files::search::{self};

    fn local(name: &str, bytes: &[u8]) -> (Read, disk::Content) {
        let dir = TempDir::new();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("write");
        let read = Read::of(name, &path, disk::load(&path, None).expect("loaded"));
        (read, disk::read(&path, None).expect("read"))
    }

    #[test]
    fn a_text_is_the_body_and_language_read_here() {
        let (read, here) = local("a.rs", b"fn a() {\r\n\tb();\n}\n");

        let there = read.content();

        assert_eq!((there.language, &there.body), (here.language, &here.body));
        assert_eq!((there.stamp.len(), there.stamp.modified().is_some()), (Some(18), true));
    }

    #[test]
    fn an_image_is_the_picture_read_here() {
        let content = image_read().content();

        let Body::Image(picture) = content.body else { panic!("not a picture: {:?}", content.body) };
        assert_eq!((picture.format, picture.width, picture.height), ("PNG", 4, 3));
    }

    #[rstest]
    #[case::binary(Raw::Binary, Body::Binary)]
    #[case::folder(Raw::Folder, Body::Binary)]
    #[case::too_large(Raw::TooLarge(9_000_000), Body::TooLarge(9_000_000))]
    #[case::missing(Raw::Missing, Body::Missing)]
    fn what_has_no_content_is_what_it_is_here(#[case] raw: Raw, #[case] body: Body) {
        let content = Read::of("x", Path::new("/w/x"), loaded(raw)).content();

        assert_eq!((content.language, content.body), (None, body));
    }

    #[test]
    fn a_missing_file_has_no_size() {
        let content = Read::of("x", Path::new("/w/x"), loaded(Raw::Missing)).content();

        assert_eq!(content.stamp.len(), None);
    }

    #[test]
    fn a_listing_gives_each_folder_s_entries_or_none_when_it_failed() {
        let folders = listing().content.folders;

        let (root, gone) = (folders[0].listed(), folders[1].listed());

        let names: Vec<(String, bool)> =
            root.1.expect("its entries").into_iter().map(|entry| (entry.name, entry.dir)).collect();
        assert_eq!(names, [("src".into(), true), (".env".into(), false), ("notes.txt".into(), false)]);
        assert_eq!((root.0.as_str(), gone.0.as_str(), gone.1), ("", "missing", None));
    }

    #[test]
    fn a_search_gives_the_names_and_lines_it_found() {
        let names = Found { names: vec![NameInfo { path: "src/a.rs".into(), indices: vec![4, 5] }], ..found() };

        let text = names.found_text();

        assert_eq!(names.found_names(), [search::Name { path: "src/a.rs".into(), indices: vec![4, 5] }]);
        assert_eq!(
            text.files[0].lines[0],
            search::Line { number: 1, text: "fn main() {}".into(), ranges: std::iter::once(3..7).collect() }
        );
        assert_eq!((text.matches, text.capped), (1, false));
    }
}

mod hostile_reads {
    use super::*;

    fn text(language: &str) -> disk::Content {
        Read { language: Some(language.into()), text: Some("x".into()), ..Read::default() }.content()
    }

    #[rstest]
    #[case::not_base64("%%%not base64%%%")]
    #[case::cut_short("iVBORw0KGgo")]
    #[case::not_an_image("aGVsbG8=")]
    fn a_broken_image_is_unreadable(#[case] data: &str) {
        let image = ImageInfo { format: "PNG".into(), data: data.into(), ..ImageInfo::default() };
        let read = Read { kind: ReadKind::Image, bytes: Some(9), image: Some(image), ..Read::default() };

        assert!(matches!(read.content().body, Body::Unreadable { .. }), "{:?}", read.content().body);
    }

    #[test]
    fn an_image_without_its_data_is_unreadable() {
        let read = Read { kind: ReadKind::Image, ..Read::default() };

        assert!(matches!(read.content().body, Body::Unreadable { bytes: 0, .. }));
    }

    #[rstest]
    #[case::known("rust", Some("rust"))]
    #[case::one_of_ours("tsx", Some("tsx"))]
    #[case::unknown("klingon", None)]
    #[case::an_extension("rs", None)]
    #[case::a_path("../../etc/passwd", None)]
    fn a_language_is_one_highlighted_here_or_none(#[case] language: &str, #[case] kept: Option<&str>) {
        assert_eq!(text(language).language, kept);
    }

    #[test]
    fn an_unreadable_time_leaves_the_stamp_without_one() {
        let read = Read { modified: Some("yesterday".into()), bytes: Some(1), ..Read::default() };

        assert_eq!(read.content().stamp.modified(), None);
    }

    #[test]
    fn ranges_and_indices_stay_within_their_text() {
        let line = FoundLine { number: 1, text: "añb".into(), ranges: vec![[0, 10_000], [2, 1], [9, 12]] };
        let found = Found {
            names: vec![NameInfo { path: "añb".into(), indices: vec![0, 2, 3, 9999] }],
            files: vec![FoundFile { path: "a".into(), lines: vec![line] }],
            ..Found::default()
        };

        assert_eq!(found.found_names()[0].indices, [0, 2]);
        assert_eq!(found.found_text().files[0].lines[0].ranges, std::iter::once(0..3).collect::<Vec<_>>());
    }
}
