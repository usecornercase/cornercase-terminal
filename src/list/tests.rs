use super::*;

mod search {
    use super::*;

    #[test]
    fn typing_selects_the_first_result() {
        let mut s = Search::default();
        s.move_selection(2, 5, 10);
        s.push('a');
        assert_eq!((s.query(), s.selected()), ("a", 0));
    }

    #[test]
    fn the_selection_stays_inside_the_results() {
        let mut s = Search::default();
        s.move_selection(10, 3, 10);
        assert_eq!(s.selected(), 2);
    }

    #[test]
    fn moving_down_scrolls_to_keep_the_selection_visible() {
        let mut s = Search::default();
        s.move_selection(4, 10, 3);
        assert_eq!(s.scroll(), 2);
    }

    #[test]
    fn scrolling_stops_at_the_last_page() {
        let mut s = Search::default();
        s.scroll_by(100, 10, 3);
        assert_eq!(s.scroll(), 7);
    }
}

mod combo {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn combo(chosen: usize) -> Combo {
        Combo::new(
            ["web-shop · root", "web-shop › main", "blog › draft", "api · root"].map(String::from).into(),
            Some(chosen),
        )
    }

    fn key(c: &mut Combo, code: KeyCode) -> bool {
        c.key(KeyEvent::new(code, KeyModifiers::NONE), COMBO_ROWS)
    }

    #[test]
    fn opens_on_the_chosen_option_and_enter_chooses_the_highlighted_one() {
        let mut c = combo(1);
        assert!(!key(&mut c, KeyCode::Down));
        assert_eq!((c.is_open(), c.highlighted()), (true, Some(1)));
        key(&mut c, KeyCode::Down);
        assert!(key(&mut c, KeyCode::Enter));
        assert_eq!((c.is_open(), c.label()), (false, Some("blog › draft")));
    }

    #[test]
    fn typing_filters_and_escape_keeps_the_choice() {
        let mut c = combo(0);
        key(&mut c, KeyCode::Char('A'));
        key(&mut c, KeyCode::Char('P'));
        assert_eq!(c.shown(), [3]);
        key(&mut c, KeyCode::Backspace);
        key(&mut c, KeyCode::Backspace);
        assert_eq!(c.shown().len(), 4);
        key(&mut c, KeyCode::Char('b'));
        key(&mut c, KeyCode::Esc);
        assert_eq!((c.is_open(), c.chosen()), (false, Some(0)));
    }

    #[test]
    fn enter_on_the_same_option_or_on_no_match_changes_nothing() {
        let mut c = combo(2);
        key(&mut c, KeyCode::Enter);
        assert!(!key(&mut c, KeyCode::Enter));
        key(&mut c, KeyCode::Char('z'));
        assert!(!key(&mut c, KeyCode::Enter));
        assert_eq!((c.is_open(), c.chosen()), (false, Some(2)));
    }

    #[test]
    fn a_long_list_opens_scrolled_to_its_choice() {
        let mut c = Combo::new((0..20).map(|i| i.to_string()).collect(), Some(15));
        c.open();
        assert_eq!((c.highlighted(), c.scroll()), (Some(15), 8));
        c.scroll_by(-3, COMBO_ROWS);
        assert_eq!(c.scroll(), 5);
    }

    #[test]
    fn a_choice_out_of_range_is_no_choice() {
        assert_eq!(Combo::new(vec!["a".into()], Some(3)).label(), None);
    }
}
