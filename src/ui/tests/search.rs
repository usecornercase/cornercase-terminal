use super::*;

fn results() -> Rect {
    areas().results
}

fn search(query: &str, results: Vec<ResultRow>) -> Search {
    Search { query: query.into(), results, selected: 0, scroll: 0, hint: "enter goes to feat/search".into() }
}

fn row(name: &str, context: &str) -> ResultRow {
    ResultRow { name: name.into(), context: context.into() }
}

fn found() -> Vec<ResultRow> {
    vec![row("feat/search", "notes"), row("nvim", "notes › feat/search")]
}

fn with(search: Search) -> View<'static> {
    View { overlay: Some(Overlay::Search(search)), ..view(&["cornercase", "notes"]) }
}

fn item(i: usize) -> Position {
    result_item(results(), 2, 0, i).as_position().offset(ratatui::layout::Offset { x: 2, y: 0 })
}

#[test]
fn the_bar_shows_a_placeholder_when_closed() {
    let bar = areas().search;
    let text: String = (bar.x..bar.right())
        .map(|x| render(&view(&[])).backend().buffer()[(x, bar.y)].symbol().to_string())
        .collect();
    assert!(text.contains(SEARCH_PLACEHOLDER), "{text:?}");
}

#[test]
fn renders_the_results_over_both_columns() {
    insta::assert_snapshot!(render(&with(search("feat", found()))).backend());
}

#[test]
fn an_empty_query_keeps_the_columns() {
    insta::assert_snapshot!(render(&with(search("", Vec::new()))).backend());
}

#[test]
fn says_when_nothing_matches() {
    let text: String = render(&with(search("zzz", Vec::new())))
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(text.contains("no matches"), "{text}");
}

#[test]
fn highlights_the_selected_result() {
    assert_eq!(render(&with(search("feat", found()))).backend().buffer()[item(0)].bg, DARK_SURFACE);
}

#[test]
fn highlights_the_hovered_result() {
    let v = View { hover: Some(item(1)), ..with(search("feat", found())) };
    assert_eq!(render(&v).backend().buffer()[item(1)].bg, DARK_HOVER);
}

#[test]
fn marks_the_matching_letters() {
    let s = Search { selected: 1, ..search("feat", found()) };
    assert_eq!(render(&with(s)).backend().buffer()[item(0)].fg, Color::Cyan);
}

#[test]
fn puts_the_cursor_after_the_query() {
    let mut t = render(&with(search("feat", found())));
    let bar = areas().search;
    let x = bar.x + u16::try_from(SEARCH_ICON.chars().count() + "feat".len()).expect("width");
    assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(x, bar.y));
}

#[rstest]
#[case::first(0, Some(0))]
#[case::second(1, Some(1))]
#[case::below_the_last(2, None)]
fn maps_clicks(#[case] row: u16, #[case] expected: Option<usize>) {
    let list = results_list(results());
    assert_eq!(result_hit(results(), 2, 0, Position::new(list.x + 3, list.y + row)), expected);
}

#[test]
fn scrolled_rows_map_to_later_results() {
    let list = results_list(results());
    let items = usize::from(list.height) + 5;
    assert_eq!(result_hit(results(), items, 3, list.as_position()), Some(3));
}
