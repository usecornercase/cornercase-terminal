use super::*;

mod the_closing_text {
    use super::*;

    #[test]
    fn it_adds_the_clause_before_the_reopen_hint() {
        let said = closed_and(5, Some("15.7 GB of builds freed"));
        assert_eq!(said, "5 agents closed, 15.7 GB of builds freed · ▶ on the workspace reopens them");
    }

    #[test]
    fn without_a_clause_it_is_the_plain_message() {
        assert_eq!(closed_and(1, None), closed_message(1));
        assert_eq!(closed_message(1), "1 agent closed · ▶ on the workspace reopens it");
    }

    #[test]
    fn the_words_stop_before_the_reopen_hint() {
        assert_eq!(closed_words(5, Some("15.7 GB of builds freed")), "5 agents closed, 15.7 GB of builds freed");
        assert_eq!(closed_words(1, None), "1 agent closed");
    }

    #[test]
    fn the_hint_is_what_the_toast_adds() {
        assert_eq!(reopen_hint(1), " · ▶ on the workspace reopens it");
        assert_eq!(reopen_hint(2), " · ▶ on the workspace reopens them");
        assert_eq!(closed_and(3, Some("x")), closed_words(3, Some("x")) + &reopen_hint(3));
    }
}
