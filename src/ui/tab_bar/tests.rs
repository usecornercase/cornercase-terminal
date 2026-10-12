use rstest::rstest;

use super::*;
use crate::activity::Status;

const BAR: Rect = Rect { x: 10, y: 0, width: 40, height: HEIGHT };

fn tabs(names: &[&str]) -> Vec<TabEntry> {
    names.iter().map(|&n| TabEntry::from(n)).collect()
}

fn strip(names: &[&str], scroll: usize) -> Strip {
    Strip::new(BAR, &tabs(names), scroll)
}

fn many() -> Vec<&'static str> {
    vec!["one", "two", "three", "four", "five", "six", "seven"]
}

mod geometry {
    use super::*;

    #[test]
    fn a_tab_is_its_icon_and_name_padded_with_room_for_the_close_button() {
        assert_eq!(width(&TabEntry::from("zsh")), 1 + 2 + 3 + MENU_WIDTH + CLOSE_WIDTH);
    }

    #[test]
    fn an_agent_tab_is_as_wide_as_a_shell_tab_with_the_same_name() {
        let tab = TabEntry { status: Some(Status::Working), ..TabEntry::from("claude") };
        assert_eq!(width(&tab), width(&TabEntry::from("claude")));
    }

    #[test]
    fn the_other_panes_of_a_split_take_their_count() {
        let tab = TabEntry { others: 2, ..TabEntry::from("zsh") };
        assert_eq!(width(&tab), 1 + 2 + 3 + 3 + MENU_WIDTH + CLOSE_WIDTH);
    }

    #[test]
    fn a_long_name_is_cut() {
        assert_eq!(width(&TabEntry::from("x".repeat(80).as_str())), 1 + 2 + 24 + MENU_WIDTH + CLOSE_WIDTH);
    }

    #[test]
    fn tabs_that_fit_sit_side_by_side_with_the_new_button_after_them() {
        let s = strip(&["zsh", "claude"], 0);
        assert_eq!((s.item(0).x, s.item(1).x, s.new_button().x), (10, 21, 35));
        assert_eq!((s.left(), s.right()), (Rect::default(), Rect::default()));
    }

    #[test]
    fn tabs_that_do_not_fit_get_arrows_and_the_new_button_at_the_end() {
        let s = strip(&many(), 0);
        assert_eq!((s.left().x, s.new_button().right()), (BAR.x, BAR.right()));
        assert_eq!(s.right().right(), s.new_button().x);
        assert_eq!(s.item(0).x, BAR.x + BUTTON_WIDTH);
        assert_eq!(s.hidden(), (false, true));
    }

    #[test]
    fn scrolling_moves_by_one_tab_and_stops_at_the_last_one() {
        let s = strip(&many(), 0);
        assert_eq!(s.scrolled(1), 1);
        assert_eq!(s.scrolled(-1), 0);
        let last = strip(&many(), 100);
        assert_eq!(last.hidden(), (true, false));
        assert_eq!(last.scrolled(1), last.first());
    }

    #[test]
    fn revealing_a_hidden_tab_scrolls_until_it_shows() {
        let s = strip(&many(), 0);
        let first = s.reveal(6);
        assert!(!strip(&many(), first).item(6).is_empty());
        assert_eq!(strip(&many(), first).reveal(0), 0);
    }

    #[test]
    fn a_tab_wider_than_the_bar_still_shows_cut() {
        let s = Strip::new(Rect { width: 12, ..BAR }, &tabs(&["a-very-long-name"]), 0);
        assert!(!s.item(0).is_empty());
    }
}

mod hits {
    use super::*;

    #[rstest]
    #[case::the_name(13, Some(Hit::Tab(0)))]
    #[case::the_menu_button(17, Some(Hit::Menu(0)))]
    #[case::the_close_button(19, Some(Hit::Close(0)))]
    #[case::the_new_button(36, Some(Hit::New))]
    #[case::past_the_tabs(40, None)]
    fn on_a_bar_that_fits(#[case] x: u16, #[case] hit: Option<Hit>) {
        assert_eq!(strip(&["zsh", "claude"], 0).hit(Position::new(x, 0)), hit);
    }

    #[test]
    fn the_arrows_scroll() {
        let s = strip(&many(), 0);
        assert_eq!(s.hit(s.left().as_position()), Some(Hit::Scroll(-1)));
        assert_eq!(s.hit(s.right().as_position()), Some(Hit::Scroll(1)));
    }

    #[test]
    fn the_details_row_is_not_a_button() {
        assert_eq!(strip(&["zsh"], 0).hit(Position::new(11, 1)), None);
    }

    #[test]
    fn holding_a_drag_on_an_arrow_scrolls_only_when_there_is_more() {
        let s = strip(&many(), 0);
        assert_eq!((s.edge(s.left().as_position()), s.edge(s.right().as_position())), (None, Some(1)));
    }
}

mod drops {
    use super::*;

    fn at(dragged: usize, onto: usize) -> Option<usize> {
        let s = strip(&["a", "b", "c", "d"], 0);
        s.drop(dragged, s.item(onto).as_position())
    }

    #[rstest]
    #[case::onto_a_tab_on_the_right(0, 2, Some(3))]
    #[case::onto_a_tab_on_the_left(3, 1, Some(1))]
    #[case::onto_itself(2, 2, None)]
    fn the_tab_under_the_pointer_gives_the_slot(
        #[case] dragged: usize,
        #[case] onto: usize,
        #[case] before: Option<usize>,
    ) {
        assert_eq!(at(dragged, onto), before);
    }

    #[test]
    fn past_the_last_tab_is_the_end() {
        let s = strip(&["a", "b", "c", "d"], 0);
        assert_eq!(s.drop(0, Position::new(BAR.right() - 1, 0)), Some(4));
    }

    #[test]
    fn outside_the_bar_there_is_no_slot() {
        let s = strip(&["a", "b"], 0);
        assert_eq!(s.drop(0, Position::new(BAR.x, 1)), None);
    }
}
