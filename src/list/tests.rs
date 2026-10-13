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
