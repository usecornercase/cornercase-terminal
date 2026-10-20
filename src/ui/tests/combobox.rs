use super::*;
use crate::list::{COMBO_ROWS, Combo};
use crate::ui::combo::{Field, draw_field, draw_list, hit, list, rows};

const SCREEN: Rect = Rect { x: 0, y: 0, width: 40, height: 20 };

fn combo(n: usize, chosen: usize) -> Combo {
    let mut combo = Combo::new((0..n).map(|i| format!("option {i}")).collect(), Some(chosen));
    combo.open();
    combo
}

#[test]
fn the_list_opens_below_the_field_and_above_it_near_the_bottom() {
    let field = Rect::new(5, 2, 20, 1);
    assert_eq!(list(field, &combo(3, 0), SCREEN), Rect::new(4, 3, 22, 5));
    let low = Rect::new(5, 17, 20, 1);
    assert_eq!(list(low, &combo(3, 0), SCREEN), Rect::new(4, 12, 22, 5));
}

#[test]
fn a_long_list_shows_eight_rows_and_a_click_picks_the_option_under_it() {
    let field = Rect::new(5, 2, 20, 1);
    let c = combo(20, 15);
    let l = list(field, &c, SCREEN);
    assert_eq!(rows(l), COMBO_ROWS);
    assert_eq!(hit(l, &c, Position::new(10, l.y + 1)), Some(c.scroll()));
    assert_eq!(hit(l, &c, Position::new(10, l.y)), None);
}

#[test]
fn the_list_marks_its_choice_and_counts_what_is_hidden() {
    let view = view(&[]);
    let mut t = Terminal::new(TestBackend::new(SCREEN.width, SCREEN.height)).expect("terminal");
    let field = Rect::new(5, 2, 20, 1);
    let c = combo(20, 15);
    t.draw(|f| {
        draw_field(f, &view, field, Field { combo: &c, placeholder: "", focused: true });
        draw_list(f, &view, list(field, &c, SCREEN), &c);
    })
    .expect("draw");
    insta::assert_snapshot!(t.backend());
}
