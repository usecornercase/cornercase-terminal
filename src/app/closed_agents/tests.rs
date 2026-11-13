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
}
