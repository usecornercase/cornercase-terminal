use super::*;

fn rows(entries: usize, scroll: usize) -> Rows {
    project_rows(list(), 1, &plain(entries), scroll)
}

fn many(n: usize, projects_scroll: usize) -> View<'static> {
    let names: Vec<String> = (0..n).map(|i| format!("p{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    View { projects_scroll, ..view(&names) }
}

#[test]
fn scrolled_rows_map_to_later_entries() {
    assert_eq!(sidebar_hit(list(), 1, &plain(20), 3, Position::new(3, list().y)), Some(SidebarHit::Select(3)));
}

#[test]
fn the_row_above_the_new_button_is_not_an_entry_when_overflowing() {
    assert_eq!(sidebar_hit(list(), 1, &plain(20), 0, rows(20, 0).more_below().as_position()), None);
}

#[rstest]
#[case::down(0, 3, 3)]
#[case::stops_at_the_last_entry(0, 100, 11)]
#[case::up(5, -3, 2)]
#[case::stops_at_the_first_entry(2, -3, 0)]
#[case::a_stale_scroll_is_clamped_first(100, -3, 8)]
fn the_wheel_moves_within_the_entries(#[case] scroll: usize, #[case] delta: isize, #[case] expected: usize) {
    assert_eq!(rows(20, scroll).scrolled(delta), expected);
}

#[test]
fn nothing_scrolls_when_everything_fits() {
    assert_eq!(rows(3, 0).scrolled(3), 0);
}

#[rstest]
#[case::already_visible(0, 2, 0)]
#[case::below(0, 10, 2)]
#[case::above(10, 4, 4)]
fn reveal_scrolls_as_little_as_it_can(#[case] scroll: usize, #[case] i: usize, #[case] expected: usize) {
    assert_eq!(rows(20, scroll).reveal(i), expected);
}

#[test]
fn hidden_entries_are_counted_above_and_below() {
    assert_eq!(rows(20, 3).hidden(), (0..3, 12..20));
}

#[test]
fn shows_how_many_projects_are_hidden() {
    let t = render(&many(20, 3));
    assert_eq!(row_text(&t, more_above(list())).trim_end(), "  ↑ 3 more");
    assert_eq!(row_text(&t, rows(20, 3).more_below()).trim_end(), "  ↓ 8 more");
}

#[test]
fn shows_no_indicator_when_everything_fits() {
    let t = render(&many(3, 0));
    assert_eq!(row_text(&t, more_above(list())).trim(), "");
    assert_eq!(row_text(&t, rows(20, 0).more_below()).trim(), "");
}

#[test]
fn the_new_button_stays_at_the_bottom_while_scrolled() {
    let t = render(&many(20, 3));
    assert!(row_text(&t, new_project_button(list(), 1, &plain(20))).contains(NEW_BUTTON));
}

#[test]
fn workspace_rows_follow_the_scroll() {
    let pos = Position::new(areas().workspaces_list.x + 4, areas().workspaces_list.y);
    assert_eq!(
        workspace_hit(areas().workspaces_list, areas().pitch, &tabs(&[10]), 2, pos),
        Some(WorkspaceHit::Tab(0, 1))
    );
}

#[test]
fn hidden_workspace_rows_count_workspaces_and_tabs_only() {
    let v = View {
        has_project: true,
        workspaces: vec![WorkspaceEntry {
            collapsed: false,
            name: "main".into(),
            tabs: vec!["zsh".into(); 10],
            behind: 0,
            pull: None,
            removing: false,
            setting_up: false,
            closed: 0,
        }],
        active_tab: Some(0),
        ..view(&["cornercase"])
    };
    let t = render(&v);
    let below = workspace_layout(areas().workspaces_list, 1, &tabs(&[10]), 0).more_below();
    assert_eq!(row_text(&t, below).trim_end(), "  ↓ 2 more");
}
