use ratatui::Terminal;
use ratatui::backend::TestBackend;
use rstest::rstest;

use super::*;

const AREA: Rect = Rect::new(0, 0, 40, 14);

fn item(id: u64, text: &str, done: bool) -> Item {
    Item { id, text: text.into(), done, editing: None }
}

fn view(items: Vec<Item>) -> View {
    View { items, scroll: 0, adding: None, light: false, muted: Color::DarkGray, drag: None, compact: false }
}

fn list() -> View {
    view(vec![
        item(1, "fix the login bug in safari when the cookie expires", false),
        item(2, "write docs", false),
        item(3, "renew the domain", true),
    ])
}

fn render(view: &View, hover: Option<Position>) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(AREA.width, AREA.height)).expect("terminal");
    t.draw(|f| draw(f, AREA, view, hover)).expect("draw");
    t
}

fn row(view: &View, i: usize) -> Rect {
    rows(AREA, view).item(i)
}

mod layout {
    use super::*;

    #[test]
    fn long_text_wraps_onto_more_rows() {
        let v = list();
        assert_eq!((text_width(AREA), row(&v, 0).height, row(&v, 1).height), (28, 2, 1));
    }

    #[test]
    fn clear_done_shows_only_with_done_items() {
        let v = view(vec![item(1, "a", false)]);
        assert_eq!(clear_done(AREA, &v), Rect::default());
        assert_eq!(clear_done(AREA, &list()).right() + 1, close(AREA).x);
    }
}

mod hits {
    use super::*;

    #[test]
    fn buttons_on_an_item_row() {
        let v = list();
        let r = row(&v, 0);
        assert_eq!(hit(AREA, &v, check(r).as_position()), Some(Hit::Check(1)));
        assert_eq!(hit(AREA, &v, delete(r).as_position()), Some(Hit::Delete(1)));
    }

    #[test]
    fn a_pending_item_has_a_send_button_and_a_done_one_has_none() {
        let v = list();
        let (pending, done) = (row(&v, 1), row(&v, 2));
        assert_eq!(hit(AREA, &v, send(pending).as_position()), Some(Hit::Send(2, pending)));
        assert!(!matches!(hit(AREA, &v, send(done).as_position()), Some(Hit::Send(..))));
    }

    #[test]
    fn the_text_gives_its_line_and_column() {
        let v = list();
        let r = row(&v, 0);
        let pos = Position::new(r.x + TEXT_X + 4, r.y + 1);
        assert_eq!(hit(AREA, &v, pos), Some(Hit::Item { id: 1, row: r, line: 1, col: 4 }));
    }

    #[test]
    fn the_header_closes_and_clears() {
        let v = list();
        assert_eq!(hit(AREA, &v, close(AREA).as_position()), Some(Hit::Close));
        assert_eq!(hit(AREA, &v, clear_done(AREA, &v).as_position()), Some(Hit::ClearDone));
    }

    #[test]
    fn the_new_button_opens_a_field_that_then_takes_clicks() {
        let mut v = list();
        assert_eq!(hit(AREA, &v, rows(AREA, &v).button().as_position()), Some(Hit::Add));
        v.adding = Some(Editor::new("call"));
        let button = rows(AREA, &v).button();
        let pos = Position::new(button.x + TEXT_X + 2, button.y);
        assert_eq!(hit(AREA, &v, pos), Some(Hit::Field { item: None, line: 0, col: 2 }));
    }

    #[test]
    fn an_item_being_edited_keeps_its_delete_button() {
        let mut v = list();
        v.items[1].editing = Some(Editor::new("write docs"));
        assert_eq!(hit(AREA, &v, delete(row(&v, 1)).as_position()), Some(Hit::Delete(2)));
    }

    #[test]
    fn an_item_being_edited_takes_clicks_as_a_field() {
        let mut v = list();
        v.items[1].editing = Some(Editor::new("write docs"));
        let r = row(&v, 1);
        let pos = Position::new(r.x + TEXT_X + 1, r.y);
        assert_eq!(hit(AREA, &v, pos), Some(Hit::Field { item: Some(2), line: 0, col: 1 }));
    }
}

mod dropping {
    use super::*;

    fn four() -> View {
        view(vec![item(1, "a", false), item(2, "b", false), item(3, "c", false), item(4, "d", true)])
    }

    #[rstest]
    #[case::onto_a_row_below(1, 2, 3)]
    #[case::onto_a_row_above(3, 0, 0)]
    #[case::onto_itself(2, 1, 1)]
    #[case::onto_a_done_item_stays_with_the_pending_ones(1, 3, 3)]
    #[case::a_done_item_stays_with_the_done_ones(4, 0, 3)]
    fn the_row_under_the_pointer_gives_the_slot(#[case] id: u64, #[case] onto: usize, #[case] before: usize) {
        let v = four();
        assert_eq!(drop(AREA, &v, id, row(&v, onto).as_position()), Some(before));
    }

    #[test]
    fn outside_the_list_there_is_no_slot() {
        let v = four();
        assert_eq!(drop(AREA, &v, 1, close(AREA).as_position()), None);
    }
}

mod drawing {
    use super::*;

    #[test]
    fn the_list() {
        insta::assert_snapshot!(render(&list(), None).backend());
    }

    #[test]
    fn hovering_an_item_shows_its_delete_button() {
        let v = list();
        let r = row(&v, 0);
        let t = render(&v, Some(r.as_position()));
        let x = delete(r);
        assert_eq!(t.backend().buffer()[(x.x + 1, x.y)].symbol(), "×");
        assert_eq!(t.backend().buffer()[(r.x, r.y)].bg, crate::ui::theme::DARK_HOVER);
    }

    #[test]
    fn hovering_a_pending_item_shows_its_send_button_and_a_done_one_has_none() {
        let v = list();
        let (pending, done) = (row(&v, 1), row(&v, 2));
        let t = render(&v, Some(pending.as_position()));
        let x = send(pending);
        assert_eq!(t.backend().buffer()[(x.x + 1, x.y)].symbol(), "▸");
        let t = render(&v, Some(done.as_position()));
        let x = send(done);
        assert_eq!(t.backend().buffer()[(x.x + 1, x.y)].symbol(), " ");
    }

    #[test]
    fn the_send_button_is_cyan_under_the_pointer() {
        let v = list();
        let x = send(row(&v, 1));
        let t = render(&v, Some(x.as_position()));
        assert_eq!(t.backend().buffer()[(x.x + 1, x.y)].fg, Color::Cyan);
    }

    #[test]
    fn compact_mode_always_shows_the_send_button_dimmed() {
        let v = View { compact: true, ..list() };
        insta::assert_snapshot!(render(&v, None).backend());
        let x = send(row(&v, 1));
        assert_eq!(render(&v, None).backend().buffer()[(x.x + 1, x.y)].fg, Color::DarkGray);
    }

    #[test]
    fn hovering_a_pending_item_draws_the_send_button_in_the_snapshot() {
        let v = list();
        insta::assert_snapshot!(render(&v, Some(row(&v, 1).as_position())).backend());
    }

    #[test]
    fn an_item_being_edited_shows_its_delete_button_without_hover() {
        let mut v = list();
        v.items[1].editing = Some(Editor::new("write docs"));
        let x = delete(row(&v, 1));
        let t = render(&v, None);
        let cell = &t.backend().buffer()[(x.x + 1, x.y)];
        assert_eq!((cell.symbol(), cell.fg), ("×", Color::DarkGray));
    }

    #[test]
    fn a_new_item_field_shows_the_cursor() {
        let mut v = list();
        v.adding = Some(Editor::new("call the bank"));
        let mut t = render(&v, None);
        let button = rows(AREA, &v).button();
        insta::assert_snapshot!(t.backend());
        assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(button.x + TEXT_X + 13, button.y));
    }

    #[test]
    fn dragging_draws_a_line_where_it_lands() {
        let v = View { drag: Some((2, Some(0))), ..list() };
        insta::assert_snapshot!(render(&v, None).backend());
    }

    #[test]
    fn a_long_list_shows_how_many_are_hidden() {
        let v = view((0..20).map(|i| item(i, &format!("task {i}"), false)).collect());
        insta::assert_snapshot!(render(&v, None).backend());
    }
}
