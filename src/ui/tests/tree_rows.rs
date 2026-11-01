use super::*;

const LIST: Rect = Rect { x: 0, y: 4, width: SIDEBAR_WIDTH - 1, height: 20 };

fn shape() -> TreeShape {
    let main = WorkspaceShape { collapsed: false, machine: false, tabs: vec![1, 2] };
    let login = WorkspaceShape { collapsed: true, machine: false, tabs: vec![1] };
    TreeShape {
        groups: vec![false],
        tab_bar: false,
        projects: vec![
            ProjectShape { group: None, collapsed: true, machine: false, workspaces: vec![WorkspaceShape::default()] },
            ProjectShape { group: Some(0), collapsed: false, machine: false, workspaces: vec![main, login] },
        ],
    }
}

#[test]
fn list_groups_projects_workspaces_and_tabs_in_order() {
    assert_eq!(
        tree_rows(&shape()),
        [
            TreeRow::Project(0),
            TreeRow::Gap,
            TreeRow::Group(0),
            TreeRow::Project(1),
            TreeRow::Workspace(1, 0),
            TreeRow::Tab(1, 0, 0),
            TreeRow::Tab(1, 0, 1),
            TreeRow::NewTab(1, 0),
            TreeRow::Workspace(1, 1),
            TreeRow::NewWorkspace(1),
        ]
    );
}

#[test]
fn a_folded_group_hides_its_projects() {
    let mut s = shape();
    s.groups[0] = true;
    assert_eq!(tree_rows(&s), [TreeRow::Project(0), TreeRow::Gap, TreeRow::Group(0)]);
}

#[test]
fn a_tab_with_its_details_takes_two_rows() {
    let rows = [TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 0, 1)].map(|r| tree_row(LIST, &shape(), 0, r).height);
    assert_eq!(rows, [1, 2]);
}

#[rstest]
#[case::the_tab(shape(), (1, 0, Some(1)), TreeRow::Tab(1, 0, 1))]
#[case::a_folded_workspace(shape(), (1, 1, Some(0)), TreeRow::Workspace(1, 1))]
#[case::a_workspace_without_tabs(shape(), (1, 0, None), TreeRow::Workspace(1, 0))]
#[case::a_folded_project(shape(), (0, 0, Some(0)), TreeRow::Project(0))]
#[case::a_folded_group(TreeShape { groups: vec![true], ..shape() }, (1, 0, Some(0)), TreeRow::Group(0))]
fn the_active_mark_goes_on_the_nearest_row_shown(
    #[case] shape: TreeShape,
    #[case] active: (usize, usize, Option<usize>),
    #[case] expected: TreeRow,
) {
    let rows = tree_rows(&shape);
    assert_eq!(tree_active_row(&rows, &shape, active).map(|i| rows[i]), Some(expected));
}

#[test]
fn rows_are_indented_one_step_per_level() {
    let arrows = [TreeRow::Group(0), TreeRow::Project(0), TreeRow::Project(1), TreeRow::Workspace(1, 0)]
        .map(|r| tree_arrow(LIST, &shape(), 0, r).x);
    assert_eq!(arrows, [2, 2, 4, 6]);
}

fn hit(row: TreeRow, x: impl Fn(Rect) -> u16) -> Option<TreeHit> {
    let r = tree_row(LIST, &shape(), 0, row);
    tree_hit(LIST, &shape(), 0, Position::new(x(r), r.y))
}

#[rstest]
#[case::the_arrow_folds(TreeRow::Workspace(1, 0), 6, Some(TreeHit::Fold(TreeRow::Workspace(1, 0))))]
#[case::the_name_selects(TreeRow::Workspace(1, 0), 9, Some(TreeHit::Select(TreeRow::Workspace(1, 0))))]
#[case::a_project_name_selects(TreeRow::Project(1), 7, Some(TreeHit::Select(TreeRow::Project(1))))]
#[case::a_group_folds_anywhere(TreeRow::Group(0), 10, Some(TreeHit::Fold(TreeRow::Group(0))))]
#[case::a_tab_selects(TreeRow::Tab(1, 0, 0), 2, Some(TreeHit::Select(TreeRow::Tab(1, 0, 0))))]
#[case::add_a_tab(TreeRow::NewTab(1, 0), 9, Some(TreeHit::NewTab(1, 0)))]
#[case::add_a_workspace(TreeRow::NewWorkspace(1), 7, Some(TreeHit::NewWorkspace(1)))]
#[case::a_gap_is_nothing(TreeRow::Gap, 4, None)]
fn a_click_lands_on_what_is_under_it(#[case] row: TreeRow, #[case] x: u16, #[case] expected: Option<TreeHit>) {
    assert_eq!(hit(row, |r| r.x + x), expected);
}

#[rstest]
#[case::a_group(TreeRow::Group(0))]
#[case::a_project(TreeRow::Project(1))]
#[case::a_workspace(TreeRow::Workspace(1, 0))]
#[case::a_tab(TreeRow::Tab(1, 0, 1))]
fn the_close_button_closes_the_row(#[case] row: TreeRow) {
    let x = tree_close(LIST, &shape(), 0, row);
    assert_eq!(tree_hit(LIST, &shape(), 0, x.as_position()), Some(TreeHit::Close(row)));
}

#[rstest]
#[case::a_group(TreeRow::Group(0))]
#[case::a_project(TreeRow::Project(1))]
#[case::a_workspace(TreeRow::Workspace(1, 0))]
#[case::a_tab(TreeRow::Tab(1, 0, 1))]
fn the_menu_button_opens_the_row_menu(#[case] row: TreeRow) {
    let menu = row_menu_button(tree_row(LIST, &shape(), 0, row), 1);
    assert_eq!(tree_hit(LIST, &shape(), 0, menu.as_position()), Some(TreeHit::Menu(row)));
}

#[test]
fn the_button_at_the_bottom_adds_a_project() {
    let rows = tree_rows(&shape());
    let button = tree_layout_rows(LIST, &shape(), &rows, 0).button();
    assert_eq!(tree_hit(LIST, &shape(), 0, button.as_position()), Some(TreeHit::NewProject));
}
