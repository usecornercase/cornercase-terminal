use super::*;
use crate::editor::Editor;
use crate::ui::{TourButton, TourCard, help_button, more_button, tour_hit, tour_layout};

const TALL: u16 = 30;

fn card(target: Rect, field: bool) -> TourCard {
    TourCard {
        title: "Get started · 1/7".into(),
        step: "Projects".into(),
        text: "A project is a folder you work in. Open one with + New, then open project.".into(),
        note: None,
        field: field.then(|| Editor::new("Explain this repository's structure.")),
        focused: false,
        agent: field.then(|| "claude".into()),
        actions: if field { vec![TourButton::Idea, TourButton::Start] } else { Vec::new() },
        back: field,
        forward: if field { TourButton::Skip } else { TourButton::Next },
        target,
        button: !field,
    }
}

fn tall() -> Areas {
    layout(Rect::new(0, 0, W, TALL), Widths::default())
}

fn help_menu() -> Overlay {
    let at = help_button(tall().more).as_position();
    let items = ["tour", "catalogue…", "docs", "report a bug"].map(String::from).to_vec();
    Overlay::Menu { at, disabled: vec![false; items.len()], items }
}

#[test]
fn the_card_points_at_help_in_the_wide_layout() {
    let target = help_button(tall().more);
    let v = View { tour: Some(card(target, false)), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, W, TALL).backend());
}

#[test]
fn the_card_with_its_prompt_field() {
    let pane = tall().pane;
    let target = Rect { x: pane.x - 1, width: 1, ..pane };
    let v = View { tour: Some(card(target, true)), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, W, TALL).backend());
}

#[test]
fn the_card_in_the_compact_layout() {
    let target = Rect::new(0, 0, 7, 3);
    let v = View { tour: Some(card(target, false)), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, 60, TALL).backend());
}

#[test]
fn the_help_menu_opens_from_the_footer() {
    let v = View { overlay: Some(help_menu()), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, W, TALL).backend());
}

#[test]
fn help_sits_in_the_icon_bar_over_the_gear() {
    let v = View { icon_bar: true, icons: true, remote: Some(false), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, W, TALL).backend());
}

#[test]
fn help_shares_the_more_row_in_compact_mode() {
    let v = View { nav: Some(Nav::Projects), ..view(&["~"]) };
    insta::assert_snapshot!(render_sized(&v, 60, TALL).backend());
}

#[test]
fn the_card_moves_away_from_what_it_points_at() {
    let area = Rect::new(0, 0, W, TALL);
    let corner = Rect::new(W - 10, TALL - 5, 10, 5);
    let placed = tour_layout(area, &card(corner, false)).area;
    assert!(placed.intersection(corner).is_empty());
}

#[test]
fn every_button_is_found_by_its_rect() {
    let area = Rect::new(0, 0, W, TALL);
    let c = card(Rect::default(), true);
    let layout = tour_layout(area, &c);
    assert!(layout.buttons.len() >= 6);
    for (button, rect) in layout.buttons {
        assert_eq!(tour_hit(area, &c, rect.as_position()), Some(button));
    }
}

#[test]
fn a_narrow_more_row_drops_help_and_keeps_more_whole() {
    let row = Rect::new(0, 0, 15, 1);
    assert!(help_button(row).is_empty());
    assert_eq!(more_button(row), row);
}

#[test]
fn the_focused_prompt_field_shows_its_selection_reversed() {
    let pane = tall().pane;
    let target = Rect { x: pane.x - 1, width: 1, ..pane };
    let reversed = |focused: bool| {
        let mut c = card(target, true);
        c.focused = focused;
        if let Some(field) = c.field.as_mut() {
            field.select_all();
        }
        let v = View { tour: Some(c), ..view(&["~"]) };
        let t = render_sized(&v, W, TALL);
        t.backend().buffer().content().iter().filter(|c| c.modifier.contains(Modifier::REVERSED)).count()
    };
    assert_eq!(reversed(true), reversed(false) + "Explain this repository's structure.".len());
}
