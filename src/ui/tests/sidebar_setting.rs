use super::*;

#[rstest]
#[case::side_by_side("side_by_side", Sidebar::SideBySide)]
#[case::projects_on_top("projects_on_top", Sidebar::ProjectsOnTop)]
#[case::workspaces_on_top("workspaces_on_top", Sidebar::WorkspacesOnTop)]
#[case::tree("tree", Sidebar::Tree)]
#[case::any_case_and_spaces(" Projects_On_Top ", Sidebar::ProjectsOnTop)]
#[case::unknown("sideways", Sidebar::ProjectsOnTop)]
#[case::empty("", Sidebar::ProjectsOnTop)]
fn is_read_from_the_config(#[case] setting: &str, #[case] expected: Sidebar) {
    assert_eq!(Sidebar::from_setting(setting), expected);
}

#[test]
fn only_side_by_side_keeps_two_columns() {
    let stacked: Vec<bool> = Sidebar::ALL.into_iter().map(Sidebar::stacked).collect();
    assert_eq!(stacked, [false, true, true, true]);
}

#[test]
fn each_choice_is_its_id_with_a_note() {
    let ids: Vec<&str> = Sidebar::choices().into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["side_by_side", "projects_on_top", "workspaces_on_top", "tree"]);
}
