use rstest::rstest;

use super::on_a_machine::start;
use super::parse;

#[rstest]
#[case::here(&["start", "claude", "--placed"], true)]
#[case::on_a_machine(&["start", "claude", "--machine", "vps", "--project", "2", "--issue", "#7", "--placed", "--json"], true)]
#[case::only_when_asked(&["start", "claude"], false)]
fn a_start_takes_placed(#[case] args: &[&str], #[case] placed: bool) {
    assert_eq!(start(args).placed, placed);
}

#[rstest]
#[case::wait(&["--wait"])]
#[case::until(&["--wait", "--until", "idle"])]
#[case::until_alone(&["--until", "idle"])]
#[case::timeout(&["--timeout", "30"])]
#[case::on_a_machine(&["--machine", "vps", "--timeout", "30"])]
fn a_placed_start_refuses_to_wait_with_status_2(#[case] waits: &[&str]) {
    let args = [&["start", "claude", "--placed"], waits].concat();

    let refused = parse(&args).expect_err("wrong usage");

    assert_eq!(refused.exit_code(), 2, "{refused}");
    assert!(refused.to_string().contains("cannot be used with"), "{refused}");
}
