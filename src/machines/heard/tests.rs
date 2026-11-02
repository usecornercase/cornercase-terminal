use std::path::PathBuf;

use rstest::rstest;
use serde::Serialize;
use serde_json::json;

use super::*;
use crate::control::changes::{Content, WorkspaceChanges};
use crate::control::files::{FoundLine, ImageInfo, Line};

const HOSTILE: &str = "a\u{1b}[2Jb\u{1b}]52;c;cm0gLXJmIH4K\u{7}c\u{202e}d\u{200b}e\rf";
const KEPT: &str = "a[2Jb]52;c;cm0gLXJmIH4Kcdef";
const SAID: &str = "a[2Jb]52;c;cm0gLXJmIH4Kcde f";

fn ok(value: &impl Serialize) -> String {
    serde_json::to_string(&Structured::Ok(serde_json::to_value(value).expect("a value"))).expect("a line")
}

fn file(path: &str) -> FileInfo {
    FileInfo { path: path.into(), digest: "00ff".into(), ..FileInfo::default() }
}

fn line(text: &str) -> LineInfo {
    LineInfo { text: text.into(), ..LineInfo::default() }
}

fn hunk(lines: usize) -> HunkInfo {
    HunkInfo { lines: vec![line("x"); lines], ..HunkInfo::default() }
}

mod code {
    use super::*;

    #[rstest]
    #[case::erase_the_screen("x\u{1b}[2Jy", "x[2Jy")]
    #[case::write_the_clipboard("x\u{1b}]52;c;cm0gLXJmIH4K\u{7}y", "x]52;c;cm0gLXJmIH4Ky")]
    #[case::bell("x\u{7}y", "xy")]
    #[case::right_to_left("x\u{202e}y", "xy")]
    #[case::zero_width("x\u{200b}y", "xy")]
    #[case::lone_carriage_return("x\ry", "xy")]
    #[case::tabs_and_line_breaks("\tx\ny\r\nz\n", "\tx\ny\r\nz\n")]
    fn keeps_line_breaks_and_tabs_but_no_control_or_hidden_character(#[case] text: &str, #[case] kept: &str) {
        assert_eq!(clean_code(text), kept);
    }

    #[rstest]
    #[case::tabs("\tx\ty", "\tx\ty")]
    #[case::line_breaks("x\ny\r\nz\n", "xyz")]
    #[case::control_and_hidden("x\u{1b}[2J\u{202e}y", "x[2Jy")]
    fn of_one_line_keeps_its_tabs_but_no_line_break(#[case] text: &str, #[case] kept: &str) {
        assert_eq!(clean_code_line(text), kept);
    }
}

mod answers {
    use super::*;

    #[rstest]
    #[case::stale_boot(r#"{"error":{"code":"stale-boot","message":"restarted"}}"#, Code::StaleBoot)]
    #[case::not_found(r#"{"error":{"code":"not-found","message":"no workspace 3"}}"#, Code::NotFound)]
    #[case::plain_error(r#"{"error":"it failed"}"#, Code::Failed)]
    #[case::garbage("{{{not json", Code::Malformed)]
    #[case::another_shape(r#"{"ok":{"folders":5}}"#, Code::Malformed)]
    fn a_refusal_keeps_its_code(#[case] text: &str, #[case] code: Code) {
        assert_eq!(listing(text).map_err(|refusal| refusal.code), Err(code));
    }

    #[test]
    fn a_refusal_s_message_is_cleaned() {
        let text = json!({"error": {"code": "failed", "message": format!("{HOSTILE}\n{}", "z".repeat(10_000))}});

        let refusal = summary(&text.to_string()).expect_err("a refusal");

        assert!(refusal.message.chars().all(|c| !text::hidden(c)), "{:?}", refusal.message);
        assert_eq!(refusal.message.chars().count(), 200);
    }

    #[test]
    fn a_stream_line_is_read_like_an_answer() {
        let streamed = Line { boot: "boot-7".into(), stream: Some(4), revision: 2, content: Listing::default() };

        assert_eq!(listing(&streamed.streamed().expect("a line")), Ok(Listing::default()));
    }
}

mod summaries {
    use super::*;

    #[test]
    fn clean_names_and_paths_but_keep_paths_long() {
        let long = "d/".repeat(3_000);
        let mut renamed = file(&format!("{long}{HOSTILE}\nx"));
        renamed.old_path = Some(format!("old{HOSTILE}"));
        renamed.digest = "z".repeat(10_000);
        let workspace = WorkspaceChanges {
            base: Some(format!("origin/main{HOSTILE}")),
            files: vec![renamed],
            ..Default::default()
        };
        let content = Content { error: Some(HOSTILE.into()), workspaces: vec![workspace], ..Content::default() };
        let sent = Summary { boot: format!("boot{HOSTILE}"), content, ..Summary::default() };

        let heard = summary(&sent.streamed().expect("a line")).expect("a summary");

        let workspace = &heard.content.workspaces[0];
        let file = &workspace.files[0];
        assert_eq!(heard.boot, format!("boot{SAID}"));
        assert_eq!(heard.content.error.as_deref(), Some(SAID));
        assert_eq!(workspace.base.as_deref(), Some(format!("origin/main{SAID}").as_str()));
        assert_eq!(file.path.chars().count(), 4096);
        assert!(file.path.chars().all(|c| !text::hidden(c)));
        assert_eq!(file.old_path.as_deref(), Some(format!("old{KEPT}").as_str()));
        assert_eq!(file.digest.len(), 200);
    }
}

mod patches {
    use super::*;

    #[test]
    fn keep_the_tabs_of_their_lines_and_drop_line_breaks_and_the_rest() {
        let mut changed = line(&format!("\tlet x = 1;\r\n{HOSTILE}\n"));
        changed.syntax = Some(vec![Segment { style: 3, text: format!("\t\n{HOSTILE}") }]);
        let hunk = HunkInfo { context: format!("fn main(){HOSTILE}\n"), lines: vec![changed], ..HunkInfo::default() };
        let sent = Patch { file: file("src/a.rs"), hunks: vec![hunk], ..Patch::default() };

        let heard = patch(&ok(&sent)).expect("a patch");

        let hunk = &heard.hunks[0];
        assert_eq!(hunk.context, format!("fn main(){SAID}"));
        assert_eq!(hunk.lines[0].text, format!("\tlet x = 1;{KEPT}"));
        let segments = hunk.lines[0].syntax.as_ref().expect("its syntax");
        assert_eq!(segments[0].text, format!("\t{KEPT}"));
    }

    #[test]
    fn keep_no_more_lines_than_a_diff_can_hold() {
        let sent = Patch { hunks: vec![hunk(LINES_KEPT - 1), hunk(5), hunk(5)], ..Patch::default() };

        let heard = patch(&ok(&sent)).expect("a patch");

        let kept: Vec<usize> = heard.hunks.iter().map(|hunk| hunk.lines.len()).collect();
        assert_eq!(kept, [LINES_KEPT - 1, 1, 0]);
    }

    #[test]
    fn keep_no_more_emphasis_or_segments_than_their_text_has_bytes() {
        let mut changed = line("abc");
        changed.emphasis = vec![[0, 1]; 50];
        changed.syntax = Some(vec![Segment::default(); 50]);
        let sent = Patch { hunks: vec![HunkInfo { lines: vec![changed], ..HunkInfo::default() }], ..Patch::default() };

        let heard = patch(&ok(&sent)).expect("a patch");

        let line = &heard.hunks[0].lines[0];
        assert_eq!(line.emphasis.len(), 3);
        assert_eq!(line.syntax.as_ref().map(Vec::len), Some(4));
    }

    #[test]
    fn a_gap_cleans_its_path_and_code() {
        let lines = vec![GapLineInfo { text: format!("\t{HOSTILE}\n"), ..GapLineInfo::default() }];
        let sent = Gap { path: format!("src/a.rs\n{HOSTILE}"), lines, ..Gap::default() };

        let heard = gap(&ok(&sent)).expect("a gap");

        assert_eq!(heard.path, format!("src/a.rs{KEPT}"));
        assert_eq!(heard.lines[0].text, format!("\t{KEPT}"));
    }
}

mod files {
    use super::*;

    #[test]
    fn a_listing_cleans_paths_names_and_errors() {
        let entry =
            EntryInfo { name: format!("a\nb{HOSTILE}"), modified: Some(HOSTILE.into()), ..EntryInfo::default() };
        let folder = FolderInfo {
            path: format!("src\n{HOSTILE}"),
            dir: PathBuf::from(format!("/home/ana/src{HOSTILE}")),
            entries: vec![entry],
            error: Some(HOSTILE.into()),
            ..FolderInfo::default()
        };

        let heard = listing(&ok(&Listing { workspace: Some(3), folders: vec![folder] })).expect("a listing");

        let folder = &heard.folders[0];
        assert_eq!(folder.path, format!("src{KEPT}"));
        assert_eq!(folder.dir, PathBuf::from(format!("/home/ana/src{KEPT}")));
        assert_eq!(folder.error.as_deref(), Some(SAID));
        assert_eq!(folder.entries[0].name, format!("ab{KEPT}"));
        assert_eq!(folder.entries[0].modified.as_deref(), Some(SAID));
    }

    #[test]
    fn a_listing_keeps_no_more_entries_than_a_folder_lists() {
        let folder = FolderInfo { entries: vec![EntryInfo::default(); MAX_ENTRIES + 5], ..FolderInfo::default() };

        let heard = listing(&ok(&Listing { workspace: None, folders: vec![folder] })).expect("a listing");

        assert_eq!(heard.folders[0].entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn a_read_keeps_the_lines_and_tabs_of_its_text() {
        let image = ImageInfo { format: format!("png{HOSTILE}"), data: "iVBORw0K".into(), ..ImageInfo::default() };
        let sent = Read {
            path: format!("src/a.rs\n{HOSTILE}"),
            file: PathBuf::from("/home/ana/src/a.rs\u{202e}"),
            language: Some(format!("Rust{HOSTILE}")),
            text: Some(format!("fn main() {{\r\n\tlet x = 1;{HOSTILE}\n}}\n")),
            image: Some(image),
            ..Read::default()
        };

        let heard = read(&ok(&sent)).expect("a read");

        assert_eq!(heard.path, format!("src/a.rs{KEPT}"));
        assert_eq!(heard.file, PathBuf::from("/home/ana/src/a.rs"));
        assert_eq!(heard.language.as_deref(), Some(format!("Rust{SAID}").as_str()));
        assert_eq!(heard.text.as_deref(), Some(format!("fn main() {{\r\n\tlet x = 1;{KEPT}\n}}\n").as_str()));
        let image = heard.image.expect("its image");
        assert_eq!((image.format, image.data), (format!("png{SAID}"), "iVBORw0K".to_string()));
    }

    #[test]
    fn found_names_and_lines_are_cleaned_and_kept_within_the_search_s_limits() {
        let name = NameInfo { path: format!("a\n{HOSTILE}"), indices: (0..9_999).collect() };
        let hit = FoundLine { text: format!("\t{HOSTILE}\r\n"), ranges: vec![[0, 1]; 500], ..FoundLine::default() };
        let files = vec![
            FoundFile { path: format!("b\n{HOSTILE}"), lines: vec![hit; MAX_MATCHES - 1] },
            FoundFile { path: "c".into(), lines: vec![FoundLine::default(); 5] },
        ];
        let sent = Found { query: HOSTILE.into(), names: vec![name; MAX_NAMES + 5], files, ..Found::default() };

        let heard = found(&ok(&sent)).expect("found");

        assert_eq!(heard.query, SAID);
        assert_eq!(heard.names.len(), MAX_NAMES);
        assert_eq!(heard.names[0].path, format!("a{KEPT}"));
        assert_eq!(heard.names[0].indices.len(), heard.names[0].path.chars().count());
        let first = &heard.files[0];
        assert_eq!(first.path, format!("b{KEPT}"));
        assert_eq!(first.lines[0].text, format!("\t{KEPT}"));
        assert_eq!(first.lines[0].ranges.len(), first.lines[0].text.len());
        assert_eq!(heard.files[1].lines.len(), 1);
    }
}
