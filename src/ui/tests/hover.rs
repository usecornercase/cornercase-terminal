use super::*;

fn close_pos() -> Position {
    Position::new(close_x(), list().y)
}

fn new_pos() -> Position {
    Position::new(3, new_project_button(list(), 1, &plain(1)).y)
}

#[test]
fn close_button_is_hidden_by_default() {
    assert_eq!(render(&view(&["~"])).backend().buffer()[close_pos()].symbol(), " ");
}

#[test]
fn close_button_shows_dim_while_the_row_is_hovered() {
    let v = View { hover: Some(Position::new(list().x + 3, list().y)), ..view(&["~"]) };
    let cell = render(&v).backend().buffer()[close_pos()].clone();
    assert_eq!((cell.symbol(), cell.fg), ("×", Color::DarkGray));
}

#[test]
fn the_active_entry_has_a_background() {
    let pos = Position::new(list().x + 3, list().y);
    assert_eq!(render(&view(&["~", "tmp"])).backend().buffer()[pos].bg, DARK_SURFACE);
}

#[test]
fn the_surface_is_light_on_a_light_theme() {
    let pos = Position::new(list().x + 3, list().y);
    assert_eq!(render(&View { light: true, ..view(&["~"]) }).backend().buffer()[pos].bg, LIGHT_SURFACE);
}

#[test]
fn the_menu_button_shows_beside_the_close_button_while_the_row_is_hovered() {
    let menu = row_menu_button(entry_row(list(), 1, &plain(1), 0, SidebarRow::Project(0)), 1);
    let at = Position::new(menu.right() - 1, menu.y);
    let hidden = render(&view(&["~"])).backend().buffer()[at].symbol().to_string();
    let v = View { hover: Some(Position::new(list().x + 3, list().y)), ..view(&["~"]) };
    let cell = render(&v).backend().buffer()[at].clone();
    assert_eq!((hidden.as_str(), cell.symbol(), cell.fg), (" ", ROW_MENU_ICON, Color::DarkGray));
}

#[test]
fn the_menu_button_turns_cyan_on_hover() {
    let menu = row_menu_button(entry_row(list(), 1, &plain(1), 0, SidebarRow::Project(0)), 1);
    let v = View { hover: Some(menu.as_position()), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[Position::new(menu.right() - 1, menu.y)].fg, Color::Cyan);
}

#[test]
fn close_button_turns_red_on_hover() {
    let v = View { hover: Some(close_pos()), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[close_pos()].fg, Color::Red);
}

#[test]
fn new_button_has_no_background_by_default() {
    assert_eq!(render(&view(&["~"])).backend().buffer()[new_pos()].bg, Color::Reset);
}

fn more_pos() -> Position {
    areas().more.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 })
}

#[test]
fn more_button_is_dim_by_default() {
    assert_eq!(render(&view(&["~"])).backend().buffer()[more_pos()].bg, Color::Reset);
}

#[test]
fn more_button_is_filled_on_hover() {
    let v = View { hover: Some(more_pos()), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[more_pos()].bg, Color::Cyan);
}

#[test]
fn new_button_is_filled_on_hover() {
    let v = View { hover: Some(new_pos()), ..view(&["~"]) };
    assert_eq!(render(&v).backend().buffer()[new_pos()].bg, Color::Cyan);
}
