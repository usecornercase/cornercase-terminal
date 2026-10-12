use super::*;

fn wlist() -> Rect {
    areas().workspaces_list
}

fn at(row: u16) -> Position {
    Position::new(wlist().x + 4, wlist().y + row)
}

fn close_at(row: u16) -> Position {
    Position::new(wlist().right() - 2, wlist().y + row)
}

fn with_workspaces(active_tab: Option<usize>) -> View<'static> {
    View {
        has_project: true,
        workspaces: vec![
            WorkspaceEntry {
                name: "login".into(),
                tabs: vec!["claude".into(), "nvim".into()],
                behind: 0,
                removing: false,
            },
            WorkspaceEntry { name: "main".into(), tabs: vec!["zsh".into()], behind: 0, removing: false },
        ],
        active_tab,
        ..view(&["cornercase"])
    }
}

fn row_text(v: &View, row: WorkspaceRow) -> String {
    let r = workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), v.workspaces_scroll, row);
    let t = render(v);
    (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

#[rstest]
#[case::workspace(WorkspaceRow::Workspace(0))]
#[case::tab(WorkspaceRow::Tab(0, 0))]
fn long_names_keep_their_start(#[case] row: WorkspaceRow) {
    let long = "feature-with-a-very-long-name-that-does-not-fit";
    let mut v = with_workspaces(Some(0));
    v.workspaces[0].name = long.into();
    v.workspaces[0].tabs[0] = long.into();
    let text = row_text(&v, row);
    assert!(text.contains("feature-with") && text.contains('…'), "{text:?}");
}

#[test]
fn rows_are_each_workspace_then_its_tabs_then_plus_tab() {
    use WorkspaceRow::{Gap, NewTab, Tab, Workspace};
    assert_eq!(
        workspace_rows(&tabs(&[2, 0])),
        [Workspace(0), Tab(0, 0), Tab(0, 1), NewTab(0), Gap, Workspace(1), NewTab(1)]
    );
}

#[rstest]
#[case::workspace(at(0), Some(WorkspaceHit::Workspace(0)))]
#[case::workspace_close(close_at(0), Some(WorkspaceHit::CloseWorkspace(0)))]
#[case::tab(at(1), Some(WorkspaceHit::Tab(0, 0)))]
#[case::tab_close(close_at(2), Some(WorkspaceHit::CloseTab(0, 1)))]
#[case::plus_tab(at(3), Some(WorkspaceHit::NewTab(0)))]
#[case::gap_before_new_workspace(at(4), None)]
#[case::new_workspace(at(5), Some(WorkspaceHit::NewWorkspace))]
fn maps_clicks(#[case] pos: Position, #[case] expected: Option<WorkspaceHit>) {
    assert_eq!(workspace_hit(wlist(), 1, &tabs(&[2]), 0, pos), expected);
}

#[test]
fn the_gap_between_workspaces_is_not_a_row() {
    assert_eq!(workspace_hit(wlist(), 1, &tabs(&[0, 0]), 0, at(2)), None);
}

#[test]
fn new_workspace_sticks_to_the_bottom_when_rows_overflow() {
    assert_eq!(new_workspace_button(wlist(), 1, &tabs(&[100])).y, wlist().bottom() - 1);
}

#[test]
fn rows_under_the_new_workspace_button_are_hidden() {
    let last_visible = usize::from(wlist().height) - 1;
    assert!(workspace_row(wlist(), 1, &tabs(&[100]), 0, WorkspaceRow::Tab(0, last_visible)).is_empty());
}

#[test]
fn renders_the_workspaces_of_the_active_project() {
    insta::assert_snapshot!(render(&with_workspaces(Some(0))).backend());
}

#[test]
fn renders_commits_to_pull_before_the_close_button() {
    let mut v = View { hover: Some(at(0)), ..with_workspaces(Some(0)) };
    v.workspaces[0].behind = 3;
    v.workspaces[1].behind = 1;
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn renders_a_workspace_being_removed_without_its_buttons() {
    let mut v = View { hover: Some(at(0)), active_workspace: 1, ..with_workspaces(Some(0)) };
    v.workspaces[0] = WorkspaceEntry { behind: 3, removing: true, tabs: Vec::new(), name: "login".into() };
    insta::assert_snapshot!(render(&v).backend());
}

#[rstest]
#[case::nothing_to_pull("login", 0, "  login")]
#[case::some("login", 3, "  login          ↓3")]
#[case::long_name_is_cut_first("feature-with-a-very-long-name", 3, "  feature-with-… ↓3")]
fn commits_to_pull_sit_at_the_end_of_the_row(#[case] name: &str, #[case] behind: u32, #[case] expected: &str) {
    let mut v = with_workspaces(Some(0));
    v.workspaces[0] = WorkspaceEntry { name: name.into(), behind, ..v.workspaces.remove(0) };
    assert_eq!(row_text(&v, WorkspaceRow::Workspace(0)).trim_end(), expected);
}

#[test]
fn marks_the_active_tab() {
    let pos = Position::new(wlist().x, wlist().y + 1);
    assert_eq!(render(&with_workspaces(Some(0))).backend().buffer()[pos].symbol(), "▌");
}

#[test]
fn plus_tab_is_filled_on_hover() {
    let v = View { hover: Some(at(3)), ..with_workspaces(Some(0)) };
    assert_eq!(render(&v).backend().buffer()[at(3)].bg, Color::Cyan);
}

#[test]
fn an_empty_workspace_says_so_in_the_pane() {
    let v = View {
        workspaces: vec![WorkspaceEntry { name: "main".into(), tabs: Vec::new(), behind: 0, removing: false }],
        ..with_workspaces(None)
    };
    let text: String =
        render(&v).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(text.contains("no tab open"), "{text}");
}

#[test]
fn nothing_shows_without_a_project() {
    let text: String =
        render(&view(&[])).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(!text.contains("New workspace"), "{text}");
}
