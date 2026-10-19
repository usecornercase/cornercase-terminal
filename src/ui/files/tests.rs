use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::changes::diff;
use crate::files::{self, disk};

const AREA: Rect = Rect { x: 0, y: 0, width: 48, height: 12 };

fn row(path: &str, depth: u16, dir: bool, open: bool, status: Option<Status>) -> TreeRow {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    TreeRow { name, path: path.into(), depth, dir, open, status }
}

fn tree() -> View {
    let rows = vec![
        row("src", 0, true, true, Some(Status::Modified)),
        row("src/ui", 1, true, false, Some(Status::Added)),
        row("src/main.rs", 1, false, false, Some(Status::Modified)),
        row("src/lib.rs", 1, false, false, None),
        row("README.md", 0, false, false, None),
    ];
    View {
        root: "~/projects/shop".into(),
        bar: Bar { query: String::new(), focused: false, mode: Mode::Text },
        screen: Screen::Tree(TreeView { rows, scroll: 0, last: Some("src/lib.rs".into()), loading: false }),
        light: false,
        muted: Color::DarkGray,
        tint: None,
        glyphs: &crate::ui::icons::PLAIN,
    }
}

const PATCH: &str = "diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,4 @@
 fn main() {
-    old();
+    run();
     done();
 }
";

fn opened(selection: Option<(u32, u32)>, unfolded: &[u32]) -> View {
    let source = "fn main() {\n    run();\n    done();\n}\n";
    let lines = source.lines().map(String::from).collect();
    let styles = crate::highlight::syntax::highlight(source, "rust");
    let content = Content {
        stamp: disk::Stamp::default(),
        language: Some("rust"),
        body: Body::Text { lines, styles, source: source.into() },
    };
    let gutter = files::gutter(&diff::parse(PATCH)[0], 4);
    View {
        root: "~/projects/shop".into(),
        bar: Bar { query: String::new(), focused: false, mode: Mode::Text },
        screen: Screen::File(FileView {
            path: "src/main.rs".into(),
            content: Some(Arc::new(content)),
            gutter: Some(gutter),
            unfolded: unfolded.iter().copied().collect(),
            scroll: 0,
            selection,
            find: None,
            image: None,
            full_screen: false,
        }),
        light: false,
        muted: Color::DarkGray,
        tint: None,
        glyphs: &crate::ui::icons::PLAIN,
    }
}

fn render(view: &View) -> String {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, view, None)).expect("draw");
    let buffer = terminal.backend().buffer();
    (0..AREA.height)
        .map(|y| (0..AREA.width).map(|x| buffer[(x, y)].symbol()).collect::<String>().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn draws_the_tree_with_git_marks() {
    insta::assert_snapshot!(render(&tree()));
}

#[test]
fn draws_a_file_with_its_marks() {
    insta::assert_snapshot!(render(&opened(None, &[])));
}

#[test]
fn unfolds_the_removed_lines_and_shows_the_selection() {
    insta::assert_snapshot!(render(&opened(Some((3, 2)), &[2])));
}

fn searched(mode: Mode, rows: Vec<Found>, selected: Option<usize>) -> View {
    let query = if mode == Mode::Name { "main" } else { "run" };
    View {
        root: "~/projects/shop".into(),
        bar: Bar { query: query.into(), focused: true, mode },
        screen: Screen::Search(SearchView { rows, note: "2 files".into(), selected, scroll: 0 }),
        light: false,
        muted: Color::DarkGray,
        tint: None,
        glyphs: &crate::ui::icons::PLAIN,
    }
}

fn names() -> View {
    let rows = vec![
        Found::Name { path: "src/main.rs".into(), indices: vec![4, 5, 6, 7] },
        Found::Name { path: "docs/maintaining.md".into(), indices: vec![5, 6, 7, 8] },
    ];
    searched(Mode::Name, rows, Some(0))
}

fn texts() -> View {
    let line = |path: &str, number, text: &str, at: usize| Found::Line {
        path: path.into(),
        number,
        text: text.into(),
        ranges: std::iter::once(at..at + 3).collect(),
    };
    let rows = vec![
        Found::File { path: "src/main.rs".into(), count: 2 },
        line("src/main.rs", 2, "run();", 0),
        line("src/main.rs", 9, "fn run() {}", 3),
        Found::File { path: "README.md".into(), count: 1 },
        line("README.md", 4, "cargo run", 6),
    ];
    searched(Mode::Text, rows, Some(2))
}

#[test]
fn a_long_path_keeps_its_end_and_its_lit_letters() {
    let (shown, indices) = tail("src/snapshots/a_long_name.snap", &[0, 14, 15], 12);
    assert_eq!((shown.as_str(), indices), ("…g_name.snap", vec![]));
    let (shown, indices) = tail("src/ui/files.rs", &[7, 8], 10);
    assert_eq!((shown.as_str(), indices), ("…/files.rs", vec![2, 3]));
}

#[test]
fn draws_name_results() {
    insta::assert_snapshot!(render(&names()));
}

#[test]
fn draws_text_results_by_file() {
    insta::assert_snapshot!(render(&texts()));
}

#[test]
fn lights_the_letters_and_the_text_that_matched() {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, &texts(), None)).expect("draw");
    let body = parts(AREA).body;
    let lit = |x: u16, y: u16| terminal.backend().buffer()[(x, y)].bg == Color::Yellow;
    let x = 2 + 5 + 2;
    assert_eq!((lit(x, body.y + 1), lit(x + 2, body.y + 1), lit(x + 3, body.y + 1)), (true, true, false));
}

#[test]
fn hits_the_bar_its_buttons_and_the_results() {
    let view = texts();
    assert_eq!(hit(AREA, &view, mode_button(AREA).as_position()), Some(Hit::Mode));
    assert_eq!(hit(AREA, &view, Position::new(10, field(AREA).y)), Some(Hit::Field));
    assert_eq!(hit(AREA, &view, clear(AREA).as_position()), Some(Hit::Clear));
    assert_eq!(hit(AREA, &view, Position::new(10, parts(AREA).body.y + 4)), Some(Hit::Found(4)));
    assert_eq!(hit(AREA, &tree(), clear(AREA).as_position()), Some(Hit::Field), "nothing to clear");
    assert_eq!(hit(AREA, &tree(), mode_button(AREA).as_position()), Some(Hit::Mode));
}

#[test]
fn the_viewer_lights_what_was_searched() {
    let mut view = opened(None, &[]);
    if let Screen::File(file) = &mut view.screen {
        file.find = Some("done".into());
    }
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, &view, None)).expect("draw");
    let row = parts(AREA).body.y + 2;
    let buffer = terminal.backend().buffer();
    let lit: String =
        (0..AREA.width).filter(|&x| buffer[(x, row)].bg == Color::Yellow).map(|x| buffer[(x, row)].symbol()).collect();
    assert_eq!(lit, "done");
}

#[test]
fn hits_rows_lines_and_marks() {
    let body = parts(AREA).body;
    assert_eq!(hit(AREA, &tree(), Position::new(10, body.y + 2)), Some(Hit::Row(2)));
    assert_eq!(hit(AREA, &tree(), Position::new(10, body.bottom() - 1)), None);
    let file = opened(None, &[]);
    let Screen::File(f) = &file.screen else { unreachable!() };
    assert_eq!(hit(AREA, &file, Position::new(2, body.y + 1)), Some(Hit::Line(2)));
    assert_eq!(hit(AREA, &file, Position::new(mark_cell(AREA, f), body.y + 1)), Some(Hit::Fold(2)));
    assert_eq!(hit(AREA, &file, back(AREA).as_position()), Some(Hit::Back));
    assert_eq!(hit(AREA, &file, close(AREA).as_position()), Some(Hit::Close));
    assert_eq!(hit(AREA, &file, action(AREA, f, Action::Ask).as_position()), Some(Hit::Action(Action::Ask)));
}

mod open_in {
    use super::*;

    const WIDE: Rect = Rect { x: 0, y: 0, width: 64, height: 12 };

    #[test]
    fn a_text_file_offers_it_when_the_panel_has_room() {
        let file = opened(None, &[]);
        let Screen::File(f) = &file.screen else { unreachable!() };
        assert_eq!(hit(WIDE, &file, action(WIDE, f, Action::OpenIn).as_position()), Some(Hit::Action(Action::OpenIn)));
    }

    #[test]
    fn a_narrow_panel_leaves_it_out_to_keep_room_for_the_line_info() {
        let file = opened(None, &[]);
        let Screen::File(f) = &file.screen else { unreachable!() };
        assert_eq!(action(AREA, f, Action::OpenIn), Rect::default());
        assert_ne!(action(AREA, f, Action::Open), Rect::default());
    }
}

#[test]
fn a_removed_line_folds_its_block_back() {
    let view = opened(None, &[2]);
    assert_eq!(hit(AREA, &view, Position::new(10, parts(AREA).body.y + 1)), Some(Hit::Fold(2)));
}

#[test]
fn dragging_past_the_edges_picks_the_nearest_line() {
    let view = opened(None, &[]);
    let body = parts(AREA).body;
    assert_eq!(line_near(AREA, &view, 0), Some(1));
    assert_eq!(line_near(AREA, &view, body.bottom() + 5), Some(4));
}

mod images {
    use rstest::rstest;

    use super::*;
    use crate::graphics::Picture;

    const PLACEHOLDER: char = '\u{10EEEE}';
    const ID: u32 = 0x2A00_00F0;

    fn imaged(body: Body, image: Option<Image>) -> View {
        let content = Content { stamp: disk::Stamp::default(), language: None, body };
        View {
            screen: Screen::File(FileView {
                path: "docs/logo.png".into(),
                content: Some(Arc::new(content)),
                gutter: None,
                unfolded: HashSet::new(),
                scroll: 0,
                selection: None,
                find: None,
                image,
                full_screen: false,
            }),
            ..tree()
        }
    }

    fn logo() -> Body {
        let picture = Picture {
            id: 1,
            format: "png",
            width: 1280,
            height: 720,
            bytes: 250_880,
            rgba: image::RgbaImage::new(1, 1),
            opaque: true,
        };
        Body::Image(Arc::new(picture))
    }

    fn rect() -> Rect {
        image_rect(image_room(AREA, AREA), 6, 3)
    }

    fn drawn(view: &View) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
        terminal.draw(|f| draw(f, AREA, view, None)).expect("draw").buffer.clone()
    }

    fn picture(buffer: &Buffer) -> String {
        (0..AREA.height)
            .map(|y| {
                let row: String = (0..AREA.width)
                    .map(|x| match buffer[(x, y)].symbol() {
                        s if s.starts_with(PLACEHOLDER) => "#",
                        MARKER => "@",
                        s => s,
                    })
                    .collect();
                row.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn draws_the_header_and_the_kitty_placeholders() {
        let buffer = drawn(&imaged(logo(), Some(Image::Placeholders { id: ID, rect: rect() })));
        let first = &buffer[rect().as_position()];

        insta::assert_snapshot!(picture(&buffer));
        assert_eq!(first.fg, Color::Indexed(0xF0));
        assert_eq!(first.diff_option, CellDiffOption::ForcedWidth(std::num::NonZeroU16::MIN));
    }

    #[test]
    fn every_placeholder_names_its_row_and_column() {
        let buffer = drawn(&imaged(logo(), Some(Image::Placeholders { id: ID, rect: rect() })));
        let r = rect();
        let corner = Position::new(r.right() - 1, r.bottom() - 1);

        assert_eq!(buffer[corner].symbol(), kitty::cell(ID, r.height - 1, r.width - 1).0);
    }

    #[test]
    fn draws_markers_that_the_diff_skips() {
        let buffer = drawn(&imaged(logo(), Some(Image::Markers(rect()))));

        insta::assert_snapshot!(picture(&buffer));
        assert!(rect().positions().all(|p| buffer[p].diff_option == CellDiffOption::Skip));
    }

    #[test]
    fn says_why_this_terminal_shows_no_image() {
        let lines = vec![
            "xterm shows images only when it runs as a VT340".to_string(),
            "start it with xterm -ti vt340".to_string(),
        ];

        insta::assert_snapshot!(picture(&drawn(&imaged(logo(), Some(Image::Message(lines))))));
    }

    #[test]
    fn an_unreadable_image_says_why() {
        let body = Body::Unreadable { format: "png", bytes: 40, reason: "unexpected end of file".into() };

        insta::assert_snapshot!(picture(&drawn(&imaged(body, None))));
    }

    #[test]
    fn a_hidden_image_leaves_its_room_blank() {
        let buffer = drawn(&imaged(logo(), Some(Image::Blank)));

        assert!(rect().positions().all(|p| buffer[p].symbol() == " "));
    }

    #[test]
    fn an_image_has_no_open_button_and_no_lines_to_pick() {
        let view = imaged(logo(), None);
        let Screen::File(file) = &view.screen else { unreachable!() };
        let body = parts(AREA).body;

        assert_eq!(action(AREA, file, Action::Open), Rect::default());
        assert_eq!(hit(AREA, &view, action(AREA, file, Action::Ask).as_position()), Some(Hit::Action(Action::Ask)));
        assert_eq!(hit(AREA, &view, Position::new(body.x + 4, body.y + 1)), None);
    }

    #[test]
    fn the_image_stays_off_the_last_row_and_is_centred() {
        let visible = Rect::new(0, 0, AREA.width, AREA.height);
        let room = image_room(AREA, visible);

        assert_eq!(room.bottom(), AREA.bottom() - 1);
        assert_eq!(image_rect(room, 6, 3).x, room.x + (room.width - 6) / 2);
        assert_eq!(image_rect(room, 500, 500), room);
    }

    #[test]
    fn a_client_that_sees_less_gets_less_room() {
        let room = image_room(AREA, Rect::new(0, 0, 20, 8));

        assert_eq!((room.right(), room.bottom()), (20, 7));
    }

    #[rstest]
    #[case::bytes(900, "900 bytes")]
    #[case::kilobytes(250_880, "245 KB")]
    #[case::megabytes(3_250_000, "3.1 MB")]
    fn sizes_read_like_a_file_manager(#[case] bytes: u64, #[case] shown: &str) {
        assert_eq!(size(bytes), shown);
    }
}

fn full(on: bool) -> View {
    let mut view = opened(None, &[]);
    if let Screen::File(file) = &mut view.screen {
        file.full_screen = on;
    }
    view
}

#[test]
fn hits_the_full_screen_button_on_a_file_only() {
    let at = full_screen_button(AREA).as_position();
    assert_eq!(hit(AREA, &opened(None, &[]), at), Some(Hit::FullScreen));
    assert_ne!(hit(AREA, &tree(), at), Some(Hit::FullScreen));
    assert_ne!(hit(AREA, &texts(), at), Some(Hit::FullScreen));
}

#[test]
fn the_full_screen_button_turns_into_restore_and_stays_lit() {
    let cell = |view: &View| {
        let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
        terminal.draw(|f| draw(f, AREA, view, None)).expect("draw");
        let c = terminal.backend().buffer()[(full_screen_button(AREA).x + 1, full_screen_button(AREA).y)].clone();
        (c.symbol().to_string(), c.fg)
    };
    assert_eq!(cell(&full(false)), ("▢".to_string(), dim(Color::DarkGray).fg.expect("a colour")));
    assert_eq!(cell(&full(true)), ("❐".to_string(), Color::Cyan));
}

#[test]
fn a_long_path_stops_before_the_full_screen_button() {
    let mut view = full(false);
    if let Screen::File(file) = &mut view.screen {
        file.path = "a/very/long/folder/name/that/goes/on/and/on/and/on/for/ever/main.rs".into();
    }
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, &view, None)).expect("draw");
    let button = full_screen_button(AREA);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(button.x - 1, button.y)].symbol(), " ");
}

#[test]
fn the_full_screen_button_never_covers_the_back_button() {
    let narrow = Rect::new(0, 0, 9, 12);
    let view = full(false);
    assert!(back(narrow).contains(Position::new(3, 0)));
    assert_eq!(hit(narrow, &view, Position::new(3, 0)), Some(Hit::Back));
    assert!(full_screen_button(narrow).is_empty());
}
