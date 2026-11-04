use super::*;

fn results() -> Rect {
    areas().results
}

fn search(query: &str, results: Vec<ResultRow>) -> Search {
    Search {
        query: query.into(),
        results,
        selected: 0,
        scroll: 0,
        hint: "enter goes to feat/search".into(),
        sessions: false,
        searching: false,
    }
}

fn row(name: &str, context: &str) -> ResultRow {
    ResultRow { name: name.into(), context: context.into(), detail: String::new(), hit: None, muted: false }
}

fn found() -> Vec<ResultRow> {
    vec![row("feat/search", "notes"), row("nvim", "notes › feat/search")]
}

fn with(search: Search) -> View<'static> {
    View { overlay: Some(Overlay::Search(search)), ..view(&["cornercase", "notes"]) }
}

fn item(i: usize) -> Position {
    result_item(results(), 2, 0, i, 1).as_position().offset(ratatui::layout::Offset { x: 2, y: 0 })
}

#[test]
fn the_bar_shows_a_placeholder_when_closed() {
    let bar = areas().search;
    let text: String =
        (bar.x..bar.right()).map(|x| render(&view(&[])).backend().buffer()[(x, bar.y)].symbol().to_string()).collect();
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
    assert_eq!(result_hit(results(), 2, 0, Position::new(list.x + 3, list.y + row), 1), expected);
}

#[test]
fn scrolled_rows_map_to_later_results() {
    let list = results_list(results());
    let items = usize::from(list.height) + 5;
    assert_eq!(result_hit(results(), items, 3, list.as_position(), 1), Some(3));
}

mod sessions {
    use super::*;

    fn said(name: &str, hit: std::ops::Range<usize>, context: &str, detail: &str) -> ResultRow {
        ResultRow { name: name.into(), context: context.into(), detail: detail.into(), hit: Some(hit), muted: false }
    }

    fn in_sessions(query: &str, results: Vec<ResultRow>) -> Search {
        Search { sessions: true, hint: "enter goes to pane 4".into(), ..search(query, results) }
    }

    fn two_matches() -> Vec<ResultRow> {
        vec![
            said("the checkout tax bug is in rounding", 4..16, "notes › feat/search › claude", "claude · reply"),
            said("migration failed: duplicate key", 0..16, "notes › main › zsh", "zsh · screen"),
        ]
    }

    fn line(t: &Terminal<TestBackend>, y: u16) -> String {
        let buffer = t.backend().buffer();
        (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>().trim_end().to_string()
    }

    fn text(t: &Terminal<TestBackend>) -> String {
        t.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
    }

    #[test]
    fn renders_two_lines_per_result() {
        insta::assert_snapshot!(render(&with(in_sessions("checkout tax", two_matches()))).backend());
    }

    #[test]
    fn the_bar_shows_the_toggle_lit_in_sessions_and_a_placeholder() {
        let bar = areas().search;
        let button = mode_button(bar);
        let (names, sessions) = (render(&with(search("", Vec::new()))), render(&with(in_sessions("", Vec::new()))));
        assert!(line(&names, bar.y).contains("projects, workspaces, tabs"));
        assert!(line(&sessions, bar.y).contains("what was said in agents and panes"));
        assert_eq!(names.backend().buffer()[button.as_position()].bg, DARK_SURFACE);
        assert_eq!(
            sessions.backend().buffer()[button.as_position().offset(ratatui::layout::Offset { x: 1, y: 0 })].bg,
            Color::Cyan
        );
    }

    #[test]
    fn the_button_is_three_cells_at_the_end_of_the_bar() {
        let bar = areas().search;
        let button = mode_button(bar);
        assert_eq!((button.width, button.right(), button.y), (3, bar.right(), bar.y));
    }

    #[test]
    fn lights_the_match_in_the_snippet() {
        let list = results_list(results());
        let t = render(&with(Search { selected: 1, ..in_sessions("checkout tax", two_matches()) }));
        let at = |dx| t.backend().buffer()[(list.x + dx, list.y)].fg;
        assert_eq!((at(2), at(6)), (Color::Reset, Color::Cyan));
    }

    #[test]
    fn says_it_is_searching_instead_of_that_nothing_matches() {
        let search = Search { searching: true, hint: "searching…".into(), ..in_sessions("zzz", Vec::new()) };
        let shown = text(&render(&with(search)));
        assert!(shown.contains("searching…") && !shown.contains("no matches"), "{shown}");
    }

    #[test]
    fn maps_clicks_on_either_line_of_a_result() {
        let list = results_list(results());
        let at = |row| result_hit(results(), 2, 0, Position::new(list.x + 3, list.y + row), SESSIONS_PITCH);
        assert_eq!((at(0), at(1), at(2), at(3), at(4)), (Some(0), Some(0), Some(1), Some(1), None));
    }

    #[test]
    fn a_selected_result_marks_both_lines() {
        let list = results_list(results());
        let t = render(&with(in_sessions("checkout tax", two_matches())));
        let buffer = t.backend().buffer();
        assert_eq!((buffer[(list.x, list.y)].symbol(), buffer[(list.x, list.y + 1)].symbol()), ("▌", "▌"));
        assert_eq!(buffer[(list.x, list.y + 2)].symbol(), " ");
    }

    #[test]
    fn the_compact_bar_keeps_the_toggle() {
        let t = render_sized(&with(in_sessions("", Vec::new())), SMALL.width, SMALL.height);
        let row = line(&t, 1);
        assert!(row.contains("what was said") && row.ends_with(" ❯"), "{row:?}");
    }
}

mod settings {
    use super::*;

    fn rows() -> Vec<ResultRow> {
        vec![
            row("prefix-lab", ""),
            row("UI › prefix key", "off · settings"),
            ResultRow {
                muted: true,
                ..row("Remote › Discord › forum mode", "shows once a phone is paired · settings")
            },
        ]
    }

    #[test]
    fn renders_after_the_names() {
        insta::assert_snapshot!(render(&with(search("prefix", rows()))).backend());
    }

    #[test]
    fn renders_in_a_narrow_window() {
        insta::assert_snapshot!(render_sized(&with(search("prefix", rows())), 60, 20).backend());
    }

    #[test]
    fn renders_in_a_compact_window() {
        insta::assert_snapshot!(render_sized(&with(search("prefix", rows())), 44, 24).backend());
    }

    #[test]
    fn a_hidden_row_is_drawn_muted() {
        let view = with(search("zzz", rows()));
        let area = result_item(results(), 3, 0, 2, 1);

        let terminal = render(&view);
        let cell = &terminal.backend().buffer()[(area.x + 2, area.y)];

        assert_eq!(cell.style().fg, Some(view.muted));
    }
}
