use super::*;

fn grouped() -> View<'static> {
    let mut v = View { active: 1, has_project: true, ..view(&["tmp", "api", "web", "cornercase"]) };
    v.groups = vec![
        GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false },
        GroupEntry { name: "oss".into(), icon: '★', colour: 99, collapsed: false },
    ];
    for (p, g) in [(1, 0), (2, 0), (3, 1)] {
        v.projects[p].group = Some(g);
    }
    v
}

fn sidebar_at(dragged: SidebarRow, y: u16) -> Option<Landing> {
    let rows = grouped().sidebar_rows();
    sidebar_drop(&project_rows(list(), 1, &rows, 0), &rows, dragged, Position::new(list().x + 3, y))
}

fn row(i: u16) -> u16 {
    list().y + i
}

fn spot(group: Option<usize>, before: Option<usize>) -> Spot {
    Spot::Project { group, before }
}

#[test]
fn the_rows_are_a_loose_project_then_two_groups() {
    assert_eq!(
        grouped().sidebar_rows(),
        [
            SidebarRow::Project(0),
            SidebarRow::Gap,
            SidebarRow::Group(0),
            SidebarRow::Project(1),
            SidebarRow::Project(2),
            SidebarRow::Gap,
            SidebarRow::Group(1),
            SidebarRow::Project(3),
        ]
    );
}

#[rstest]
#[case::onto_itself(row(0), None)]
#[case::onto_a_project_below_lands_after_it(row(3), Some((4, spot(Some(0), Some(2)))))]
#[case::onto_a_header_lands_first_in_the_group(row(2), Some((3, spot(Some(0), Some(1)))))]
#[case::onto_the_last_of_a_group_lands_at_its_end(row(4), Some((5, spot(Some(0), None))))]
#[case::onto_the_gap_below_a_group_lands_at_its_end(row(5), Some((5, spot(Some(0), None))))]
#[case::onto_the_last_header(row(6), Some((7, spot(Some(1), Some(3)))))]
#[case::below_every_row(row(9), Some((8, spot(Some(1), None))))]
#[case::outside_the_list(list().bottom(), None)]
fn a_loose_project_dragged_down(#[case] y: u16, #[case] expected: Option<(usize, Spot)>) {
    let expected = expected.map(|(at, spot)| Landing { at, spot });
    assert_eq!(sidebar_at(SidebarRow::Project(0), y), expected);
}

#[rstest]
#[case::onto_a_loose_project_above_lands_before_it(row(0), Some((0, spot(None, Some(0)))))]
#[case::onto_the_gap_below_the_loose_ones_ungroups_it(row(1), Some((1, spot(None, None))))]
#[case::onto_a_header(row(2), Some((3, spot(Some(0), Some(1)))))]
#[case::onto_a_project_above_lands_before_it(row(4), Some((4, spot(Some(0), Some(2)))))]
#[case::onto_its_own_header_stays(row(6), Some((7, spot(Some(1), None))))]
#[case::above_the_list(list().y - 1, Some((0, spot(None, Some(0)))))]
fn a_grouped_project_dragged_up(#[case] y: u16, #[case] expected: Option<(usize, Spot)>) {
    let expected = expected.map(|(at, spot)| Landing { at, spot });
    assert_eq!(sidebar_at(SidebarRow::Project(3), y), expected);
}

#[rstest]
#[case::onto_a_loose_project_lands_first(SidebarRow::Group(1), row(0), Some((2, Spot::Group(0))))]
#[case::onto_a_project_of_a_group_above(SidebarRow::Group(1), row(3), Some((2, Spot::Group(0))))]
#[case::onto_the_gap_right_above_itself(SidebarRow::Group(1), row(5), Some((6, Spot::Group(1))))]
#[case::onto_one_of_its_projects(SidebarRow::Group(1), row(7), None)]
#[case::onto_a_group_below_lands_after_it(SidebarRow::Group(0), row(6), Some((8, Spot::Group(2))))]
#[case::below_every_row(SidebarRow::Group(0), row(9), Some((8, Spot::Group(2))))]
fn a_group_lands_among_groups(#[case] dragged: SidebarRow, #[case] y: u16, #[case] expected: Option<(usize, Spot)>) {
    let expected = expected.map(|(at, spot)| Landing { at, spot });
    assert_eq!(sidebar_at(dragged, y), expected);
}

fn workspaces_at(dragged: WorkspaceRow, row: WorkspaceRow) -> Option<Landing> {
    let (list, tabs) = (Rect { height: 20, ..areas().workspaces_list }, tabs(&[2, 1, 2]));
    let y = workspace_row(list, 1, &tabs, 0, row).y;
    workspace_drop(&workspace_layout(list, 1, &tabs, 0), &tabs, dragged, Position::new(list.x + 3, y))
}

#[rstest]
#[case::onto_a_tab_of_a_workspace_above(WorkspaceRow::Workspace(2), WorkspaceRow::Tab(0, 1), 0, 0)]
#[case::onto_the_plus_tab_of_a_workspace_above(WorkspaceRow::Workspace(2), WorkspaceRow::NewTab(1), 5, 1)]
#[case::onto_a_workspace_below(WorkspaceRow::Workspace(0), WorkspaceRow::Tab(1, 0), 8, 2)]
#[case::onto_the_last_workspace(WorkspaceRow::Workspace(0), WorkspaceRow::Workspace(2), 13, 3)]
fn a_workspace_lands_before_or_after_the_one_under_the_mouse(
    #[case] dragged: WorkspaceRow,
    #[case] under: WorkspaceRow,
    #[case] at: usize,
    #[case] before: usize,
) {
    assert_eq!(workspaces_at(dragged, under), Some(Landing { at, spot: Spot::Workspace(before) }));
}

#[rstest]
#[case::onto_the_next_tab(WorkspaceRow::Tab(0, 0), WorkspaceRow::Tab(0, 1), 3, 2)]
#[case::onto_plus_tab(WorkspaceRow::Tab(0, 0), WorkspaceRow::NewTab(0), 3, 2)]
#[case::onto_its_workspace(WorkspaceRow::Tab(2, 1), WorkspaceRow::Workspace(2), 10, 0)]
#[case::onto_a_workspace_below_stays_in_its_own(WorkspaceRow::Tab(0, 0), WorkspaceRow::Tab(1, 0), 3, 2)]
#[case::onto_a_workspace_above_stays_in_its_own(WorkspaceRow::Tab(2, 1), WorkspaceRow::Tab(0, 0), 10, 0)]
fn a_tab_lands_within_its_workspace(
    #[case] dragged: WorkspaceRow,
    #[case] under: WorkspaceRow,
    #[case] at: usize,
    #[case] before: usize,
) {
    assert_eq!(workspaces_at(dragged, under), Some(Landing { at, spot: Spot::Tab(before) }));
}

#[test]
fn a_tab_dropped_on_itself_goes_nowhere() {
    assert_eq!(workspaces_at(WorkspaceRow::Tab(0, 1), WorkspaceRow::Tab(0, 1)), None);
}

fn many(n: usize, scroll: usize) -> Rows {
    project_rows(list(), 1, &plain(n), scroll)
}

#[rstest]
#[case::the_line_above_scrolls_up(3, more_above(list()).as_position(), Some(-1))]
#[case::the_line_below_scrolls_down(3, many(20, 3).more_below().as_position(), Some(1))]
#[case::the_new_button_scrolls_down(3, many(20, 3).button().as_position(), Some(1))]
#[case::a_row_does_not_scroll(3, Position::new(3, list().y + 2), None)]
#[case::nothing_hidden_above(0, more_above(list()).as_position(), None)]
fn the_edges_of_a_long_list_scroll_it(#[case] scroll: usize, #[case] pos: Position, #[case] expected: Option<isize>) {
    assert_eq!(many(20, scroll).edge(pos), expected);
}

#[test]
fn a_short_list_never_scrolls() {
    assert_eq!(many(3, 0).edge(many(3, 0).more_below().as_position()), None);
}

fn landed_rows(rows: &[SidebarRow], scroll: usize, at: usize) -> (Vec<SidebarRow>, Rect) {
    let layout = project_rows(list(), 1, rows, scroll);
    let landing = Landing { at, spot: Spot::Group(0) };
    landed(rows.to_vec(), &layout, Some(landing), SidebarRow::Gap, SidebarRow::Landing)
}

#[test]
fn the_line_goes_between_two_rows() {
    let (rows, line) = landed_rows(&plain(3), 0, 1);
    assert_eq!(
        (rows, line),
        (
            vec![SidebarRow::Project(0), SidebarRow::Landing, SidebarRow::Project(1), SidebarRow::Project(2)],
            Rect::default()
        )
    );
}

#[test]
fn the_line_takes_the_place_of_a_gap() {
    let rows = grouped().sidebar_rows();
    let (drawn, _) = landed_rows(&rows, 0, 6);
    assert_eq!((drawn.len(), drawn[5]), (rows.len(), SidebarRow::Landing));
}

#[rstest]
#[case::above_the_first_row(0, 0, more_above(list()))]
#[case::above_the_first_row_shown(5, 5, more_above(list()))]
#[case::below_the_last_row(0, 3, Rect::new(list().x, list().y + 3, list().width, 1))]
fn the_line_at_an_end_goes_on_the_line_beside_it(#[case] scroll: usize, #[case] at: usize, #[case] line: Rect) {
    let n = if scroll == 0 { 3 } else { 30 };
    assert_eq!(landed_rows(&plain(n), scroll, at), (plain(n), line));
}

#[test]
fn a_line_out_of_sight_is_not_drawn() {
    assert_eq!(landed_rows(&plain(30), 5, 2), (plain(30), Rect::default()));
}

#[test]
fn a_line_on_the_more_label_replaces_it() {
    let names: Vec<String> = (0..20).map(|i| format!("p{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let landing = Landing { at: 9, spot: spot(Some(0), Some(9)) };
    let t = render(&dragging(view(&names), Drag::Sidebar(SidebarRow::Project(0), Some(landing))));
    let below = project_rows(list(), 1, &plain(20), 0).more_below();
    assert_eq!(row_text(&t, below).trim_end(), format!("    {}", "─".repeat(usize::from(list().width) - 5)));
}

fn dragging(view: View<'static>, drag: Drag) -> View<'static> {
    View { drag: Some(drag), ..view }
}

#[test]
fn renders_the_landing_line_between_projects() {
    const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 22 };
    let (rows, list) = (grouped().sidebar_rows(), layout(TALL, Widths::default()).list);
    let landing =
        sidebar_drop(&project_rows(list, 1, &rows, 0), &rows, SidebarRow::Project(0), Position::new(3, list.y + 3));
    let v = dragging(grouped(), Drag::Sidebar(SidebarRow::Project(0), landing));
    insta::assert_snapshot!(render_sized(&v, TALL.width, TALL.height).backend());
}

#[test]
fn the_landing_line_is_cyan() {
    let landing = Some(Landing { at: 3, spot: spot(Some(0), Some(1)) });
    let t = render(&dragging(grouped(), Drag::Sidebar(SidebarRow::Project(0), landing)));
    let cell = &t.backend().buffer()[(list().x + 5, row(3))];
    assert_eq!((cell.symbol(), cell.fg), ("─", Color::Cyan));
}
