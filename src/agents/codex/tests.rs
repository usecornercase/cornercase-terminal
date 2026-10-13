use rstest::rstest;

use super::*;

mod codex_title {
    use super::*;

    #[rstest]
    #[case::starting("⠏ ⠏ | shop", false, Activity::Working)]
    #[case::working("⠴ Check example.com status | shop", false, Activity::Working)]
    #[case::approval("[ ! ] Action Required | Check example.com status | shop", true, Activity::Waiting)]
    #[case::approval_blinking("[ . ] Action Required | Check example.com status | shop", true, Activity::Waiting)]
    #[case::finished("Check example.com status | shop", false, Activity::Idle)]
    #[case::fresh("shop", false, Activity::Idle)]
    #[case::no_activity_in_the_title("shop", true, Activity::Working)]
    #[case::approval_without_a_turn("[ ! ] Action Required | shop", false, Activity::Waiting)]
    #[case::empty("", false, Activity::Idle)]
    fn shows_what_codex_is_doing(#[case] title: &str, #[case] turn: bool, #[case] expected: Activity) {
        assert_eq!(activity(title, turn), expected);
    }
}
