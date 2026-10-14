use super::*;

fn small() -> Areas {
    layout(SMALL, Widths::default())
}

fn render_small(view: &View) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(SMALL.width, SMALL.height)).expect("test backend");
    t.draw(|f| draw(f, view)).expect("draw");
    t
}

fn in_a_project(nav: Option<Nav>) -> View<'static> {
    View {
        has_project: true,
        workspaces: vec![
            WorkspaceEntry {
                name: "feat/login".into(),
                tabs: vec!["claude".into(), "nvim".into()],
                behind: 0,
                pull: None,
                removing: false,
            },
            WorkspaceEntry { name: "main".into(), tabs: vec!["zsh".into()], behind: 0, pull: None, removing: false },
        ],
        active_tab: Some(0),
        nav,
        ..view(&["cornercase", "shop"])
    }
}

#[rstest]
#[case::narrow(COMPACT_WIDTH - 1, true)]
#[case::wide(COMPACT_WIDTH, false)]
fn narrow_terminals_get_the_menu_bar(#[case] width: u16, #[case] compact: bool) {
    assert_eq!(layout(Rect::new(0, 0, width, 20), Widths::default()).compact(), compact);
}

#[test]
fn the_projects_menu_draws_in_a_terminal_two_columns_wide() {
    let t = render_sized(&in_a_project(Some(Nav::Projects)), 2, SMALL.height);
    assert_eq!(t.backend().buffer().area.width, 2);
}

#[test]
fn the_pane_takes_everything_under_the_bar() {
    assert_eq!(small().pane, Rect::new(0, COMPACT_PITCH, SMALL.width, SMALL.height - COMPACT_PITCH));
}

#[test]
fn the_search_button_is_the_end_of_the_bar() {
    assert_eq!(
        small().search_button,
        Rect::new(SMALL.width - COMPACT_BUTTON_WIDTH, 0, COMPACT_BUTTON_WIDTH, COMPACT_PITCH)
    );
}

#[rstest]
#[case::closed(None, false, false)]
#[case::projects(Some(Nav::Projects), true, false)]
#[case::workspaces(Some(Nav::Workspaces), false, true)]
fn the_menu_shows_one_column_at_a_time(#[case] nav: Option<Nav>, #[case] projects: bool, #[case] workspaces: bool) {
    let shown = small().shown(nav);
    assert_eq!((!shown.list.is_empty(), !shown.workspaces_list.is_empty()), (projects, workspaces));
    assert_eq!(!shown.back.is_empty(), workspaces);
}

#[test]
fn wide_terminals_show_both_columns_whatever_the_menu() {
    let areas = layout(AREA, Widths::default());
    assert_eq!(areas.shown(None), areas);
}

#[test]
fn renders_the_bar_over_the_pane() {
    insta::assert_snapshot!(render_small(&in_a_project(None)).backend());
}

#[test]
fn renders_the_workspaces_menu() {
    insta::assert_snapshot!(render_small(&in_a_project(Some(Nav::Workspaces))).backend());
}

#[test]
fn renders_commits_to_pull_in_the_workspaces_menu() {
    let mut v = in_a_project(Some(Nav::Workspaces));
    v.workspaces[0].behind = 12;
    insta::assert_snapshot!(render_small(&v).backend());
}

#[test]
fn renders_the_projects_menu() {
    insta::assert_snapshot!(render_small(&in_a_project(Some(Nav::Projects))).backend());
}

fn with_groups(view: View<'static>) -> View<'static> {
    let mut v = View { active: 1, ..view };
    v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }];
    v.projects[1].group = Some(0);
    v
}

#[test]
fn renders_groups_in_the_projects_menu() {
    insta::assert_snapshot!(render_small(&with_groups(in_a_project(Some(Nav::Projects)))).backend());
}

fn close_cell(view: &View, row: Rect) -> (String, Color) {
    let r = row_close_button(row, COMPACT_PITCH);
    let cell = render_small(view).backend().buffer()[Position::new(r.x + r.width / 2, r.y + 1)].clone();
    (cell.symbol().to_string(), cell.fg)
}

#[rstest]
#[case::workspace(WorkspaceRow::Workspace(0))]
#[case::active_tab(WorkspaceRow::Tab(0, 0))]
#[case::tab(WorkspaceRow::Tab(0, 1))]
fn every_workspace_and_tab_shows_its_close_button_without_hover(#[case] row: WorkspaceRow) {
    let v = in_a_project(Some(Nav::Workspaces));
    let r = workspace_row(small().workspaces_list, COMPACT_PITCH, &v.tab_lines(), 0, row);
    assert_eq!(close_cell(&v, r), ("×".into(), Color::DarkGray));
}

#[rstest]
#[case::active(0)]
#[case::inactive(1)]
fn every_project_shows_its_close_button_without_hover(#[case] p: usize) {
    let v = in_a_project(Some(Nav::Projects));
    let r = entry_row(small().list, COMPACT_PITCH, &plain(2), 0, SidebarRow::Project(p));
    assert_eq!(close_cell(&v, r), ("×".into(), Color::DarkGray));
}

#[test]
fn a_close_button_turns_red_on_hover() {
    let v = in_a_project(Some(Nav::Workspaces));
    let r = workspace_row(small().workspaces_list, COMPACT_PITCH, &v.tab_lines(), 0, WorkspaceRow::Tab(0, 1));
    let close = row_close_button(r, COMPACT_PITCH);
    let hover = Some(Position::new(close.x + 1, close.y + 1));
    assert_eq!(close_cell(&View { hover, ..v }, r).1, Color::Red);
}

#[test]
fn renders_the_landing_line_between_tabs() {
    let v = in_a_project(Some(Nav::Workspaces));
    let (list, tabs) = (small().workspaces_list, v.tab_lines());
    let nvim = workspace_row(list, COMPACT_PITCH, &tabs, 0, WorkspaceRow::Tab(0, 1));
    let dragged = WorkspaceRow::Tab(0, 0);
    let landing = workspace_drop(list, COMPACT_PITCH, &tabs, 0, dragged, nvim.as_position());
    insta::assert_snapshot!(render_small(&View { drag: Some(Drag::Workspaces(dragged, landing)), ..v }).backend());
}

#[test]
fn a_header_is_a_whole_band_to_click() {
    let v = with_groups(in_a_project(Some(Nav::Projects)));
    let rows = v.sidebar_rows();
    let header = entry_row(small().list, COMPACT_PITCH, &rows, 0, SidebarRow::Group(0));
    let bottom = Position::new(header.x + 2, header.bottom() - 1);
    assert_eq!(
        (header.height, sidebar_hit(small().list, COMPACT_PITCH, &rows, 0, bottom)),
        (COMPACT_PITCH, Some(SidebarHit::Group(0)))
    );
}

#[test]
fn the_active_entry_fills_its_whole_band() {
    let t = render_small(&in_a_project(Some(Nav::Projects)));
    let r = entry_row(small().list, COMPACT_PITCH, &plain(2), 0, SidebarRow::Project(0));
    let rows: Vec<Color> = (r.top()..r.bottom()).map(|y| t.backend().buffer()[(r.x + 20, y)].bg).collect();
    assert_eq!(rows, vec![DARK_SURFACE; usize::from(COMPACT_PITCH)]);
}

#[test]
fn the_menu_button_is_a_block_while_the_menu_is_open() {
    let t = render_small(&in_a_project(Some(Nav::Workspaces)));
    let rows: Vec<Color> = (0..COMPACT_PITCH).map(|y| t.backend().buffer()[(1, y)].bg).collect();
    assert_eq!(rows, vec![Color::Cyan; usize::from(COMPACT_PITCH)]);
}

#[test]
fn the_bar_shows_the_brand_without_a_project() {
    let t = render_small(&view(&[]));
    let row: String = (0..SMALL.width).map(|x| t.backend().buffer()[(x, 1)].symbol().to_string()).collect();
    assert!(row.contains("cornercase"), "{row:?}");
}

#[test]
fn the_search_field_takes_the_whole_bar() {
    let search = Search { query: "feat".into(), results: Vec::new(), selected: 0, scroll: 0, hint: String::new() };
    let v = View { overlay: Some(Overlay::Search(search)), ..in_a_project(None) };
    let t = render_small(&v);
    let row: String = (0..SMALL.width).map(|x| t.backend().buffer()[(x, 1)].symbol().to_string()).collect();
    assert!(row.starts_with(" ⌕ feat") && !row.contains("claude"), "{row:?}");
}
