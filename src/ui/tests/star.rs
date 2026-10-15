use super::*;
use crate::ui::{StarButton, star_area, star_buttons, star_hit};

fn card() -> View<'static> {
    View { star: true, ..view(&["~"]) }
}

#[test]
fn the_card_sits_at_the_bottom_right() {
    insta::assert_snapshot!(render(&card()).backend());
}

#[test]
fn every_button_is_found_by_its_rect() {
    for (button, rect) in star_buttons(AREA) {
        assert_eq!(star_hit(AREA, rect.as_position()), Some(button));
    }
    let area = star_area(AREA);
    assert_eq!(star_hit(AREA, Position::new(area.x + 2, area.y + 1)), None);
}

#[test]
fn the_star_button_is_the_rightmost() {
    let buttons = star_buttons(AREA);
    assert_eq!(buttons[2].0, StarButton::Star);
    assert!(buttons[0].1.x < buttons[1].1.x && buttons[1].1.x < buttons[2].1.x);
}

#[test]
fn hovering_a_button_lights_it() {
    let (_, rect) = star_buttons(AREA)[2];
    let plain = render(&card());
    let lit = render(&View { hover: Some(rect.as_position()), ..card() });
    let bg = |t: &Terminal<TestBackend>| t.backend().buffer()[rect.as_position()].bg;
    assert_ne!(bg(&plain), bg(&lit));
}
