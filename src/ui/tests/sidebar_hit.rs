use super::*;

#[rstest]
#[case::first(3, 0, Some(SidebarHit::Select(0)))]
#[case::second(3, 1, Some(SidebarHit::Select(1)))]
#[case::gap_before_the_new_button(3, 2, None)]
#[case::new_button_after_two_entries(3, 3, Some(SidebarHit::New))]
fn maps_rows_to_entries(#[case] x: u16, #[case] row: u16, #[case] expected: Option<SidebarHit>) {
    assert_eq!(sidebar_hit(list(), 1, &plain(2), 0, Position::new(x, list().y + row)), expected);
}

#[rstest]
#[case::first(0)]
#[case::second(1)]
fn close_button_closes_its_entry(#[case] i: u16) {
    let pos = Position::new(close_x(), list().y + i);
    assert_eq!(sidebar_hit(list(), 1, &plain(2), 0, pos), Some(SidebarHit::Close(usize::from(i))));
}

#[test]
fn a_narrow_row_leaves_the_menu_to_the_right_click() {
    let narrow = Rect::new(0, 0, MIN_MENU_ROW_WIDTH - 1, 1);
    assert_eq!((row_menu_button(narrow, 1).width, row_close_button(narrow, 1).width), (0, CLOSE_BUTTON_WIDTH));
}

#[rstest]
#[case::wide(1)]
#[case::compact(COMPACT_PITCH)]
fn the_menu_button_sits_left_of_the_close_button(#[case] pitch: u16) {
    let row = entry_row(list(), pitch, &plain(2), 0, SidebarRow::Project(1));
    let (menu, close) = (row_menu_button(row, pitch), row_close_button(row, pitch));
    assert_eq!(menu.right(), close.x);
    assert_eq!(sidebar_hit(list(), pitch, &plain(2), 0, menu.as_position()), Some(SidebarHit::Menu(1)));
}

#[rstest]
#[case::search(areas().search.y)]
#[case::title(areas().title.y)]
fn header_rows_are_ignored(#[case] y: u16) {
    assert_eq!(sidebar_hit(list(), 1, &plain(1), 0, Position::new(3, y)), None);
}

#[test]
fn pane_is_ignored() {
    assert_eq!(sidebar_hit(list(), 1, &plain(1), 0, Position::new(SIDEBAR_WIDTH + 5, list().y)), None);
}
