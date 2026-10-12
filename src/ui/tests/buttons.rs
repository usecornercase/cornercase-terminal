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

fn with_update(hover: Option<Position>) -> View<'static> {
    View { update: Some("↑ 9.0.0".into()), hover, ..view(&["~"]) }
}

fn settings_row(v: &View) -> String {
    let t = render(v);
    let r = areas().settings;
    (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

#[test]
fn an_update_sits_at_the_end_of_the_settings_row() {
    let row = settings_row(&with_update(None));
    assert!(row.starts_with("  Settings ") && row.ends_with(" ↑ 9.0.0 "), "{row:?}");
}

#[test]
fn hovering_the_update_leaves_settings_alone() {
    let r = update_button(areas().settings, "↑ 9.0.0");
    let t = render(&with_update(Some(r.as_position())));
    let buffer = t.backend().buffer();
    let settings = areas().settings.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 });
    assert_eq!((buffer[r.as_position()].bg, buffer[settings].bg), (Color::Cyan, Color::Reset));
}
