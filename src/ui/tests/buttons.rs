use super::*;

#[test]
fn new_button_sticks_to_bottom_when_entries_overflow() {
    assert_eq!(new_project_button(list(), 1, &plain(100)).y, list().bottom() - 1);
}

#[test]
fn new_button_wins_over_entries_when_overflowing() {
    let pos = Position::new(3, list().bottom() - 1);
    assert_eq!(sidebar_hit(list(), 1, &plain(100), 0, pos), Some(SidebarHit::New));
}

#[test]
fn close_button_out_of_view_is_empty() {
    assert!(close_button(list(), 1, &plain(50), 0, SidebarRow::Project(49)).is_empty());
}

fn more_label(v: &View) -> ratatui::buffer::Cell {
    let more = areas().more;
    render(v).backend().buffer()[(more.x + 2, more.y)].clone()
}

#[test]
fn an_update_turns_more_cyan() {
    let muted = view(&["~"]).muted;
    let idle = more_label(&view(&["~"]));
    let update = more_label(&View { update: Some("↑ 9.0.0".into()), ..view(&["~"]) });
    assert_eq!((idle.fg, update.fg, update.modifier.contains(Modifier::BOLD)), (muted, Color::Cyan, true));
}
