use super::*;

#[test]
fn the_projects_border_is_the_last_column_of_the_sidebar() {
    assert_eq!(areas().projects_border, Rect::new(SIDEBAR_WIDTH - 1, HEADER_HEIGHT, 1, H - HEADER_HEIGHT));
}

#[test]
fn the_workspaces_border_is_the_last_column_of_the_workspaces_column() {
    let x = SIDEBAR_WIDTH + WORKSPACES_WIDTH - 1;
    assert_eq!(areas().workspaces_border, Rect::new(x, 0, 1, H));
}

#[test]
fn they_follow_the_widths() {
    let widths = Widths { projects: 40, workspaces: 20, ..Widths::default() };
    let areas = layout(AREA, widths);
    assert_eq!((areas.projects_border.x, areas.workspaces_border.x, areas.pane.x), (39, 59, 61));
}

#[rstest]
#[case::projects(Position::new(SIDEBAR_WIDTH - 1, HEADER_HEIGHT + 2), Some(Border::Projects))]
#[case::workspaces(Position::new(SIDEBAR_WIDTH + WORKSPACES_WIDTH - 1, 0), Some(Border::Workspaces))]
#[case::header_has_no_projects_border(Position::new(SIDEBAR_WIDTH - 1, 0), None)]
#[case::inside_the_list(Position::new(3, HEADER_HEIGHT + 2), None)]
fn hit_finds_the_border_under_the_mouse(#[case] pos: Position, #[case] expected: Option<Border>) {
    assert_eq!(areas().border_hit(pos), expected);
}

fn middle(border: Border) -> Position {
    let r = areas().border(border);
    Position::new(r.x, r.y + r.height / 2)
}

#[test]
fn a_border_is_dim_by_default() {
    let pos = middle(Border::Projects);
    assert_eq!(render(&view(&["~"])).backend().buffer()[pos].fg, DARK_LINE);
}

#[test]
fn a_border_turns_cyan_on_hover() {
    let pos = middle(Border::Workspaces);
    let v = View { hover: Some(pos), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[pos].fg, Color::Cyan);
}

#[test]
fn a_border_stays_cyan_while_it_is_dragged() {
    let pos = middle(Border::Projects);
    let v = View { resizing: Some(Border::Projects), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[pos].fg, Color::Cyan);
}

#[test]
fn a_border_ignores_the_hover_while_an_overlay_is_open() {
    let pos = middle(Border::Workspaces);
    let menu = Overlay::Menu { at: Position::new(80, 1), items: vec!["rename tab".into()] };
    let v = View { hover: Some(pos), overlay: Some(menu), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[pos].fg, DARK_LINE);
}
