use super::*;

const TALL: Rect = Rect { x: 0, y: 4, width: 40, height: 14 };

#[test]
fn each_entry_takes_the_whole_pitch() {
    assert_eq!(project_rows(TALL, 3, &plain(2), 0).item(1), Rect::new(0, 7, 40, 3));
}

#[rstest]
#[case::top(4)]
#[case::middle(5)]
#[case::bottom(6)]
fn any_row_of_an_entry_selects_it(#[case] y: u16) {
    assert_eq!(sidebar_hit(TALL, 3, &plain(2), 0, Position::new(10, y)), Some(SidebarHit::Select(0)));
}

#[test]
fn the_new_button_is_as_tall_as_an_entry() {
    assert_eq!(new_project_button(TALL, 3, &plain(1)), Rect::new(0, 8, 40, 3));
}

#[test]
fn the_gap_between_workspaces_stays_one_row() {
    let second = workspace_row(TALL, 3, &tabs(&[0, 0]), 0, WorkspaceRow::Workspace(1));
    assert_eq!(second.y, TALL.y + 3 + 3 + GAP);
}

#[test]
fn the_wheel_moves_by_entries() {
    assert_eq!(project_rows(TALL, 3, &plain(10), 0).scrolled(100), 7);
}

#[test]
fn reveal_counts_rows_not_entries() {
    assert_eq!(project_rows(TALL, 3, &plain(10), 0).reveal(5), 3);
}

#[test]
fn a_tall_row_gets_a_wider_close_button() {
    assert_eq!(row_close_button(Rect::new(0, 0, 40, 3), 3).width, COMPACT_CLOSE_WIDTH);
}
