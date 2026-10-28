use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::changes::diff;
use crate::files::find::Find;
use crate::files::{self, Top, disk};

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
        icons: false,
        soft: Color::Gray,
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
        body: Body::text(lines, styles, source.into()),
    };
    let gutter = files::gutter(&diff::parse(PATCH)[0], 4);
    View {
        root: "~/projects/shop".into(),
        bar: Bar { query: String::new(), focused: false, mode: Mode::Text },
        screen: Screen::File(Box::new(FileView {
            path: "src/main.rs".into(),
            content: Some(Arc::new(content)),
            gutter: Some(gutter),
            unfolded: unfolded.iter().copied().collect(),
            top: Top::default(),
            jump: None,
            selection,
            stretch: None,
            find: None,
            image: None,
            full_screen: false,
        })),
        light: false,
        muted: Color::DarkGray,
        tint: None,
        icons: false,
        soft: Color::Gray,
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

fn with_icons(view: View) -> View {
    View { icons: true, ..view }
}

#[test]
fn draws_the_tree_with_icons() {
    insta::assert_snapshot!(render(&with_icons(tree())));
}

#[test]
fn the_icon_has_its_type_colour_and_the_name_its_status_colour() {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, &with_icons(tree()), None)).expect("draw");
    let buffer = terminal.backend().buffer();
    let cells = (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| (x, y)));
    let (x, y) = cells.into_iter().find(|&(x, y)| buffer[(x, y)].symbol() == "\u{e68b}").expect("a rust icon");
    assert_eq!(
        (buffer[(x, y)].fg, buffer[(x + 2, y)].symbol(), buffer[(x + 2, y)].fg),
        (Color::Red, "m", Color::Yellow)
    );
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
        icons: false,
        soft: Color::Gray,
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
fn draws_name_results_with_icons() {
    insta::assert_snapshot!(render(&with_icons(names())));
}

#[test]
fn draws_text_results_with_icons() {
    insta::assert_snapshot!(render(&with_icons(texts())));
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
    let view = finding("done", false);
    let buffer = drawn_at(&view);
    let row = body(AREA, &view).y + 2;
    let lit: String = (0..AREA.width)
        .filter(|&x| buffer[(x, row)].bg == Color::Cyan && buffer[(x, row)].fg == Color::Black)
        .map(|x| buffer[(x, row)].symbol())
        .collect();
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

mod stretching {
    use super::*;
    use crate::text::stretch::{Point, Stretch};

    fn viewing(stretch: Option<Stretch<u32>>, unfolded: &[u32]) -> View {
        let mut view = opened(Some((2, 2)), unfolded);
        let Screen::File(file) = &mut view.screen else { unreachable!() };
        file.stretch = stretch;
        view
    }

    fn at(view: &View, row: u16, column: u16) -> Position {
        Position::new(code_x(AREA, file_of(view)) + column, body(AREA, view).y + row)
    }

    fn file_of(view: &View) -> &FileView {
        let Screen::File(file) = &view.screen else { unreachable!() };
        file
    }

    #[test]
    fn the_numbers_select_lines_the_text_selects_characters_and_a_blocks_mark_folds() {
        let view = viewing(None, &[]);
        let y = body(AREA, &view).y + 1;
        let mark = mark_cell(AREA, file_of(&view));
        assert_eq!(hit(AREA, &view, Position::new(2, y)), Some(Hit::Line(2)));
        assert_eq!(hit(AREA, &view, Position::new(mark, y)), Some(Hit::Fold(2)));
        assert_eq!(hit(AREA, &view, Position::new(mark + 1, y)), Some(Hit::Line(2)));
        assert_eq!(hit(AREA, &view, at(&view, 1, 0)), Some(Hit::Text(2)));
        assert_eq!(hit(AREA, &view, at(&view, 1, 30)), Some(Hit::Text(2)));
    }

    #[test]
    fn a_cell_stands_for_the_chars_under_it() {
        let view = viewing(None, &[]);
        assert_eq!(stretch_near(AREA, &view, at(&view, 1, 4)), Some(Stretch::within(2, 4..5)));
        assert_eq!(stretch_near(AREA, &view, at(&view, 1, 30)), Some(Stretch::within(2, 10..10)));
        assert_eq!(stretch_near(AREA, &view, Position::new(2, at(&view, 1, 0).y)), Some(Stretch::within(2, 0..0)));
    }

    #[test]
    fn rows_above_and_below_the_body_are_clamped_to_the_first_and_last_shown() {
        let view = viewing(None, &[]);
        let (above, below) = (Position::new(at(&view, 0, 3).x, 0), Position::new(at(&view, 0, 3).x, AREA.bottom() + 5));
        assert_eq!(stretch_near(AREA, &view, above), Some(Stretch::within(1, 3..4)));
        assert_eq!(stretch_near(AREA, &view, below), Some(Stretch::within(4, 1..1)));
    }

    #[test]
    fn a_removed_row_stands_for_the_end_of_the_line_above() {
        let view = viewing(None, &[2]);
        assert_eq!(stretch_near(AREA, &view, at(&view, 1, 3)), Some(Stretch::within(1, 11..11)));
    }

    #[test]
    fn a_character_selection_reverses_its_cells_only() {
        let view = viewing(Some(Stretch::within(2, 4..7)), &[]);
        let buffer = drawn_at(&view);
        let (y, plain) = (at(&view, 1, 0).y, drawn_at(&viewing(None, &[])));
        let reversed = |column: u16| buffer[(at(&view, 1, column).x, y)].modifier.contains(Modifier::REVERSED);
        assert_eq!((reversed(3), reversed(4), reversed(6), reversed(7)), (false, true, true, false));
        let number = buffer[(2, y)].clone();
        assert_eq!(number.fg, Color::DarkGray);
        assert_ne!(buffer[(0, y)].bg, plain[(0, y)].bg, "no surface background");
        assert_ne!(plain[(2, y)].fg, number.fg, "a line selection lights the number");
    }

    #[test]
    fn a_selected_line_break_reverses_to_the_edge() {
        let view = viewing(Some(Stretch { from: Point { line: 1, offset: 3 }, to: Point { line: 2, offset: 2 } }), &[]);
        let buffer = drawn_at(&view);
        let first = body(AREA, &view).y;
        let reversed = |row: u16, column: u16| {
            buffer[(at(&view, row, column).x, first + row)].modifier.contains(Modifier::REVERSED)
        };
        assert!(!reversed(0, 2) && reversed(0, 3) && reversed(0, 30));
        assert!(reversed(1, 0) && reversed(1, 1) && !reversed(1, 2));
    }

    #[test]
    fn the_header_names_the_lines_the_characters_cover() {
        let ends_at_a_line_start = Stretch { from: Point { line: 1, offset: 3 }, to: Point { line: 3, offset: 0 } };
        let head = render(&viewing(Some(ends_at_a_line_start), &[]));
        assert!(head.lines().nth(1).is_some_and(|row| row.contains("lines 1-2")), "{head}");
    }
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
            screen: Screen::File(Box::new(FileView {
                path: "docs/logo.png".into(),
                content: Some(Arc::new(content)),
                gutter: None,
                unfolded: HashSet::new(),
                top: Top::default(),
                jump: None,
                selection: None,
                stretch: None,
                find: None,
                image,
                full_screen: false,
            })),
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
        drawn_at(view)
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

fn drawn_at(view: &View) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, view, None)).expect("draw").buffer.clone()
}

fn finding(query: &str, focused: bool) -> View {
    let mut view = opened(None, &[]);
    if let Screen::File(file) = &mut view.screen {
        let mut find = Find::new(query, 1, focused);
        find.recount(file.content.as_ref().map_or(&[][..], |c| c.lines()));
        file.find = Some(find);
    }
    view
}

mod find {
    use super::*;
    use crate::graphics::Picture;

    fn cell(buffer: &Buffer, x: u16, y: u16) -> (String, Option<Color>, Option<Color>) {
        let c = &buffer[(x, y)];
        (c.symbol().to_string(), c.fg.into(), c.bg.into())
    }

    #[test]
    fn draws_the_field_with_its_count() {
        insta::assert_snapshot!(render(&finding("n", true)));
    }

    #[test]
    fn says_when_nothing_matches() {
        insta::assert_snapshot!(render(&finding("zzz", false)));
    }

    #[test]
    fn shows_the_placeholder_while_empty() {
        let view = finding("", true);
        assert!(render(&view).contains("text in this file"));
        assert!(!render(&view).contains("no matches"));
    }

    #[test]
    fn the_current_match_stands_out_from_the_others() {
        let view = finding("n", false);
        let buffer = drawn_at(&view);
        let body = body(AREA, &view);
        let painted: Vec<&ratatui::buffer::Cell> =
            body.positions().map(|p| &buffer[p]).filter(|c| c.bg == Color::Cyan || c.bg == Color::Yellow).collect();
        let current: Vec<_> = painted.iter().filter(|c| c.bg == Color::Cyan).collect();
        assert_eq!((current.len(), painted.len() - current.len()), (1, 3));
        assert!(current[0].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(body.x + 2 + 3 + 2 + 1, body.y)].bg, Color::Cyan, "the n of fn, on the first line");
    }

    #[test]
    fn the_button_shows_on_a_text_file_only() {
        let at = find_button(AREA).as_position();
        assert_eq!(hit(AREA, &opened(None, &[]), at), Some(Hit::Find));
        assert_ne!(hit(AREA, &tree(), at), Some(Hit::Find));
        assert_ne!(hit(AREA, &texts(), at), Some(Hit::Find));
        let mut loading = opened(None, &[]);
        if let Screen::File(file) = &mut loading.screen {
            file.content = None;
        }
        assert_ne!(hit(AREA, &loading, at), Some(Hit::Find));
        let picture = Picture {
            id: 1,
            format: "png",
            width: 4,
            height: 4,
            bytes: 10,
            rgba: image::RgbaImage::new(1, 1),
            opaque: true,
        };
        let mut image = opened(None, &[]);
        if let Screen::File(file) = &mut image.screen {
            file.content = Some(Arc::new(Content {
                stamp: disk::Stamp::default(),
                language: None,
                body: Body::Image(Arc::new(picture)),
            }));
        }
        assert_ne!(hit(AREA, &image, at), Some(Hit::Find));
    }

    #[test]
    fn the_button_turns_cyan_while_the_field_is_open() {
        let button = find_button(AREA);
        let colour = |view: &View| cell(&drawn_at(view), button.x + 1, button.y).1;
        assert_eq!(colour(&opened(None, &[])), dim(Color::DarkGray).fg);
        assert_eq!(colour(&finding("", false)), Some(Color::Cyan));
    }

    #[test]
    fn hits_the_field_its_buttons_and_the_lines_below_it() {
        let view = finding("n", true);
        let y = find_field(AREA).y;
        assert_eq!(hit(AREA, &view, find_close(AREA).as_position()), Some(Hit::FindClose));
        assert_eq!(hit(AREA, &view, find_next(AREA).as_position()), Some(Hit::FindNext));
        assert_eq!(hit(AREA, &view, find_previous(AREA).as_position()), Some(Hit::FindPrevious));
        assert_eq!(hit(AREA, &view, Position::new(5, y)), Some(Hit::FindField));
        assert_eq!(hit(AREA, &view, Position::new(2, body(AREA, &view).y)), Some(Hit::Line(1)));
        assert_eq!(body(AREA, &view).y, body(AREA, &opened(None, &[])).y + 1);
        assert_eq!(hit(AREA, &opened(None, &[]), Position::new(5, y)), None);
    }

    #[test]
    fn a_long_path_stops_before_the_find_button() {
        let mut view = finding("", false);
        if let Screen::File(file) = &mut view.screen {
            file.path = "a/very/long/folder/path/that/does/not/fit/in/the/panel/main.rs".into();
        }
        let buffer = drawn_at(&view);
        let button = find_button(AREA);
        assert_eq!(buffer[(button.x - 1, button.y)].symbol(), " ");
    }

    #[test]
    fn the_find_button_never_covers_the_back_button() {
        let narrow = Rect::new(0, 0, 9, 12);
        let view = opened(None, &[]);
        assert_eq!(find_button(narrow), Rect::default());
        assert_eq!(hit(narrow, &view, back(narrow).as_position()), Some(Hit::Back));
    }

    #[test]
    fn a_long_line_draws_only_what_shows() {
        let source = "ab".repeat(2500);
        let content = Content {
            stamp: disk::Stamp::default(),
            language: None,
            body: Body::text(vec![source.clone()], None, source.as_str().into()),
        };
        let mut view = finding("b", false);
        if let Screen::File(file) = &mut view.screen {
            file.content = Some(Arc::new(content));
            let mut find = Find::new("b", 1, false);
            find.recount(file.content.as_ref().map_or(&[][..], |c| c.lines()));
            file.find = Some(find);
        }
        let buffer = drawn_at(&view);
        let y = body(AREA, &view).y;
        let lit = (0..AREA.width).filter(|&x| buffer[(x, y)].bg != Color::Reset).count();
        assert!(lit > 10 && lit < usize::from(AREA.width));
        assert!(!(0..AREA.width).any(|x| buffer[(x, y)].symbol() == "…"));
        if let Screen::File(file) = &mut view.screen {
            file.top = Top { line: 0, row: 99 };
        }
        let buffer = drawn_at(&view);
        let last = body(AREA, &view).bottom() - 1;
        assert_ne!(top(AREA, &view), Top { line: 0, row: 99 });
        assert!((0..AREA.width).any(|x| buffer[(x, last)].symbol() == "…"));
    }

    #[test]
    fn jumps_scroll_only_to_reveal_an_off_screen_line() {
        let source = (1..=60).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        let lines: Vec<String> = source.lines().map(String::from).collect();
        let content = Content {
            stamp: disk::Stamp::default(),
            language: None,
            body: Body::text(lines, None, source.as_str().into()),
        };
        let mut view = finding("line", false);
        if let Screen::File(file) = &mut view.screen {
            file.content = Some(Arc::new(content));
        }
        let shown = u32::from(body(AREA, &view).height);
        assert_eq!(reveal(AREA, &view, (shown, 0)), None);
        assert_eq!(reveal(AREA, &view, (40, 0)), Some(Top { line: 36, row: 0 }));
        assert_eq!(top_spot(AREA, &view), (1, 0));
        if let Screen::File(file) = &mut view.screen {
            file.top = Top { line: 20, row: 0 };
        }
        assert_eq!(top_spot(AREA, &view), (21, 0));
        assert_eq!(reveal(AREA, &view, (22, 0)), None);
        assert_eq!(reveal(AREA, &view, (3, 0)), Some(Top::default()));
    }

    #[test]
    fn a_removed_row_at_the_top_leads_to_the_line_it_sits_before() {
        let mut view = finding("n", false);
        if let Screen::File(file) = &mut view.screen {
            file.unfolded = HashSet::from([2]);
            file.top = Top { line: 1, row: 0 };
        }
        assert_eq!(top_spot(Rect::new(0, 0, 48, 6), &view), (2, 0));
    }
}

mod wrapped {
    use super::*;

    const LONG_OLD: &str = "    old_call(first_argument, second_argument, third_argument, fourth_argument);";
    const LONG_NEW: &str = "    new_call(first_argument, second_argument, third_argument, fourth_argument);";

    fn long_file(unfolded: &[u32]) -> View {
        let patch = format!(
            "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,4 +1,4 @@\n fn main() {{\n-{LONG_OLD}\n+{LONG_NEW}\n     done();\n }}\n"
        );
        let source = format!("fn main() {{\n{LONG_NEW}\n    done();\n}}\n");
        let lines = source.lines().map(String::from).collect();
        let content = Content {
            stamp: disk::Stamp::default(),
            language: None,
            body: Body::text(lines, None, source.as_str().into()),
        };
        let mut view = opened(None, unfolded);
        if let Screen::File(file) = &mut view.screen {
            file.content = Some(Arc::new(content));
            file.gutter = Some(files::gutter(&diff::parse(&patch)[0], 4));
        }
        view
    }

    fn file_of(view: &View) -> &FileView {
        let Screen::File(file) = &view.screen else { unreachable!() };
        file
    }

    fn file_mut(view: &mut View) -> &mut FileView {
        let Screen::File(file) = &mut view.screen else { unreachable!() };
        file
    }

    fn text_row(buffer: &Buffer, y: u16) -> String {
        (0..AREA.width).map(|x| buffer[(x, y)].symbol().to_string()).collect()
    }

    #[test]
    fn wraps_a_long_line_under_its_number() {
        let view = long_file(&[2]);
        insta::assert_snapshot!(
            Terminal::new(TestBackend::new(AREA.width, AREA.height))
                .map(|mut t| {
                    t.draw(|f| draw(f, AREA, &view, None)).expect("draw");
                    t.backend().to_string()
                })
                .expect("terminal")
        );
    }

    #[test]
    fn the_numbers_and_the_removed_dash_stay_on_the_first_row() {
        let view = long_file(&[2]);
        let buffer = drawn_at(&view);
        let y = body(AREA, &view).y;
        let rows: Vec<String> = (0..7).map(|k| text_row(&buffer, y + k)).collect();
        assert!(rows[1].trim_start().starts_with('-') || rows[1].contains(" - "), "{rows:?}");
        assert!(rows[4].trim_start().starts_with('2'), "{rows:?}");
        assert!(!rows[5].trim_start().starts_with(|c: char| c.is_ascii_digit()), "{rows:?}");
    }

    #[test]
    fn a_continuation_row_gives_the_same_hits_as_the_first() {
        let view = long_file(&[]);
        let file = file_of(&view);
        let y = line_y(AREA, &view, 2, 1).expect("the second row shows");
        let (mark, code) = (mark_cell(AREA, file), code_x(AREA, file));
        assert_eq!(hit(AREA, &view, Position::new(2, y)), Some(Hit::Line(2)));
        assert_eq!(hit(AREA, &view, Position::new(code + 3, y)), Some(Hit::Text(2)));
        assert_eq!(hit(AREA, &view, Position::new(mark, y)), Some(Hit::Fold(2)));
    }

    #[test]
    fn a_press_on_a_continuation_row_maps_to_that_row_s_characters() {
        let view = long_file(&[]);
        let y = line_y(AREA, &view, 2, 1).expect("the second row shows");
        let code = code_x(AREA, file_of(&view));
        let first = stretch_near(AREA, &view, Position::new(code, line_y(AREA, &view, 2, 0).expect("first row")));
        let second = stretch_near(AREA, &view, Position::new(code, y));
        assert_eq!(first, Some(Stretch::within(2, 0..1)));
        assert!(second.is_some_and(|s| s.from.offset > 10), "{second:?}");
    }

    #[test]
    fn the_last_top_lays_out_from_the_end_back() {
        let mut view = long_file(&[]);
        file_mut(&mut view).top = Top { line: 3, row: 0 };
        let area = Rect::new(0, 0, 48, 6);
        let last = top(area, &view);
        assert_eq!(last, Top { line: 1, row: 2 }, "a screenful ends on the last line");
    }

    #[test]
    fn a_width_change_keeps_the_top_line() {
        let mut view = long_file(&[]);
        file_mut(&mut view).top = Top { line: 1, row: 2 };
        let (narrow, wide) = (Rect::new(0, 0, 48, 6), Rect::new(0, 0, 100, 6));
        assert_eq!(top(narrow, &view), Top { line: 1, row: 2 });
        assert_eq!(top(wide, &view), Top { line: 1, row: 0 });
    }

    #[test]
    fn the_edge_scrolls_up_while_the_top_is_inside_a_line() {
        let mut view = long_file(&[]);
        let area = Rect::new(0, 0, 48, 6);
        file_mut(&mut view).top = Top { line: 1, row: 1 };
        assert_eq!(edge(area, &view, body(area, &view).y), Some(-1));
        file_mut(&mut view).top = Top::default();
        assert_eq!(edge(area, &view, body(area, &view).y), None);
    }

    fn only(text: &str) -> View {
        let mut view = long_file(&[]);
        let file = file_mut(&mut view);
        file.gutter = None;
        file.content = Some(Arc::new(Content {
            stamp: disk::Stamp::default(),
            language: None,
            body: Body::text(text.lines().map(String::from).collect(), None, text.into()),
        }));
        view
    }

    #[test]
    fn a_jump_waits_for_the_text_and_goes_to_the_row_of_the_match() {
        let tall = (1..=40).map(|n| format!("line {n}\n")).collect::<Vec<_>>().concat();
        let source = format!("{tall}{}needle\n", "word ".repeat(30));
        let mut view = only(&source);
        let area = Rect::new(0, 0, 48, 8);
        let file = file_mut(&mut view);
        let text = file.content.take();
        file.jump = Some(41);
        assert_eq!(top(area, &view), Top::default(), "no text to lay out yet");
        assert_eq!(top_spot(area, &view), (1, 0));
        file_mut(&mut view).content = text;
        assert_eq!(top(area, &view), Top { line: 37, row: 0 }, "the line, with its context");
        let mut find = Find::new("needle", 41, false);
        find.recount(file_of(&view).content.as_ref().map_or(&[][..], |c| c.lines()));
        file_mut(&mut view).find = Some(find);
        assert_eq!(top(area, &view), Top { line: 40, row: 0 }, "the row of the match, with its context");
    }

    #[test]
    fn a_match_on_a_later_row_is_revealed_and_one_on_a_shown_row_is_not() {
        let view = only(&"word ".repeat(60));
        let area = Rect::new(0, 0, 48, 6);
        assert_eq!(reveal(area, &view, (1, 80)), None);
        assert_eq!(reveal(area, &view, (1, 150)), Some(Top::default()));
        assert_eq!(reveal(area, &view, (1, 280)), Some(Top { line: 0, row: 4 }));
    }
}
