use super::*;

#[test]
fn a_server_without_restart_when_idle_is_called_too_old_to_wait() {
    let refused = Error::Control(control::unknown_command("restart-when-idle"));

    assert_eq!(too_old_to_wait(refused).to_string(), TOO_OLD_TO_WAIT);
}

mod kill_server {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::outside(false, false, false, Kill::Go)]
    #[case::outside_with_terminal(false, false, true, Kill::Go)]
    #[case::inside_with_yes(true, true, false, Kill::Go)]
    #[case::inside_at_a_terminal(true, false, true, Kill::Ask)]
    #[case::inside_without_terminal(true, false, false, Kill::Refuse)]
    fn asks_only_from_inside(#[case] inside: bool, #[case] yes: bool, #[case] terminal: bool, #[case] expected: Kill) {
        assert_eq!(decide(inside, yes, terminal), expected);
    }
}
