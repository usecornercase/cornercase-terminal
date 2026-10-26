use super::*;

const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

fn project(name: &str, workspaces: usize, group: Option<usize>, status: Option<Status>) -> ProjectEntry {
    ProjectEntry { name: name.into(), workspaces, group, status }
}

fn tree_shape() -> TreeShape {
    TreeShape {
        groups: vec![false],
        tab_bar: false,
        projects: vec![
            ProjectShape { group: None, collapsed: true, workspaces: Vec::new() },
            ProjectShape {
                group: Some(0),
                collapsed: false,
                workspaces: vec![
                    WorkspaceShape { collapsed: false, tabs: vec![2, 1] },
                    WorkspaceShape { collapsed: true, tabs: Vec::new() },
                ],
            },
            ProjectShape { group: Some(0), collapsed: true, workspaces: Vec::new() },
        ],
    }
}

fn tree_workspaces() -> Vec<Vec<WorkspaceEntry>> {
    let details = Details {
        model: Some("Opus 5.5".into()),
        percent: Some(23),
        subagents: 0,
        memory: None,
        account: None,
        compacted: false,
    };
    let claude = TabEntry { status: Some(Status::Working), details, ..TabEntry::from("claude") };
    let finished = TabEntry { status: Some(Status::Done), ..TabEntry::from("") };
    let workspace = |name: &str, tabs: Vec<TabEntry>, behind: u32| WorkspaceEntry {
        collapsed: false,
        name: name.into(),
        tabs,
        behind,
        pull: None,
        removing: false,
        setting_up: false,
        closed: 0,
    };
    vec![
        Vec::new(),
        vec![workspace("main", vec![claude, "zsh".into()], 0), workspace("issue-50-reorder", vec![finished], 2)],
        Vec::new(),
    ]
}

fn with_tree() -> View<'static> {
    View {
        has_project: true,
        issues: true,
        sidebar: Sidebar::Tree,
        active: 1,
        active_tab: Some(0),
        groups: vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }],
        projects: vec![
            project("notes", 1, None, None),
            project("cornercase", 2, Some(0), Some(Status::Done)),
            project("api", 1, Some(0), Some(Status::Waiting)),
        ],
        tree: Some(TreeView { shape: tree_shape(), workspaces: tree_workspaces() }),
        ..view(&[])
    }
}

#[test]
fn grey_text_takes_the_muted_colour() {
    let v = View { muted: Color::Indexed(243), ..with_tree() };
    let t = render_sized(&v, W, TALL.height);
    let a = layout_with(TALL, v.widths, false, Sidebar::Tree);
    let arrow = tree_arrow(a.list, &v.tree.as_ref().expect("a tree").shape, 0, TreeRow::Project(1));
    let first = |r: Rect| {
        let buf = t.backend().buffer();
        (r.x..r.right()).map(|x| &buf[(x, r.y)]).find(|c| !c.symbol().trim().is_empty()).map(|c| c.fg)
    };

    let greys = [a.title, middle(a.search), arrow, a.more].map(first);

    assert_eq!(greys, [Some(Color::Indexed(243)); 4]);
}

fn row_of(v: &View, row: TreeRow) -> Rect {
    let a = layout_with(TALL, v.widths, false, Sidebar::Tree);
    tree_row(a.list, &v.tree.as_ref().expect("a tree").shape, 0, row)
}

fn line(v: &View, row: TreeRow) -> String {
    row_text(&render_sized(v, W, TALL.height), row_of(v, row)).trim_end().to_string()
}

#[test]
fn renders_groups_projects_workspaces_and_tabs() {
    insta::assert_snapshot!(render_sized(&with_tree(), W, TALL.height).backend());
}

#[test]
fn a_narrow_column_cuts_the_names() {
    let v = View { widths: Widths { projects: MIN_COLUMN_WIDTH, ..Widths::default() }, ..with_tree() };
    insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
}

#[test]
fn the_active_tab_holds_the_mark() {
    assert_eq!(line(&with_tree(), TreeRow::Tab(1, 0, 0)), "▌     ├ ◐ claude");
}

#[test]
fn a_folded_project_holds_the_mark_of_its_active_tab() {
    let mut v = with_tree();
    v.tree.as_mut().expect("a tree").shape.projects[1].collapsed = true;
    let text = line(&v, TreeRow::Project(1));
    assert!(text.starts_with("▌   ▸ cornercase (2)"), "{text}");
}

#[rstest]
#[case::a_folded_project(TreeRow::Project(2), "!")]
#[case::a_folded_workspace(TreeRow::Workspace(1, 1), "✓ ↓2")]
#[case::an_open_project(TreeRow::Project(1), "cornercase")]
#[case::an_open_workspace(TreeRow::Workspace(1, 0), "main")]
fn only_folded_rows_show_what_needs_you(#[case] row: TreeRow, #[case] end: &str) {
    let text = line(&with_tree(), row);
    assert!(text.ends_with(end), "{text}");
}

#[test]
fn a_folded_workspace_shows_agents_that_only_work() {
    let mut v = with_tree();
    v.tree.as_mut().expect("a tree").workspaces[1][1].tabs =
        vec![TabEntry { status: Some(Status::Working), .."".into() }];
    let text = line(&v, TreeRow::Workspace(1, 1));
    assert!(text.contains('◐'), "{text}");
}

#[rstest]
#[case::a_tab(TreeRow::Tab(1, 0, 1), Landing { at: 5, spot: Spot::Tab(0) }, 8)]
#[case::a_workspace(TreeRow::Workspace(1, 1), Landing { at: 4, spot: Spot::Workspace(0) }, 6)]
#[case::a_project_into_a_group(TreeRow::Project(0), Landing { at: 3, spot: Spot::Project { group: Some(0), before: Some(1) } }, 4)]
fn the_landing_line_starts_where_the_row_would(#[case] row: TreeRow, #[case] landing: Landing, #[case] indent: usize) {
    let v = View { drag: Some(Drag::Tree(row, Some(landing))), ..with_tree() };
    let t = render_sized(&v, W, TALL.height);
    let list = layout_with(TALL, v.widths, false, Sidebar::Tree).list;
    let line = (list.y..list.bottom()).map(|y| row_text(&t, Rect { y, height: 1, ..list })).find(|l| l.contains('─'));
    assert_eq!(line.map(|l| l.find('─').map(|b| l[..b].chars().count())), Some(Some(indent)));
}

#[test]
fn a_hovered_row_shows_its_close_button() {
    let v = with_tree();
    let r = row_of(&v, TreeRow::Tab(1, 0, 1));
    let t = render_sized(&View { hover: Some(Position::new(r.x + 8, r.y)), ..v }, W, TALL.height);
    assert_eq!(t.backend().buffer()[(r.right() - 2, r.y)].symbol(), "×");
}
