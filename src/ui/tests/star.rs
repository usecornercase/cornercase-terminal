use super::*;
use crate::ui::{StarButton, star_area, star_buttons, star_hit};

fn dialog() -> View<'static> {
    View { overlay: Some(Overlay::Star), ..view(&["~"]) }
}

#[test]
fn its_border_is_drawn_at_its_area() {
    assert_eq!(corners_of_its_area(&dialog()), ("╭".to_string(), "╯".to_string()));
}

#[test]
fn it_sits_in_the_middle() {
    let r = star_area(AREA);
    assert_eq!(r.x, (AREA.width - r.width) / 2);
    assert_eq!(r.y, (AREA.height - r.height) / 2);
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
fn star_on_github_comes_before_no_thanks() {
    let buttons = star_buttons(AREA);
    assert_eq!((buttons[0].0, buttons[1].0), (StarButton::Star, StarButton::NoThanks));
    assert!(buttons[0].1.x < buttons[1].1.x);
}

#[test]
fn hovering_a_button_lights_it() {
    let (_, rect) = star_buttons(AREA)[0];
    let plain = render(&dialog());
    let lit = render(&View { hover: Some(rect.as_position()), ..dialog() });
    let bg = |t: &Terminal<TestBackend>| t.backend().buffer()[rect.as_position()].bg;
    assert_ne!(bg(&plain), bg(&lit));
}

#[test]
fn in_a_narrow_window_the_message_wraps_and_the_buttons_stay_inside() {
    let screen = Rect::new(0, 0, 50, 18);
    let area = star_area(screen);
    assert!(area.height > 5);
    for (_, rect) in star_buttons(screen) {
        assert_eq!(rect.intersection(area), rect);
    }
    let t = render_sized(&dialog(), 50, 18);
    assert_eq!(t.backend().buffer()[(area.x, area.y)].symbol(), "╭");
}
