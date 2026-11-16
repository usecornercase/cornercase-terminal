use rstest::rstest;

use super::*;

mod destinations {
    use super::*;

    #[rstest]
    #[case::a_host("devbox")]
    #[case::a_user_and_a_host("me@devbox.example.com")]
    #[case::a_port("ssh://me@devbox:2222")]
    #[case::an_address("10.0.0.7")]
    fn ssh_takes_it(#[case] destination: &str) {
        assert!(valid(destination));
    }

    #[rstest]
    #[case::empty("")]
    #[case::an_option("-oProxyCommand=sh")]
    #[case::two_words("devbox ls")]
    #[case::a_line_break("devbox\nls")]
    #[case::a_tab("dev\tbox")]
    fn it_is_refused(#[case] destination: &str) {
        assert!(!valid(destination));
    }

    #[test]
    fn a_destination_longer_than_a_host_name_is_refused() {
        assert!(!valid(&"a".repeat(LONGEST + 1)));
    }
}
