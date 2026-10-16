use super::*;

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

fn stacked(sidebar: Sidebar) -> Areas {
    layout_with(TALL, Widths::default(), false, sidebar)
}

#[test]
fn the_pane_gets_the_width_of_the_workspaces_column() {
    let side = layout(TALL, Widths::default());
    assert_eq!(stacked(Sidebar::ProjectsOnTop).pane.width, side.pane.width + WORKSPACES_WIDTH);
}

#[rstest]
#[case::projects_on_top(Sidebar::ProjectsOnTop, true)]
#[case::workspaces_on_top(Sidebar::WorkspacesOnTop, false)]
fn the_setting_picks_the_list_on_top(#[case] sidebar: Sidebar, #[case] projects_first: bool) {
    let a = stacked(sidebar);
    assert_eq!(a.title.y < a.workspaces_title.y, projects_first);
}

#[test]
fn the_line_sits_between_the_lists() {
    let a = stacked(Sidebar::ProjectsOnTop);
    assert_eq!(
        (a.list.bottom(), a.stack_border.y, a.workspaces_title.y),
        (HEADER_HEIGHT + 10, HEADER_HEIGHT + 10, HEADER_HEIGHT + 11)
    );
}

#[test]
fn one_footer_holds_issues_todo_settings_usage_and_quit() {
    let a = stacked(Sidebar::WorkspacesOnTop);
    assert_eq!(
        (a.separator.y, a.issues.y, a.todo_button.y, a.settings.y, a.usage.y, a.quit.y, a.workspaces_separator),
        (24, 25, 26, 27, 28, 29, Rect::default())
    );
}

#[test]
fn the_column_border_runs_the_whole_height() {
    let a = stacked(Sidebar::ProjectsOnTop);
    assert_eq!((a.projects_border, a.workspaces_border), (Rect::new(SIDEBAR_WIDTH - 1, 0, 1, 30), Rect::default()));
}

#[rstest]
#[case::line(Position::new(3, HEADER_HEIGHT + 10), Some(Border::Stack))]
#[case::column(Position::new(SIDEBAR_WIDTH - 1, 2), Some(Border::Projects))]
#[case::list(Position::new(3, HEADER_HEIGHT + 4), None)]
fn hit_finds_the_line_and_the_column_border(#[case] pos: Position, #[case] expected: Option<Border>) {
    assert_eq!(stacked(Sidebar::ProjectsOnTop).border_hit(pos), expected);
}

#[test]
fn each_list_scrolls_in_its_own_section() {
    let a = stacked(Sidebar::ProjectsOnTop);
    let (projects, workspaces) = (a.list.as_position(), a.workspaces_list.as_position());
    assert_eq!(
        (a.sidebar.contains(projects), a.sidebar.contains(workspaces), a.workspaces.contains(workspaces)),
        (true, false, true)
    );
}

#[test]
fn narrow_terminals_stay_compact() {
    let a = layout_with(Rect { width: 80, ..TALL }, Widths::default(), false, Sidebar::ProjectsOnTop);
    assert!(a.compact());
}

#[test]
fn the_changes_panel_sits_right_of_the_pane() {
    let a = layout_with(Rect { width: 160, ..TALL }, Widths::default(), true, Sidebar::ProjectsOnTop);
    assert_eq!((a.pane.x, a.changes_border.x), (SIDEBAR_WIDTH + PANE_PADDING, a.pane.right()));
}

fn with_lists(sidebar: Sidebar) -> View<'static> {
    let mut v = View {
        has_project: true,
        issues: true,
        active_tab: Some(0),
        sidebar,
        workspaces: vec![
            WorkspaceEntry {
                collapsed: false,
                name: "login".into(),
                tabs: vec!["claude".into(), "nvim".into()],
                behind: 0,
                pull: None,
                removing: false,
                setting_up: false,
            },
            WorkspaceEntry {
                collapsed: false,
                name: "main".into(),
                tabs: vec!["zsh".into()],
                behind: 0,
                pull: None,
                removing: false,
                setting_up: false,
            },
        ],
        ..view(&["tmp", "api", "cornercase"])
    };
    v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }];
    v.projects[1].group = Some(0);
    v
}

#[test]
fn renders_projects_on_top() {
    insta::assert_snapshot!(render_sized(&with_lists(Sidebar::ProjectsOnTop), W, TALL.height).backend());
}

#[test]
fn renders_workspaces_on_top() {
    insta::assert_snapshot!(render_sized(&with_lists(Sidebar::WorkspacesOnTop), W, TALL.height).backend());
}

#[test]
fn the_column_border_runs_down_the_header_too() {
    let t = render_sized(&with_lists(Sidebar::ProjectsOnTop), W, TALL.height);
    assert_eq!(t.backend().buffer()[(SIDEBAR_WIDTH - 1, 1)].symbol(), "│");
}

#[test]
fn the_hovered_line_turns_cyan() {
    let line = stacked(Sidebar::ProjectsOnTop).stack_border;
    let at = Position::new(line.x + 3, line.y);
    let t = render_sized(&View { hover: Some(at), ..with_lists(Sidebar::ProjectsOnTop) }, W, TALL.height);
    assert_eq!(t.backend().buffer()[(at.x, at.y)].fg, Color::Cyan);
}

#[rstest]
#[case::projects_on_top_10_rows(Sidebar::ProjectsOnTop, 10)]
#[case::projects_on_top_11_rows(Sidebar::ProjectsOnTop, 11)]
#[case::workspaces_on_top_10_rows(Sidebar::WorkspacesOnTop, 10)]
#[case::workspaces_on_top_11_rows(Sidebar::WorkspacesOnTop, 11)]
fn a_short_terminal_still_shows_the_footer(#[case] sidebar: Sidebar, #[case] height: u16) {
    let a = layout_with(Rect { height, ..TALL }, Widths::default(), false, sidebar);
    let t = render_sized(&with_lists(sidebar), W, height);
    let text = |r: Rect| row_text(&t, r).trim().to_string();
    assert_eq!(
        [text(a.issues), text(a.settings), text(a.usage), text(a.quit)],
        ["Issues", "Settings", "Usage", "Quit"]
    );
}

#[test]
fn short_terminals_do_not_panic() {
    for height in 1..=TALL.height {
        render_sized(&with_lists(Sidebar::WorkspacesOnTop), W, height);
    }
}
