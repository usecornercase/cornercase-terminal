use clap::CommandFactory;
use serde_json::json;

use super::*;
use crate::cli::args::Cli;

fn parsed(args: &[&str]) -> std::result::Result<(), clap::Error> {
    let line = std::iter::once("cornercase").chain(args.iter().copied());
    Cli::command().try_get_matches_from(line).map(drop)
}

#[rstest::rstest]
#[case::set(&["bot", "secret", "set", "campus", "pw"], true)]
#[case::set_with_the_value_as_an_argument(&["bot", "secret", "set", "campus", "pw", "hunter2"], false)]
#[case::set_without_a_name(&["bot", "secret", "set", "campus"], false)]
#[case::list_inside_a_bot(&["bot", "secret", "list"], true)]
#[case::list_json(&["bot", "secret", "list", "campus", "--json"], true)]
#[case::remove_one(&["bot", "secret", "remove", "campus", "pw"], true)]
#[case::remove_all(&["bot", "secret", "remove", "campus", "--all"], true)]
#[case::remove_nothing(&["bot", "secret", "remove", "campus"], false)]
#[case::remove_both(&["bot", "secret", "remove", "campus", "pw", "--all"], false)]
#[case::get(&["bot", "secret", "get", "pw"], true)]
#[case::get_without_a_name(&["bot", "secret", "get"], false)]
fn the_commands_take_what_they_say(#[case] args: &[&str], #[case] fine: bool) {
    assert_eq!(parsed(args).is_ok(), fine);
}

#[rstest::rstest]
#[case::bare("hunter2", "hunter2")]
#[case::one_break("hunter2\n", "hunter2")]
#[case::two_breaks("hunter2\n\n", "hunter2\n")]
#[case::windows_break("hunter2\r\n", "hunter2")]
#[case::spaces_stay(" a b \n", " a b ")]
fn one_line_break_goes_and_nothing_else(#[case] given: &str, #[case] kept: &str) {
    assert_eq!(without_one_break(given.to_string()), kept);
}

#[test]
fn the_list_says_each_name_and_the_key() {
    let answer = json!({ "bot": "campus", "key": "file", "secrets": [{ "name": "pw", "saved": 100 }] });
    let text = listing(&answer);
    assert!(text.starts_with("pw\tsaved ") && text.ends_with("key: file\n"));
}

#[test]
fn an_empty_list_says_so() {
    assert_eq!(listing(&json!({ "bot": "campus", "key": "file", "secrets": [] })), "campus has no secrets\n");
}
