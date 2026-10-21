use ratatui::style::Modifier;

use super::*;
use crate::project::split::Dir;

fn three(dim_inactive: bool) -> TabView {
    let mut layout = Node::Leaf(0);
    layout.split(0, Dir::Right, 1);
    layout.split(1, Dir::Down, 2);
    let screens = vec![screen(b"$ left"), screen(b"$ top"), screen(b"$ bottom")];
    TabView {
        layout,
        screens,
        active: 1,
        dim_inactive,
        dragging: None,
        link: None,
        landing: None,
        bands: Vec::new(),
        footer: false,
        accounts: Vec::new(),
    }
}

fn style_at(v: &View, at: Position) -> Style {
    render(v).backend().buffer()[(at.x, at.y)].style()
}

fn first_cell(v: &View, i: usize) -> Position {
    let tab = v.tab.as_ref().expect("a tab");
    tab.layout.panes(areas().pane)[i].1.as_position()
}

#[test]
fn panes_are_drawn_with_dividers_that_join() {
    let v = View { tab: Some(three(true)), ..view(&["~"]) };
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn without_room_for_every_pane_only_the_active_one_is_drawn() {
    let v = View { tab: Some(three(true)), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, W, 6).backend());
}

fn landing(place: Place) -> String {
    let target = three(true).layout.panes(areas().pane)[0].1;
    let tab = TabView { landing: Some((place.area(target), place)), ..three(true) };
    let v = View { tab: Some(tab), ..view(&["~"]) };
    format!("{:?}", render(&v).backend())
}

#[test]
fn a_pane_landing_on_an_edge_tints_that_half() {
    insta::assert_snapshot!(landing(Place::Below));
}

#[test]
fn a_pane_landing_in_the_middle_offers_a_swap() {
    insta::assert_snapshot!(landing(Place::Swap));
}

#[test]
fn the_landing_takes_the_surface_background() {
    let v = View { tab: Some(TabView { landing: Some((areas().pane, Place::Swap)), ..three(true) }), ..view(&["~"]) };
    assert_eq!(style_at(&v, areas().pane.as_position()).bg, Some(v.surface()));
}

#[test]
fn inactive_panes_are_dimmed() {
    let v = View { tab: Some(three(true)), ..view(&["~"]) };
    let at = first_cell(&v, 0);
    assert!(style_at(&v, at).add_modifier.contains(Modifier::DIM));
}

#[test]
fn the_active_pane_is_not_dimmed() {
    let v = View { tab: Some(three(true)), ..view(&["~"]) };
    let at = first_cell(&v, 1);
    assert!(!style_at(&v, at).add_modifier.contains(Modifier::DIM));
}

#[test]
fn dimming_can_be_turned_off() {
    let v = View { tab: Some(three(false)), ..view(&["~"]) };
    let at = first_cell(&v, 0);
    assert!(!style_at(&v, at).add_modifier.contains(Modifier::DIM));
}

#[test]
fn the_cursor_is_in_the_active_pane() {
    let v = View { tab: Some(three(true)), ..view(&["~"]) };
    let top = first_cell(&v, 1);
    assert_eq!(render(&v).get_cursor_position().expect("cursor"), Position::new(top.x + 5, top.y));
}

#[test]
fn a_hovered_divider_turns_cyan() {
    let tab = three(true);
    let line = tab.layout.dividers(areas().pane)[0].line;
    let at = Position::new(line.x, line.y + 1);
    let v = View { tab: Some(tab), hover: Some(at), ..view(&["~"]) };
    assert_eq!(style_at(&v, at).fg, Some(Color::Cyan));
}
