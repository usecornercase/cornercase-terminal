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

    fn noted() -> Combo {
        let notes = vec![None, Some(String::from("not yet")), None];
        Combo::with_notes(vec!["claude".into(), "gemini".into(), "codex".into()], Some(0), notes)
    }

    #[test]
    fn an_option_with_a_note_cannot_be_chosen_and_the_list_stays_open() {
        let mut c = noted();
        c.open();
        c.choose(1);
        assert_eq!((c.chosen(), c.is_open(), c.note(1)), (Some(0), true, Some("not yet")));
    }

    #[test]
    fn enter_on_an_option_with_a_note_chooses_nothing() {
        let mut c = noted();
        c.open();
        key(&mut c, KeyCode::Down);
        assert!(!key(&mut c, KeyCode::Enter));
        assert_eq!((c.chosen(), c.is_open()), (Some(0), true));
    }

    #[test]
    fn an_option_without_a_note_is_chosen_beside_one_with_a_note() {
        let mut c = noted();
        c.open();
        c.choose(2);
        assert_eq!((c.chosen(), c.is_open(), c.note(2)), (Some(2), false, None));
    }

    #[test]
    fn a_typed_value_that_matches_nothing_is_added_and_chosen_on_enter() {
        let mut c = Combo::typed(vec!["default".into(), "opus".into()], Some(0));
        c.open();
        "my-model".chars().for_each(|ch| _ = key(&mut c, KeyCode::Char(ch)));
        assert!(key(&mut c, KeyCode::Enter));
        assert_eq!((c.label(), c.options().len(), c.is_open()), (Some("my-model"), 3, false));
    }

    #[test]
    fn a_combo_without_typing_adds_nothing() {
        let mut c = Combo::new(vec!["default".into(), "opus".into()], Some(0));
        c.open();
        "zzz".chars().for_each(|ch| _ = key(&mut c, KeyCode::Char(ch)));
        key(&mut c, KeyCode::Enter);
        assert_eq!((c.label(), c.options().len()), (Some("default"), 2));
    }
}

mod chosen {
    use super::*;

    #[test]
    fn the_selection_follows_its_id_when_rows_arrive_around_it() {
        let chosen = Chosen::new(Some(12));
        assert_eq!(chosen.index(&[10, 11, 12]), Some(2));
        assert_eq!(chosen.index(&[9, 12, 10, 11]), Some(1));
    }

    #[test]
    fn a_missing_selection_falls_back_to_the_first_row() {
        let chosen = Chosen::new(Some(99));
        assert_eq!(chosen.index(&[10, 11]), Some(0));
        assert_eq!(chosen.index(&[]), None);
    }

    #[test]
    fn moving_scrolls_to_keep_the_selection_in_view() {
        let ids: Vec<u64> = (0..10).collect();
        let mut chosen = Chosen::new(None);
        chosen.move_by(6, &ids, 3);
        assert_eq!((chosen.selected, chosen.scroll), (Some(6), 4));
        chosen.move_by(isize::MIN, &ids, 3);
        assert_eq!((chosen.selected, chosen.scroll), (Some(0), 0));
    }

    #[test]
    fn a_wheel_scroll_stops_following_until_the_next_move() {
        let ids: Vec<u64> = (0..10).collect();
        let mut chosen = Chosen::new(Some(0));
        chosen.scroll_by(4, ids.len(), 3);
        chosen.fit(&ids, 3);
        assert_eq!(chosen.scroll, 4);
        chosen.move_by(1, &ids, 3);
        chosen.fit(&ids, 3);
        assert_eq!((chosen.selected, chosen.scroll), (Some(1), 1));
    }

    #[test]
    fn choosing_outside_the_rows_changes_nothing() {
        let mut chosen = Chosen::new(Some(11));
        assert_eq!(chosen.choose(5, &[10, 11]), None);
        assert_eq!(chosen.selected, Some(11));
        assert_eq!(chosen.choose(0, &[10, 11]), Some(10));
    }
}
