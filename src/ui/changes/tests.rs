use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::changes::diff;

const PATCH: &str = "diff --git a/src/returns.rs b/src/returns.rs
--- a/src/returns.rs
+++ b/src/returns.rs
@@ -9,4 +9,4 @@ impl Address
 /// The first line, when the address has one.
-pub fn street(parts: &[String]) -> &str {
-    parts.first().unwrap()
+pub fn street(parts: &[String]) -> Option<&str> {
+    parts.first().map(String::as_str)
 }
@@ -44,2 +44,6 @@ mod tests
     }
+
+    #[test]
+    fn empty_address_is_rejected() {}
 }
diff --git a/Cargo.lock b/Cargo.lock
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1 +1 @@
-a
+b
";

const TINTS: Tints = Tints {
    removed: Some(Color::Indexed(52)),
    added: Some(Color::Indexed(22)),
    removed_word: Color::Indexed(88),
    added_word: Color::Indexed(28),
};

fn sample() -> View {
    let mut files = diff::parse(PATCH);
    files.iter_mut().for_each(diff::finish);
    let mut untracked = diff::untracked("tests/returns.rs", 4, Some(b"one\n"));
    diff::finish(&mut untracked);
    files.push(untracked);
    let diff = Diff { files: files.into_iter().map(Arc::new).collect() };
    View {
        scope: Scope::Project,
        project: "shop › issue-482".into(),
        owners: vec![1; 3],
        headers: Vec::new(),
        mode: Mode::Uncommitted,
        base: None,
        folded: vec![false, true, true],
        viewed: vec![false, false, false],
        body: Body::Ready(Arc::new(diff)),
        gaps: HashMap::new(),
        scroll: 0,
        live: true,
        light: false,
        muted: Color::DarkGray,
        tints: TINTS,
        filter: None,
    }
}

const AREA: Rect = Rect { x: 0, y: 0, width: 60, height: 24 };

fn render(view: &View, hover: Option<Position>) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, view, hover)).expect("draw");
    terminal
}

fn row_of(view: &View, wanted: Row) -> Rect {
    visible(AREA, view).into_iter().find(|(row, _)| *row == wanted).map(|(_, r)| r).expect("row is visible")
}

fn every() -> View {
    View {
        scope: Scope::Every,
        folded: vec![true; 3],
        headers: vec![
            Header { id: 10, label: "shop".into(), files: 0..2, workspace: false, folded: false },
            Header { id: 1, label: "issue-482".into(), files: 0..1, workspace: true, folded: false },
            Header { id: 2, label: "feat/dark-mode".into(), files: 1..2, workspace: true, folded: false },
            Header { id: 20, label: "api".into(), files: 2..3, workspace: false, folded: false },
        ],
        owners: vec![1, 2, 3],
        ..sample()
    }
}

#[rstest::rstest]
#[case::project_wide(Scope::Project, 140)]
#[case::project_compact(Scope::Project, 80)]
#[case::every_wide(Scope::Every, 140)]
#[case::every_compact(Scope::Every, 80)]
fn scopes_draw_in_both_layouts(#[case] scope: Scope, #[case] columns: u16) {
    let view = if scope == Scope::Every { every() } else { sample() };
    let areas = super::super::layout_with(
        Rect::new(0, 0, columns, 24),
        super::super::Widths::default(),
        true,
        super::super::Sidebar::SideBySide,
    );
    let mut terminal = Terminal::new(TestBackend::new(columns, 24)).expect("terminal");
    terminal.draw(|f| draw(f, areas.changes, &view, None)).expect("draw");
    insta::assert_snapshot!(format!("{scope:?}_{columns}"), terminal.backend());
}

#[test]
fn headers_are_clickable_and_hide_their_own_rows() {
    let mut view = every();
    for index in 0..view.headers.len() {
        assert_eq!(hit(AREA, &view, row_of(&view, Row::Header(index)).as_position()), Some(Hit::Header(index)));
    }
    view.headers[1].folded = true;
    assert!(!rows(&view).contains(&Row::File(0)));
    assert!(rows(&view).contains(&Row::File(1)));
    view.headers[0].folded = true;
    assert_eq!(rows(&view), [Row::Header(0), Row::Header(3), Row::File(2)]);
}

#[test]
fn files_are_indented_beyond_the_project_and_workspace_headers() {
    let view = every();
    let terminal = render(&view, None);
    let buffer = terminal.backend().buffer();
    for (row, indent, symbol) in
        [(Row::Header(0), 1, "▾"), (Row::Header(1), 3, "▾"), (Row::File(0), 4, "▸"), (Row::File(2), 4, "▸")]
    {
        let r = row_of(&view, row);
        assert_eq!(buffer[(r.x + indent, r.y)].symbol(), symbol);
    }
}

#[test]
fn every_project_has_no_mode_tabs_or_base_selector() {
    let view = View { mode: Mode::Commits, base: Some("main".into()), ..every() };
    assert!(base(AREA, &view).is_empty());
    assert_eq!(hit(AREA, &view, tabs(AREA)[0].1.as_position()), None);
    for (scope, r, _) in scopes(AREA, &view) {
        assert_eq!(hit(AREA, &view, r.as_position()), Some(Hit::Scope(scope)));
    }
}

#[test]
fn the_empty_every_project_view_says_where_it_looked() {
    let view =
        View { body: Body::Ready(Arc::new(Diff::default())), headers: Vec::new(), owners: Vec::new(), ..every() };
    insta::assert_snapshot!(render(&view, None).backend());
}

#[test]
fn a_filter_hides_headers_without_matching_files() {
    let view = View {
        filter: Some(FilterView { query: "tests/".into(), focused: false, kept: vec![false, false, true] }),
        ..every()
    };
    assert_eq!(rows(&view), [Row::Header(3), Row::File(2)]);
}

#[test]
fn draws_the_panel() {
    insta::assert_snapshot!(render(&sample(), None).backend());
}

#[test]
fn shows_the_base_in_commits() {
    let view = View { mode: Mode::Commits, base: Some("origin/main".into()), ..sample() };
    insta::assert_snapshot!(render(&view, None).backend());
}

#[test]
fn a_hovered_hunk_shows_its_actions() {
    let view = sample();
    let line = row_of(&view, Row::Line(0, 0, 2));
    let terminal = render(&view, Some(Position::new(10, line.y)));
    let header = row_of(&view, Row::Hunk(0, 0));
    let text: String =
        (0..AREA.width).map(|x| terminal.backend().buffer()[(x, header.y)].symbol().to_string()).collect();
    assert!(text.contains(" open   ask agent   copy "), "{text}");
}

#[test]
fn changed_words_get_a_stronger_tint() {
    let view = sample();
    let terminal = render(&view, None);
    let r = row_of(&view, Row::Line(0, 0, 3));
    let buffer = terminal.backend().buffer();
    let x = (0..AREA.width).find(|&x| buffer[(x, r.y)].symbol() == "O").expect("Option on screen");
    assert_eq!((Some(buffer[(x, r.y)].bg), buffer[(2, r.y)].bg), (Some(TINTS.added_word), TINTS.added.expect("tint")));
}

#[test]
fn rows_skip_folded_files() {
    let rows = rows(&sample());
    assert!(rows.contains(&Row::File(1)) && !rows.iter().any(|r| matches!(r, Row::Line(1, ..))));
}

#[test]
fn a_gap_between_hunks_can_be_opened() {
    let view = sample();
    assert!(rows(&view).contains(&Row::Gap(0, 1)));
    let gap = row_of(&view, Row::Gap(0, 1));
    assert_eq!(hit(AREA, &view, Position::new(20, gap.y)), Some(Hit::Gap(0, 1)));
}

#[rstest::rstest]
#[case::uncommitted(4, 1, Hit::Mode(Mode::Uncommitted))]
#[case::commits(17, 1, Hit::Mode(Mode::Commits))]
#[case::all(26, 1, Hit::Mode(Mode::All))]
#[case::close(58, 0, Hit::Close)]
fn header_clicks(#[case] x: u16, #[case] y: u16, #[case] expected: Hit) {
    assert_eq!(hit(AREA, &sample(), Position::new(x, y)), Some(expected));
}

#[test]
fn clicks_on_a_file_row() {
    let view = sample();
    let r = row_of(&view, Row::File(1));
    assert_eq!(
        (hit(AREA, &view, Position::new(10, r.y)), hit(AREA, &view, Position::new(r.right() - 2, r.y))),
        (Some(Hit::File(1)), Some(Hit::Viewed(1)))
    );
}

#[test]
fn hunk_actions_are_clickable() {
    let view = sample();
    let r = row_of(&view, Row::Hunk(0, 0));
    let (_, ask) = actions(r)[1];
    assert_eq!(hit(AREA, &view, Position::new(ask.x + 1, r.y)), Some(Hit::Action(0, 0, Action::Ask)));
}

#[test]
fn the_base_selector_is_clickable_in_commits() {
    let view = View { mode: Mode::Commits, base: Some("main".into()), ..sample() };
    let r = base(AREA, &view);
    assert_eq!(hit(AREA, &view, Position::new(r.x + 1, r.y)), Some(Hit::Base));
}

#[test]
fn scrolling_moves_the_rows() {
    let short = Rect { height: 8, ..AREA };
    let first = visible(short, &View { scroll: 2, ..sample() })[0].0;
    assert_eq!(first, rows(&sample())[2]);
}

fn filtered(view: &View, query: &str) -> View {
    let Body::Ready(diff) = &view.body else { panic!("a diff") };
    let kept = crate::changes::filter::kept(diff, query);
    View { filter: Some(FilterView { query: query.into(), focused: true, kept }), ..view.clone() }
}

mod filter {
    use super::*;

    #[test]
    fn draws_the_field_and_the_filtered_summary() {
        insta::assert_snapshot!(render(&filtered(&sample(), "returns.rs"), None).backend());
    }

    #[test]
    fn an_empty_field_shows_how_to_filter() {
        insta::assert_snapshot!(render(&filtered(&sample(), ""), None).backend());
    }

    #[test]
    fn its_placeholder_takes_the_muted_colour() {
        let view = View { muted: Color::Indexed(243), ..filtered(&sample(), "") };
        let field = parts(AREA, &view).field;
        let t = render(&view, None);
        let buf = t.backend().buffer();
        let row: String = (field.x..field.right()).map(|x| buf[(x, field.y)].symbol().to_string()).collect();
        let at = row.find(FILTER_PLACEHOLDER).expect("the placeholder shows");
        let column = u16::try_from(row[..at].chars().count()).expect("column");

        assert_eq!(buf[(field.x + column, field.y)].fg, Color::Indexed(243));
    }

    #[test]
    fn shows_the_cursor_in_a_focused_field() {
        let view = filtered(&sample(), "*.rs");
        let mut terminal = render(&view, None);
        let field = parts(AREA, &view).field;
        assert_eq!(
            terminal.get_cursor_position().expect("cursor"),
            Position::new(field.x + 3 + width("*.rs"), field.y)
        );
    }

    #[test]
    fn hides_the_files_that_do_not_match() {
        let rows = rows(&filtered(&sample(), "!*.lock"));
        assert!(rows.contains(&Row::File(0)) && !rows.contains(&Row::File(1)) && rows.contains(&Row::File(2)));
    }

    #[test]
    fn an_empty_query_hides_nothing() {
        assert_eq!(rows(&filtered(&sample(), " ")).len(), rows(&sample()).len());
    }

    #[test]
    fn says_when_no_file_matches() {
        let terminal = render(&filtered(&sample(), "nothing"), None);
        let body = parts(AREA, &filtered(&sample(), "nothing")).body;
        let text: String = (body.x..body.right()).map(|x| terminal.backend().buffer()[(x, body.y)].symbol()).collect();
        assert_eq!(text.trim(), NO_FILE_MATCHES);
    }

    #[rstest::rstest]
    #[case::the_button(Hit::Filter)]
    #[case::the_field(Hit::Query)]
    #[case::its_clear_button(Hit::ClearFilter)]
    fn is_clickable(#[case] expected: Hit) {
        let view = filtered(&sample(), "*.rs");
        let pos = match expected {
            Hit::Filter => filter_button(AREA).as_position(),
            Hit::Query => parts(AREA, &view).field.as_position(),
            _ => clear_filter(AREA, &view).as_position(),
        };
        assert_eq!(hit(AREA, &view, Position::new(pos.x + 1, pos.y)), Some(expected));
    }

    #[test]
    fn the_scope_keeps_every_project_visible_on_a_narrow_panel() {
        let narrow = Rect { width: MIN_WIDTH, ..AREA };
        let view = View { project: "a very long project and workspace name".into(), ..sample() };
        let scopes = scopes(narrow, &view);
        assert!(scopes[1].1.right() <= filter_button(narrow).x);
        assert_eq!(scopes[1].1.width, width(" Every project "));
        assert!(scopes[0].2.ends_with("… "));
        assert_eq!(hit(narrow, &view, scopes[1].1.as_position()), Some(Hit::Scope(Scope::Every)));
    }
}

#[test]
fn shows_an_error() {
    let view = View { body: Body::Failed("cannot compare with nope".into()), ..sample() };
    insta::assert_snapshot!(render(&view, None).backend());
}
