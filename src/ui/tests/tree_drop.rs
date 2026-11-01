use super::*;

const LIST: Rect = Rect { x: 0, y: 4, width: SIDEBAR_WIDTH - 1, height: 24 };

fn shape() -> TreeShape {
    let workspace = |tabs: usize| WorkspaceShape { collapsed: false, machine: false, tabs: vec![1; tabs] };
    TreeShape {
        groups: vec![false],
        tab_bar: false,
        projects: vec![
            ProjectShape { group: None, collapsed: false, machine: false, workspaces: vec![workspace(1)] },
            ProjectShape {
                group: Some(0),
                collapsed: false,
                machine: false,
                workspaces: vec![workspace(2), workspace(1)],
            },
            ProjectShape { group: Some(0), collapsed: true, machine: false, workspaces: vec![workspace(1)] },
        ],
    }
}

fn drop_on(dragged: TreeRow, under: TreeRow) -> Option<Landing> {
    let r = tree_row(LIST, &shape(), 0, under);
    tree_drop(LIST, &shape(), 0, dragged, Position::new(r.x + 8, r.y))
}

fn project(group: Option<usize>, before: Option<usize>) -> Spot {
    Spot::Project { group, before }
}

#[rstest]
#[case::onto_a_tab_of_a_project_above(TreeRow::Project(2), TreeRow::Tab(1, 0, 0), Some((7, project(Some(0), Some(1)))))]
#[case::after_the_whole_project_below(TreeRow::Project(0), TreeRow::Tab(1, 1, 0), Some((16, project(Some(0), Some(2)))))]
#[case::to_the_end_of_the_group(TreeRow::Project(1), TreeRow::Project(2), Some((17, project(Some(0), None))))]
#[case::out_of_its_group(TreeRow::Project(1), TreeRow::Tab(0, 0, 0), Some((0, project(None, Some(0)))))]
#[case::into_a_group_from_its_header(TreeRow::Project(0), TreeRow::Group(0), Some((7, project(Some(0), Some(1)))))]
#[case::not_into_itself(TreeRow::Project(1), TreeRow::Tab(1, 0, 0), None)]
fn a_project_lands_beside_whole_projects(
    #[case] dragged: TreeRow,
    #[case] under: TreeRow,
    #[case] expected: Option<(usize, Spot)>,
) {
    assert_eq!(drop_on(dragged, under), expected.map(|(at, spot)| Landing { at, spot }));
}

#[rstest]
#[case::before_a_workspace_above(TreeRow::Workspace(1, 1), TreeRow::Tab(1, 0, 1), Some((8, Spot::Workspace(0))))]
#[case::after_a_workspace_below(TreeRow::Workspace(1, 0), TreeRow::Tab(1, 1, 0), Some((15, Spot::Workspace(2))))]
#[case::kept_in_its_project_above(TreeRow::Workspace(1, 1), TreeRow::Tab(0, 0, 0), Some((8, Spot::Workspace(0))))]
#[case::kept_in_its_project_below(TreeRow::Workspace(1, 0), TreeRow::Project(2), Some((15, Spot::Workspace(2))))]
fn a_workspace_stays_in_its_project(
    #[case] dragged: TreeRow,
    #[case] under: TreeRow,
    #[case] expected: Option<(usize, Spot)>,
) {
    assert_eq!(drop_on(dragged, under), expected.map(|(at, spot)| Landing { at, spot }));
}

#[rstest]
#[case::down_one(TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 0, 1), Some((11, Spot::Tab(2))))]
#[case::onto_its_workspace(TreeRow::Tab(1, 0, 1), TreeRow::Workspace(1, 0), Some((9, Spot::Tab(0))))]
#[case::kept_in_its_workspace(TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 1, 0), Some((11, Spot::Tab(2))))]
fn a_tab_stays_in_its_workspace(
    #[case] dragged: TreeRow,
    #[case] under: TreeRow,
    #[case] expected: Option<(usize, Spot)>,
) {
    assert_eq!(drop_on(dragged, under), expected.map(|(at, spot)| Landing { at, spot }));
}

#[test]
fn a_project_dropped_above_the_list_lands_before_the_project_after_its_own_rows() {
    let short = Rect { height: 8, ..LIST };
    let landing = tree_drop(short, &shape(), 7, TreeRow::Project(1), Position::new(8, short.y - 1));
    assert_eq!(landing, Some(Landing { at: 7, spot: project(Some(0), Some(2)) }));
}

#[test]
fn a_group_moves_with_everything_in_it() {
    assert_eq!(drop_on(TreeRow::Group(0), TreeRow::Tab(0, 0, 0)), Some(Landing { at: 6, spot: Spot::Group(0) }));
}
