use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::changes::diff;
use crate::config;
use crate::state;

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
        shared: Vec::new(),
        body: Body::Ready(Arc::new(diff)),
        gaps: HashMap::new(),
        scroll: 0,
        live: true,
        light: false,
        muted: Color::DarkGray,
        soft: Color::Gray,
        tints: TINTS,
        filter: None,
        selection: None,
        icons: false,
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
        state::Widths::default(),
        true,
        config::Sidebar::SideBySide,
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
    assert!(!lines(&view).contains(&Row::File(0)));
    assert!(lines(&view).contains(&Row::File(1)));
    view.headers[0].folded = true;
    assert_eq!(lines(&view), [Row::Header(0), Row::Header(3), Row::File(2)]);
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
    assert_eq!(lines(&view), [Row::Header(3), Row::File(2)]);
}

#[test]
fn draws_the_panel() {
    insta::assert_snapshot!(render(&sample(), None).backend());
}

fn with_icons(view: View) -> View {
    View { icons: true, ..view }
}

#[test]
fn draws_file_rows_with_icons() {
    insta::assert_snapshot!(render(&with_icons(sample()), None).backend());
}

#[test]
fn draws_every_project_rows_with_icons() {
    insta::assert_snapshot!(render(&with_icons(every()), None).backend());
}

#[test]
fn a_viewed_row_mutes_its_icon() {
    let mut view = with_icons(sample());
    view.viewed = vec![true, false, false];
    let terminal = render(&view, None);
    let buffer = terminal.backend().buffer();
    let cells = (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| (x, y)));
    let (x, y) = cells.into_iter().find(|&(x, y)| buffer[(x, y)].symbol() == "\u{e68b}").expect("a rust icon");
    let muted = dim(view.muted).fg;
    assert_eq!(Some(buffer[(x, y)].fg), muted);
}

fn marked_cells(view: &View, mark: &str) -> Vec<(u16, u16, Color)> {
    let terminal = render(view, None);
    let buffer = terminal.backend().buffer();
    let cells = (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| (x, y)));
    cells.filter(|&(x, y)| buffer[(x, y)].symbol() == mark).map(|(x, y)| (x, y, buffer[(x, y)].fg)).collect()
}

#[test]
fn a_shared_file_shows_a_yellow_mark_before_its_counts() {
    let mut view = sample();
    view.shared = vec![true, false, false];

    let marks = marked_cells(&view, "⇄");

    let row = row_of(&view, Row::File(0));
    assert_eq!(marks.len(), 1, "{marks:?}");
    let (x, y, colour) = marks[0];
    assert_eq!((y, colour), (row.y, Color::Yellow));
    let terminal = render(&view, None);
    let plus = (row.left()..row.right()).find(|&x| terminal.backend().buffer()[(x, row.y)].symbol() == "+");
    assert!(plus.is_some_and(|plus| x < plus), "mark at {x}, count at {plus:?}");
}

#[test]
fn a_viewed_shared_file_mutes_its_mark() {
    let mut view = sample();
    (view.shared, view.viewed) = (vec![true, false, false], vec![true, false, false]);

    assert_eq!(
        marked_cells(&view, "⇄").iter().map(|(_, _, colour)| *colour).collect::<Vec<_>>(),
        [dim(view.muted).fg.expect("a colour")]
    );
}

#[test]
fn files_nobody_shares_show_no_mark() {
    assert_eq!(marked_cells(&sample(), "⇄"), vec![]);
}

#[test]
fn shows_the_base_in_commits() {
    let view = View { mode: Mode::Commits, base: Some("origin/main".into()), ..sample() };
    insta::assert_snapshot!(render(&view, None).backend());
}

#[test]
fn a_hovered_hunk_shows_its_actions() {
    let view = sample();
    let line = row_of(&view, Row::Line(0, 0, 2, 0));
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
    let r = row_of(&view, Row::Line(0, 0, 3, 0));
    let buffer = terminal.backend().buffer();
    let x = (0..AREA.width).find(|&x| buffer[(x, r.y)].symbol() == "O").expect("Option on screen");
    assert_eq!((Some(buffer[(x, r.y)].bg), buffer[(2, r.y)].bg), (Some(TINTS.added_word), TINTS.added.expect("tint")));
}

#[test]
fn rows_skip_folded_files() {
    let rows = lines(&sample());
    assert!(rows.contains(&Row::File(1)) && !rows.iter().any(|r| matches!(r, Row::Line(1, ..))));
}

#[test]
fn a_gap_between_hunks_can_be_opened() {
    let view = sample();
    assert!(lines(&view).contains(&Row::Gap(0, 1)));
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

fn glyph_at(terminal: &Terminal<TestBackend>, cell: Rect) -> (String, Color) {
    let c = &terminal.backend().buffer()[(cell.x + 1, cell.y)];
    (c.symbol().to_string(), c.fg)
}

fn staged(staged: Staged) -> View {
    let mut view = sample();
    if let Body::Ready(diff) = &mut view.body {
        let files = &mut Arc::make_mut(diff).files;
        Arc::make_mut(&mut files[1]).staged = Some(staged);
    }
    view
}

mod discard {
    use super::*;

    #[test]
    fn the_icon_shows_on_a_hovered_row_and_turns_red_under_the_pointer() {
        let view = sample();
        let r = row_of(&view, Row::File(1));
        let cell = discard_cell(&view, r);
        let on_row = render(&view, Some(Position::new(10, r.y)));
        assert_eq!(glyph_at(&on_row, cell), ("↺".to_string(), Color::DarkGray));
        let on_cell = render(&view, Some(Position::new(cell.x + 1, r.y)));
        assert_eq!(glyph_at(&on_cell, cell), ("↺".to_string(), Color::Red));
        assert_eq!(glyph_at(&render(&view, None), cell).0, " ");
        insta::assert_snapshot!(on_row.backend());
    }

    #[test]
    fn it_shows_on_a_viewed_row_too() {
        let view = View { viewed: vec![false, true, false], ..sample() };
        let r = row_of(&view, Row::File(1));
        let terminal = render(&view, Some(Position::new(10, r.y)));
        assert_eq!(glyph_at(&terminal, discard_cell(&view, r)).0, "↺");
    }
}

mod slots {
    use super::*;

    #[test]
    fn sit_in_order_before_the_viewed_mark() {
        let view = sample();
        let r = row_of(&view, Row::File(1));
        let (discard, stage) = (discard_cell(&view, r), stage_cell(&view, r));
        assert_eq!((discard.right(), stage.right()), (stage.x, viewed_cell(r).x));
    }

    #[rstest::rstest]
    #[case::commits(Mode::Commits, Scope::Project)]
    #[case::all(Mode::All, Scope::Project)]
    fn commits_and_all_have_none(#[case] mode: Mode, #[case] scope: Scope) {
        let view = View { scope, mode, ..sample() };
        let r = row_of(&view, Row::File(1));
        let hovered = render(&view, Some(Position::new(10, r.y)));
        let line: String =
            (r.x..r.right()).map(|x| hovered.backend().buffer()[(x, r.y)].symbol().to_string()).collect();
        assert!(!line.contains('↺'), "{line}");
        assert_eq!((discard_cell(&view, r), stage_cell(&view, r)), (Rect::default(), Rect::default()));
        let at = Position::new(viewed_cell(r).x.saturating_sub(2), r.y);
        assert_eq!(hit(AREA, &view, at), Some(Hit::File(1)));
    }

    #[rstest::rstest]
    #[case::project(Scope::Project)]
    #[case::every_project(Scope::Every)]
    fn are_hit_in_uncommitted_and_every_project(#[case] scope: Scope) {
        let view = View { scope, ..sample() };
        let r = row_of(&view, Row::File(1));
        let (discard, stage) = (discard_cell(&view, r), stage_cell(&view, r));
        assert_eq!(hit(AREA, &view, Position::new(discard.x + 1, r.y)), Some(Hit::Discard(1)));
        assert_eq!(hit(AREA, &view, Position::new(stage.x + 1, r.y)), Some(Hit::Stage(1)));
    }
}

mod stage {
    use super::*;

    const GREEN: Color = Color::Green;

    #[rstest::rstest]
    #[case::unstaged_at_rest(Staged::No, false, false, " ", GREEN)]
    #[case::unstaged_on_the_row(Staged::No, true, false, "+", Color::DarkGray)]
    #[case::unstaged_under_the_pointer(Staged::No, true, true, "+", GREEN)]
    #[case::partly_at_rest(Staged::Partly, false, false, "◐", GREEN)]
    #[case::partly_on_the_row(Staged::Partly, true, false, "◐", GREEN)]
    #[case::partly_under_the_pointer(Staged::Partly, true, true, "+", GREEN)]
    #[case::staged_at_rest(Staged::Yes, false, false, "●", GREEN)]
    #[case::staged_on_the_row(Staged::Yes, true, false, "●", GREEN)]
    #[case::staged_under_the_pointer(Staged::Yes, true, true, "−", Color::Yellow)]
    fn the_glyph(
        #[case] state: Staged,
        #[case] on_row: bool,
        #[case] under: bool,
        #[case] glyph: &str,
        #[case] colour: Color,
    ) {
        let view = staged(state);
        let r = row_of(&view, Row::File(1));
        let cell = stage_cell(&view, r);
        let pointer = match (on_row, under) {
            (_, true) => Some(Position::new(cell.x + 1, r.y)),
            (true, false) => Some(Position::new(10, r.y)),
            _ => None,
        };
        let (shown, fg) = glyph_at(&render(&view, pointer), cell);
        assert_eq!(shown, glyph);
        if glyph != " " {
            assert_eq!(fg, colour);
        }
    }

    #[test]
    fn an_unread_file_counts_as_unstaged() {
        let view = sample();
        let r = row_of(&view, Row::File(1));
        let cell = stage_cell(&view, r);
        let under = render(&view, Some(Position::new(cell.x + 1, r.y)));
        assert_eq!(glyph_at(&under, cell), ("+".to_string(), GREEN));
    }
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
    assert_eq!(first, lines(&sample())[2]);
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
        let rows = lines(&filtered(&sample(), "!*.lock"));
        assert!(rows.contains(&Row::File(0)) && !rows.contains(&Row::File(1)) && rows.contains(&Row::File(2)));
    }

    #[test]
    fn an_empty_query_hides_nothing() {
        assert_eq!(lines(&filtered(&sample(), " ")).len(), lines(&sample()).len());
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

mod selecting {
    use super::*;
    use crate::changes::selection::{At, Spot};
    use crate::text::stretch::{Point, Stretch};

    fn spot(hunk: usize, line: usize) -> Spot {
        Spot { hunk, at: At::Line(line) }
    }

    fn selecting(from: Spot, to: Spot) -> View {
        let view = sample();
        View { selection: Some(Selected { file: 0, from, to, stretch: None }), ..view }
    }

    fn text_of(terminal: &Terminal<TestBackend>, y: u16) -> String {
        (0..AREA.width).map(|x| terminal.backend().buffer()[(x, y)].symbol().to_string()).collect()
    }

    #[test]
    fn line_rows_are_pressable_and_the_other_rows_keep_their_hits() {
        let view = sample();
        let line = row_of(&view, Row::Line(0, 0, 2, 0));
        assert_eq!(hit(AREA, &view, Position::new(8, line.y)), Some(Hit::Line(0, spot(0, 2))));
        assert_eq!(hit(AREA, &view, Position::new(20, line.y)), Some(Hit::Text(0, spot(0, 2))));
        let hunk = row_of(&view, Row::Hunk(0, 0));
        assert_eq!(hit(AREA, &view, Position::new(hunk.x + 2, hunk.y)), None);
        let file = row_of(&view, Row::File(0));
        assert_eq!(hit(AREA, &view, Position::new(10, file.y)), Some(Hit::File(0)));
    }

    #[test]
    fn unchanged_lines_a_gap_shows_are_pressable() {
        let mut view = sample();
        let lines: Vec<GapLine> =
            (14..44).map(|n| GapLine { old: n, new: n, text: format!("line {n}"), raw: None, syntax: None }).collect();
        view.gaps.insert((0, 1), Arc::new(lines));
        let r = row_of(&view, Row::GapLine(0, 1, 2, 0));
        let gap = Spot { hunk: 1, at: At::Gap(2) };
        assert_eq!(hit(AREA, &view, Position::new(8, r.y)), Some(Hit::Line(0, gap)));
        assert_eq!(hit(AREA, &view, Position::new(20, r.y)), Some(Hit::Text(0, gap)));
    }

    #[test]
    fn a_selection_marks_its_rows_only_and_keeps_the_tint() {
        let view = selecting(spot(0, 1), spot(0, 3));
        let terminal = render(&view, None);
        let buffer = terminal.backend().buffer();
        let marked = |l| {
            let r = row_of(&view, Row::Line(0, 0, l, 0));
            (buffer[(r.x, r.y)].symbol().to_string(), buffer[(r.x + 1, r.y)].fg, buffer[(r.x + 2, r.y)].bg)
        };
        assert_eq!(marked(0).0, " ");
        assert_eq!(marked(1).0, "▌");
        assert_eq!(marked(2).0, "▌");
        assert_eq!(marked(3), ("▌".to_string(), Color::Cyan, TINTS.added.expect("tint")));
        assert_eq!(marked(4).0, " ");
    }

    #[test]
    fn the_footer_names_the_lines_and_offers_ask_agent_and_copy() {
        let view = selecting(spot(0, 3), spot(0, 4));
        let footer = text_of(&render(&view, None), parts(AREA, &view).footer.y);
        assert!(footer.contains("lines 10-11") && footer.contains(" ask agent   copy "), "{footer}");
        assert!(!footer.contains("collapse all") && !footer.contains("viewed"), "{footer}");
        assert_eq!(fold_all(AREA, &view), Rect::default());
    }

    #[rstest::rstest]
    #[case::one_line(spot(0, 3), spot(0, 3), "line 10 ")]
    #[case::a_range(spot(0, 3), spot(0, 5), "lines 10-12")]
    #[case::only_removed(spot(0, 1), spot(0, 2), "removed lines 10-")]
    fn the_footer_label(#[case] from: Spot, #[case] to: Spot, #[case] expected: &str) {
        let view = selecting(from, to);
        let footer = text_of(&render(&view, None), parts(AREA, &view).footer.y);
        assert!(footer.contains(expected), "{footer}");
    }

    #[test]
    fn the_footer_buttons_hit() {
        let view = selecting(spot(0, 2), spot(0, 2));
        let actions = lines_actions(AREA, &view);
        let hits: Vec<_> = actions.iter().map(|(_, r)| hit(AREA, &view, Position::new(r.x + 1, r.y))).collect();
        assert_eq!(hits, [Some(Hit::Lines(LinesAction::Ask)), Some(Hit::Lines(LinesAction::Copy))]);
        assert_eq!(lines_actions(AREA, &sample()).len(), 0);
    }

    #[rstest::rstest]
    #[case::above_the_file_is_its_first_line(Row::Header(0), spot(0, 0))]
    #[case::a_line(Row::Line(0, 0, 2, 0), spot(0, 2))]
    #[case::a_hunk_header_is_the_line_above(Row::Hunk(0, 1), spot(0, 5))]
    #[case::the_spacer_is_the_last_line(Row::Spacer, spot(1, 4))]
    fn line_near_maps_a_row_to_a_line_of_the_file(#[case] row: Row, #[case] expected: Spot) {
        let view = sample();
        let y = if row == Row::Header(0) { 0 } else { row_of(&view, row).y };
        assert_eq!(line_near(AREA, &view, 0, y), Some(expected));
    }

    #[rstest::rstest]
    #[case::above_the_file_is_the_start_of_its_first_line(Row::Header(0), 0, Stretch::within(spot(0, 0), 0..0))]
    #[case::a_hunk_header_is_the_end_of_the_line_above(Row::Hunk(0, 1), 3, Stretch::within(spot(0, 5), 1..1))]
    #[case::the_spacer_is_the_end_of_the_last_line(Row::Spacer, 3, Stretch::within(spot(1, 4), 1..1))]
    #[case::a_cell_of_the_code(Row::Line(0, 0, 2, 0), 15, Stretch::within(spot(0, 2), 4..5))]
    #[case::the_gutter_is_the_start_of_the_line(Row::Line(0, 0, 2, 0), 8, Stretch::within(spot(0, 2), 0..0))]
    #[case::past_the_end_is_the_end_of_the_line(Row::Line(0, 0, 2, 0), 55, Stretch::within(spot(0, 2), 26..26))]
    fn stretch_near_maps_a_pointer_to_the_chars_of_the_file(
        #[case] row: Row,
        #[case] x: u16,
        #[case] expected: Stretch<Spot>,
    ) {
        let view = sample();
        let y = if row == Row::Header(0) { 0 } else { row_of(&view, row).y };
        assert_eq!(stretch_near(AREA, &view, 0, Position::new(x, y)), Some(expected));
    }

    fn stretching(from: (Spot, usize), to: (Spot, usize)) -> View {
        let stretch = Stretch { from: Point { line: from.0, offset: from.1 }, to: Point { line: to.0, offset: to.1 } };
        let view = sample();
        View { selection: Some(Selected { file: 0, from: from.0, to: to.0, stretch: Some(stretch) }), ..view }
    }

    #[test]
    fn a_character_selection_reverses_its_cells_and_keeps_the_tint_without_the_line_look() {
        let view = stretching((spot(0, 3), 4), (spot(0, 3), 7));
        let terminal = render(&view, None);
        let buffer = terminal.backend().buffer();
        let r = row_of(&view, Row::Line(0, 0, 3, 0));
        let reversed = |x: u16| buffer[(x, r.y)].modifier.contains(Modifier::REVERSED);
        assert_eq!(
            (reversed(r.x + 14), reversed(r.x + 15), reversed(r.x + 17), reversed(r.x + 18)),
            (false, true, true, false)
        );
        assert_eq!(buffer[(r.x + 2, r.y)].bg, TINTS.added.expect("tint"));
        assert_eq!((buffer[(r.x, r.y)].symbol(), buffer[(r.x + 1, r.y)].fg), (" ", Color::DarkGray));
    }

    #[test]
    fn the_footer_names_the_lines_the_characters_cover() {
        let view = stretching((spot(0, 3), 2), (spot(0, 5), 0));
        let footer = text_of(&render(&view, None), parts(AREA, &view).footer.y);
        assert!(footer.contains("lines 10-11"), "{footer}");
    }

    #[test]
    fn the_edge_scrolls_only_where_there_is_more_to_show() {
        let short = Rect { height: 8, ..AREA };
        let view = sample();
        let body = parts(short, &view).body;
        assert_eq!(edge(short, &view, body.y), None);
        assert_eq!(edge(short, &view, body.bottom() - 1), Some(1));
        let scrolled = View { scroll: 2, ..sample() };
        assert_eq!(edge(short, &scrolled, body.y), Some(-1));
    }

    #[test]
    fn it_is_none_for_a_folded_file_a_filtered_file_or_another_patch() {
        use crate::changes::selection::Selection;
        let view = sample();
        let file = &ready(&view).files[0];
        let selection = Selection {
            workspace: 1,
            path: file.path.clone(),
            digest: file.digest,
            anchor: spot(0, 1),
            end: spot(0, 2),
            stretch: None,
        };
        assert!(selected(&view, &selection).is_some());
        assert!(selected(&View { folded: vec![true; 3], ..sample() }, &selection).is_none());
        assert!(selected(&sample(), &Selection { digest: file.digest + 1, ..selection.clone() }).is_none());
        let filter = FilterView { query: "tests/".into(), focused: false, kept: vec![false, false, true] };
        assert!(selected(&View { filter: Some(filter), ..sample() }, &selection).is_none());
        assert!(selected(&sample(), &Selection { workspace: 2, ..selection }).is_none());
    }

    #[test]
    fn a_continuation_row_is_pressable_and_gives_its_own_chars() {
        let view = sample();
        let r = row_of(&view, Row::Line(0, 0, 3, 1));
        let x = code_x(r, 3);
        assert_eq!(hit(AREA, &view, Position::new(r.x + 1, r.y)), Some(Hit::Line(0, spot(0, 3))));
        assert_eq!(hit(AREA, &view, Position::new(x, r.y)), Some(Hit::Text(0, spot(0, 3))));
        assert_eq!(stretch_near(AREA, &view, 0, Position::new(x, r.y)), Some(Stretch::within(spot(0, 3), 48..49)));
        assert_eq!(stretch_near(AREA, &view, 0, Position::new(55, r.y)), Some(Stretch::within(spot(0, 3), 49..49)));
        assert_eq!(line_near(AREA, &view, 0, r.y), Some(spot(0, 3)));
    }

    #[test]
    fn a_character_selection_on_a_continuation_row_reverses_that_row() {
        let view = stretching((spot(0, 3), 40), (spot(0, 3), 49));
        let terminal = render(&view, None);
        let buffer = terminal.backend().buffer();
        let (first, second) = (row_of(&view, Row::Line(0, 0, 3, 0)), row_of(&view, Row::Line(0, 0, 3, 1)));
        let reversed = |r: Rect, x: u16| buffer[(code_x(r, 3) + x, r.y)].modifier.contains(Modifier::REVERSED);
        assert_eq!((reversed(first, 39), reversed(first, 40), reversed(first, 47)), (false, true, true));
        assert_eq!((reversed(second, 0), reversed(second, 1)), (true, false));
    }

    #[test]
    fn every_row_of_a_line_keeps_its_tint_and_bar_and_only_the_first_its_numbers() {
        let view = sample();
        let terminal = render(&view, None);
        let buffer = terminal.backend().buffer();
        let (first, second) = (row_of(&view, Row::Line(0, 0, 3, 0)), row_of(&view, Row::Line(0, 0, 3, 1)));
        let bar = code_x(second, 3) - 2;
        assert_eq!(buffer[(bar, second.y)].symbol(), "▎");
        assert_eq!(buffer[(bar, first.y)].symbol(), "▎");
        assert_eq!(buffer[(second.x + 2, second.y)].bg, TINTS.added.expect("tint"));
        assert_eq!(text_of(&terminal, second.y)[..bar as usize].trim(), "");
        assert!(text_of(&terminal, first.y).contains("10"));
    }

    #[test]
    fn the_scroll_counts_wrapped_rows() {
        let view = sample();
        let body = parts(AREA, &view).body;
        let wrapped = rows(&view, body.width).len();
        assert!(wrapped > lines(&view).len());
        assert_eq!(max_scroll(AREA, &view), wrapped.saturating_sub(usize::from(body.height)));
    }

    #[test]
    fn wraps_lines_with_their_tint_bar_and_emphasis() {
        let area = Rect::new(0, 0, 40, 18);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
        terminal.draw(|f| draw(f, area, &sample(), None)).expect("draw");
        insta::assert_snapshot!(terminal.backend());
    }

    fn ready(view: &View) -> &Diff {
        view.diff().expect("ready")
    }
}
