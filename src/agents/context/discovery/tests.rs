use rstest::rstest;

use super::*;

#[rstest]
#[case::direct(&["/usr/local/bin/gemini"], true)]
#[case::parent(&["node", "/usr/local/bin/gemini"], true)]
#[case::child(&["/usr/local/bin/node", "--max-old-space-size=8192", "/usr/local/bin/gemini"], true)]
#[case::argument(&["cat", "--", "/tmp/gemini"], false)]
#[case::other_script(&["node", "--max-old-space-size=8192", "/tmp/other", "/tmp/gemini"], false)]
fn a_node_child_is_recognized_past_its_runtime_flags(#[case] args: &[&str], #[case] expected: bool) {
    assert_eq!(named(&args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>(), "gemini"), expected);
}
