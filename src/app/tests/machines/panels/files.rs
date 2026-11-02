use super::*;
use crate::app::tests::files_panel::{action_pos, file, line_pos, lines, results, row_pos, tree_rows};
use crate::changes::diff::Status;
use crate::control::changes::FileStatus;
use crate::control::files::{
    EntryInfo, EntryKind, FolderInfo, Found, FoundFile, FoundLine, ImageInfo, Listing, NameInfo, Read, ReadKind,
    SearchMode,
};
use crate::files::disk::Body;
use crate::ui::files::{self as panel, Action, Screen};

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAQAAAADCAYAAAC09K7GAAAAPklEQVR4AQEzAMz/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADMAAcS8COkAAAAASUVORK5CYII=";

fn entry(name: &str, kind: EntryKind, status: Option<FileStatus>) -> EntryInfo {
    EntryInfo { name: name.into(), kind, status, ..EntryInfo::default() }
}

fn folder(path: &str, entries: Vec<EntryInfo>) -> FolderInfo {
    let dir = format!("/home/ana/web-shop/{path}").into();
    FolderInfo { path: path.into(), dir, entries, ..FolderInfo::default() }
}

fn root() -> FolderInfo {
    let entries = vec![
        entry("src", EntryKind::Folder, Some(FileStatus::Modified)),
        entry("Cargo.toml", EntryKind::File, None),
        entry("README.md", EntryKind::File, Some(FileStatus::Added)),
    ];
    folder("", entries)
}

fn listing(folders: Vec<FolderInfo>) -> Listing {
    Listing { workspace: Some(SHOP), folders }
}

fn view(app: &App) -> panel::View {
    app.files_view().expect("the panel")
}

fn marked(app: &App) -> Vec<(String, Option<Status>)> {
    match view(app).screen {
        Screen::Tree(tree) => tree.rows.into_iter().map(|row| (row.path, row.status)).collect(),
        Screen::Search(_) | Screen::File(_) => Vec::new(),
    }
}

fn shows(app: &App, path: &str) -> bool {
    tree_rows(&view(app)).iter().any(|row| row == path)
}

fn read(text: &str) -> Read {
    let (path, file) = ("Cargo.toml".into(), "/home/ana/web-shop/Cargo.toml".into());
    Read {
        workspace: Some(SHOP),
        path,
        file,
        language: Some("toml".into()),
        text: Some(text.into()),
        ..Read::default()
    }
}

fn body(app: &App) -> Option<Body> {
    match view(app).screen {
        Screen::File(file) => file.content.map(|content| content.body.clone()),
        Screen::Tree(_) | Screen::Search(_) => None,
    }
}

fn coloured(app: &App) -> bool {
    matches!(view(app).screen, Screen::File(file) if file.content.as_ref().is_some_and(|content| content.styles().is_some()))
}

fn note(app: &App) -> Option<String> {
    match view(app).screen {
        Screen::Search(search) => Some(search.note),
        Screen::Tree(_) | Screen::File(_) => None,
    }
}

fn found(query: &str, mode: SearchMode) -> Found {
    Found { workspace: SHOP, query: query.into(), mode, ..Found::default() }
}

fn type_in_field(panels: &mut Panels, at: fn(Rect) -> Rect, text: &str) {
    let pos = at(panel_area(&panels.rig.app)).as_position();
    click(&mut panels.rig.app, pos);
    type_text(&mut panels.rig.app, text);
}

impl Panels {
    fn tree() -> (Self, UnixStream) {
        let (mut panels, mut stream) = Self::showing(Side::Files);
        request(&mut stream);
        tell(&mut stream, &listing(vec![root()]));
        panels.rig.until("the machine's tree shows", |app| shows(app, "README.md"));
        (panels, stream)
    }

    fn reading() -> (Self, UnixStream) {
        let (mut panels, _tree) = Self::tree();
        let pos = row_pos(&panels.rig.app, "Cargo.toml");
        click(&mut panels.rig.app, pos);
        let mut stream = panels.opened();
        let args = json!({ "workspace": SHOP, "path": "Cargo.toml", "follow": true });
        assert_eq!(asked(&mut stream), json!({ "command": "files-read", "boot": BOOT, "args": args }));
        (panels, stream)
    }

    fn with_file(text: &str) -> (Self, UnixStream) {
        let (mut panels, mut stream) = Self::reading();
        tell(&mut stream, &read(text));
        panels.rig.until("the file is read and coloured", coloured);
        (panels, stream)
    }
}

mod the_remote_tree {
    use super::*;

    #[test]
    fn shows_the_folder_of_the_machine_with_its_git_marks() {
        let (panels, _stream) = Panels::tree();
        let app = &panels.rig.app;

        let marks = vec![
            ("src".to_string(), Some(Status::Modified)),
            ("Cargo.toml".to_string(), None),
            ("README.md".to_string(), Some(Status::Added)),
        ];
        assert_eq!(marked(app), marks);
        let view = view(app);
        assert_eq!((view.root.as_str(), view.remote.map(|note| note.machine)), ("~/web-shop", Some("vps".into())));
    }

    #[test]
    fn lists_a_folder_opened_with_a_click_in_a_stream_of_its_own() {
        let (mut panels, mut stream) = Panels::tree();

        let pos = row_pos(&panels.rig.app, "src");
        click(&mut panels.rig.app, pos);
        let mut opened = panels.opened();

        let args = json!({ "workspace": SHOP, "folders": ["", "src"], "follow": true });
        assert_eq!(asked(&mut opened), json!({ "command": "files-list", "boot": BOOT, "args": args }));
        assert!(ended(&mut stream));
        let inside = folder("src", vec![entry("main.rs", EntryKind::File, Some(FileStatus::Modified))]);
        tell(&mut opened, &listing(vec![root(), inside]));
        panels.rig.until("the folder shows", |app| shows(app, "src/main.rs"));
        assert_eq!(marked(&panels.rig.app)[1], ("src/main.rs".into(), Some(Status::Modified)));
    }

    #[test]
    fn shows_a_file_new_there_without_a_click() {
        let (mut panels, mut stream) = Panels::tree();

        let mut later = root();
        later.entries.push(entry("notes.md", EntryKind::File, Some(FileStatus::Untracked)));
        tell(&mut stream, &listing(vec![later]));

        panels.rig.until("the new file shows", |app| shows(app, "notes.md"));
    }

    #[test]
    fn leaves_out_a_listing_of_another_workspace() {
        let (mut panels, mut stream) = Panels::tree();

        tell(&mut stream, &Listing { workspace: Some(BLOG), ..listing(vec![folder("", Vec::new())]) });
        let mut later = root();
        later.entries.push(entry("notes.md", EntryKind::File, None));
        tell(&mut stream, &listing(vec![later]));

        panels.rig.until("the next listing shows", |app| shows(app, "notes.md"));
        assert!(shows(&panels.rig.app, "README.md"));
    }

    #[test]
    fn offers_only_copy_path_on_a_row_with_the_path_there() {
        let (mut panels, _stream) = Panels::tree();
        let app = &mut panels.rig.app;

        let pos = row_pos(app, "README.md");
        right_click(app, pos);
        assert_eq!(menu_labels(app), ["copy path"]);
        pick(app, "copy path");

        assert_eq!(app.take_host_writes().last(), Some(&clipboard::osc52("/home/ana/web-shop/README.md")));
        assert_eq!(toast(app), Some("copied the file's path: it is on vps"));
    }
}

mod the_remote_viewer {
    use super::*;

    #[test]
    fn shows_the_text_read_there_and_then_its_colours() {
        let (panels, _stream) = Panels::with_file("[package]\nname = \"shop\"\n");

        assert_eq!(lines(&view(&panels.rig.app)), ["[package]", "name = \"shop\""]);
    }

    #[test]
    fn swaps_in_what_a_later_read_says() {
        let (mut panels, mut stream) = Panels::with_file("[package]\n");

        tell(&mut stream, &read("[workspace]\n"));

        panels.rig.until("the new text shows", |app| lines(&view(app)) == ["[workspace]"]);
    }

    #[test]
    fn shows_each_new_text_before_its_colours() {
        let (mut panels, mut stream) = Panels::with_file("[package]\n");
        let long = (0..4000).map(|n| format!("key{n} = \"{n}\"")).collect::<Vec<_>>().join("\n");

        let mut changes = 0;

        panels.rig.until("a new text shows while the file keeps changing", |app| {
            changes += 1;
            tell(&mut stream, &read(&format!("[v{changes}]\n{long}")));
            lines(&view(app)).first().is_some_and(|line| line.starts_with("[v"))
        });
    }

    #[test]
    fn ends_its_stream_once_closed() {
        let (mut panels, mut stream) = Panels::with_file("[package]\n");

        let back = panel::back(panel_area(&panels.rig.app)).as_position();
        click(&mut panels.rig.app, back);
        panels.sync();

        assert!(ended(&mut stream));
    }

    #[test]
    fn shows_a_small_picture() {
        let (mut panels, mut stream) = Panels::reading();

        let image = ImageInfo { format: "PNG".into(), width: Some(4), height: Some(3), data: PNG.into() };
        tell(&mut stream, &Read { kind: ReadKind::Image, image: Some(image), text: None, ..read("") });

        panels.rig.until("the picture shows", |app| matches!(body(app), Some(Body::Image(_))));
    }

    #[rstest]
    #[case::missing(ReadKind::Missing, None, Body::Missing)]
    #[case::binary(ReadKind::Binary, Some(12), Body::Binary)]
    #[case::too_large(ReadKind::TooLarge, Some(9_000_000), Body::TooLarge(9_000_000))]
    fn says_what_it_cannot_show_like_here(#[case] kind: ReadKind, #[case] bytes: Option<u64>, #[case] shown: Body) {
        let (mut panels, mut stream) = Panels::reading();

        tell(&mut stream, &Read { kind, bytes, text: None, ..read("") });

        panels.rig.until("the file's state shows", |app| body(app).as_ref() == Some(&shown));
    }

    #[test]
    fn finds_in_what_was_read() {
        let (mut panels, _stream) = Panels::with_file("a = 1\nneedle = 2\nb = \"needle\"\n");

        type_in_field(&mut panels, panel::find_button, "needle");

        assert_eq!(file(&panels.rig.app).find.map(|find| find.count()), Some(2));
    }

    #[test]
    fn copies_a_selected_line_with_osc_52() {
        let (mut panels, _stream) = Panels::with_file("[package]\nname = \"shop\"\n");
        let app = &mut panels.rig.app;

        let line = line_pos(app, 2);
        click(app, line);
        let copy = action_pos(app, Action::Copy);
        click(app, copy);

        assert_eq!(app.take_host_writes().last(), Some(&clipboard::osc52("name = \"shop\"")));
    }

    #[test]
    fn that_ends_leaves_the_tree_s_stream_open() {
        let (mut panels, mut tree) = Panels::tree();
        let pos = row_pos(&panels.rig.app, "Cargo.toml");
        click(&mut panels.rig.app, pos);
        let mut reading = panels.opened();
        request(&mut reading);

        drop(reading);
        panels.rig.until("the viewer's stream ends", |app| down(app).is_some());
        panels.sync();
        let mut later = root();
        later.entries.push(entry("notes.md", EntryKind::File, None));
        tell(&mut tree, &listing(vec![later]));
        let back = panel::back(panel_area(&panels.rig.app)).as_position();
        click(&mut panels.rig.app, back);

        panels.rig.until("the tree's stream still lists", |app| shows(app, "notes.md"));
        assert_eq!((panels.streams(), panels.down()), (2, None));
    }

    #[test]
    fn says_nothing_of_the_tree_s_stream_while_a_file_shows() {
        let (mut panels, tree) = Panels::tree();
        drop(tree);
        panels.rig.until("the tree's stream ends", |app| down(app).is_some());

        let pos = row_pos(&panels.rig.app, "Cargo.toml");
        click(&mut panels.rig.app, pos);

        assert_eq!(panels.down(), None);
    }

    #[test]
    fn goes_full_screen() {
        let (mut panels, _stream) = Panels::with_file("[package]\n");

        let button = panel::full_screen_button(panel_area(&panels.rig.app)).as_position();
        click(&mut panels.rig.app, button);

        assert!(panels.rig.app.file_full_screen());
    }
}

mod the_remote_search {
    use super::*;

    #[test]
    fn asks_once_for_the_last_query_and_shows_its_answer() {
        let (mut panels, _tree) = Panels::tree();

        type_in_field(&mut panels, panel::field, "a");
        let mut first = panels.opened();
        type_text(&mut panels.rig.app, "bc");
        let mut last = panels.opened();

        let args = |query: &str| json!({ "workspace": SHOP, "query": query, "mode": "text" });
        assert_eq!(asked(&mut first), json!({ "command": "files-search", "boot": BOOT, "args": args("a") }));
        assert!(ended(&mut first));
        assert_eq!(asked(&mut last), json!({ "command": "files-search", "boot": BOOT, "args": args("abc") }));
        let line = FoundLine { number: 2, text: "abc = 1".into(), ranges: vec![[0, 3]] };
        let files = vec![FoundFile { path: "src/lib.toml".into(), lines: vec![line] }];
        tell(&mut last, &Found { files, matches: 1, ..found("abc", SearchMode::Text) });
        panels.rig.until("the answer shows", |app| results(&view(app)).len() == 2);
        assert_eq!(panels.streams(), 3);
    }

    #[test]
    fn that_ends_without_an_answer_says_so_and_is_not_asked_again() {
        let (mut panels, _tree) = Panels::tree();
        type_in_field(&mut panels, panel::field, "abc");
        let mut search = panels.opened();
        request(&mut search);

        drop(search);
        panels.rig.until("the search is given up", |app| toast(app) == Some("vps did not answer the search"));
        panels.sync();

        assert_eq!(note(&panels.rig.app).as_deref(), Some("nothing found"));
        assert_eq!(panels.streams(), 2);
    }

    #[test]
    fn shows_the_answer_to_a_query_longer_than_the_machine_s_text_is_kept() {
        let (mut panels, _tree) = Panels::tree();
        let query = "abc".repeat(90);
        type_in_field(&mut panels, panel::field, &query);
        let mut search = panels.opened();
        request(&mut search);

        let line = FoundLine { number: 2, text: "abc = 1".into(), ranges: vec![[0, 3]] };
        let files = vec![FoundFile { path: "src/lib.toml".into(), lines: vec![line] }];
        tell(&mut search, &Found { files, matches: 1, ..found(&query, SearchMode::Text) });

        panels.rig.until("the answer shows", |app| results(&view(app)).len() == 2);
    }

    #[test]
    fn asks_for_names_in_the_names_mode() {
        let (mut panels, _tree) = Panels::tree();

        type_in_field(&mut panels, panel::mode_button, "main");
        let mut stream = panels.opened();

        assert_eq!(request(&mut stream)["args"]["mode"], "names");
        let names = vec![NameInfo { path: "src/main.rs".into(), indices: vec![4, 5, 6, 7] }];
        tell(&mut stream, &Found { names, matches: 1, ..found("main", SearchMode::Names) });
        let shown =
            |app: &App| matches!(&results(&view(app))[..], [panel::Found::Name { path, .. }] if path == "src/main.rs");
        panels.rig.until("the name shows", shown);
    }

    #[test]
    fn opens_a_text_result_there_at_its_line() {
        let (mut panels, _tree) = Panels::tree();
        type_in_field(&mut panels, panel::field, "abc");
        let mut search = panels.opened();
        request(&mut search);
        let line = FoundLine { number: 2, text: "abc = 1".into(), ranges: vec![[0, 3]] };
        let files = vec![FoundFile { path: "Cargo.toml".into(), lines: vec![line] }];
        tell(&mut search, &Found { files, matches: 1, ..found("abc", SearchMode::Text) });
        panels.rig.until("the answer shows", |app| results(&view(app)).len() == 2);

        let body = panel::parts(panel_area(&panels.rig.app)).body;
        click(&mut panels.rig.app, Position::new(body.x + 4, body.y + 1));
        let mut stream = panels.opened();

        assert_eq!(request(&mut stream)["args"]["path"], "Cargo.toml");
        assert_eq!(file(&panels.rig.app).selection, Some((2, 2)));
    }
}

mod a_fallen_machine {
    use super::*;

    fn fallen() -> Panels {
        let (mut panels, mut stream) = Panels::tree();
        panels.fall();
        assert!(ended(&mut stream));
        panels
    }

    #[test]
    fn keeps_the_tree_and_says_so() {
        let panels = fallen();

        assert!(shows(&panels.rig.app, "README.md"));
        assert!(panels.down().is_some());
    }

    #[rstest]
    #[case::opening_a_folder("src")]
    #[case::opening_a_file("Cargo.toml")]
    fn sends_nothing_when(#[case] row: &str) {
        let mut panels = fallen();

        let pos = row_pos(&panels.rig.app, row);
        click(&mut panels.rig.app, pos);
        panels.sync();

        assert_eq!((panels.streams(), toast(&panels.rig.app)), (1, Some("vps unreachable; nothing was sent")));
        assert!(matches!(view(&panels.rig.app).screen, Screen::Tree(_)));
    }

    #[test]
    fn takes_no_search() {
        let mut panels = fallen();

        type_in_field(&mut panels, panel::field, "abc");
        panels.sync();

        assert_eq!((panels.streams(), toast(&panels.rig.app)), (1, Some("vps unreachable; nothing was sent")));
    }
}
