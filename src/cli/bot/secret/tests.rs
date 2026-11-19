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
#[case::ask(&["bot", "secret", "ask", "pw", "--about", "to log in"], true)]
#[case::ask_four(&["bot", "secret", "ask", "a", "b", "c", "d", "--about", "x", "--replace"], true)]
#[case::ask_five(&["bot", "secret", "ask", "a", "b", "c", "d", "e", "--about", "x"], false)]
#[case::ask_without_about(&["bot", "secret", "ask", "pw"], false)]
#[case::ask_without_a_name(&["bot", "secret", "ask", "--about", "x"], false)]
#[case::give(&["bot", "secret", "give", "campus", "3", "--json"], true)]
#[case::give_without_an_ask(&["bot", "secret", "give", "campus"], false)]
#[case::decline(&["bot", "secret", "decline", "campus", "3"], true)]
#[case::decline_with_a_word(&["bot", "secret", "decline", "campus", "three"], false)]
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

mod given_values {
    use super::*;

    #[rstest::rstest]
    #[case::not_json("hunter2-value")]
    #[case::an_array(r#"["hunter2-value"]"#)]
    #[case::a_number(r#"{"pw": 1}"#)]
    #[case::a_nested_object(r#"{"pw": {"v": "hunter2-value"}}"#)]
    fn are_an_object_of_strings_and_never_echoed(#[case] text: &str) {
        let error = values_of(text).expect_err("refused").to_string();
        assert!(error.contains("not a JSON object of name to value") && !error.contains("hunter2"), "{error}");
    }

    #[test]
    fn are_read_name_to_value() {
        let values = values_of(r#"{"user":"me","pw":"hunter2-value"}"#).expect("an object");
        assert_eq!(values.keys().collect::<Vec<_>>(), ["pw", "user"]);
        assert_eq!(values["pw"].0, "hunter2-value");
    }

    #[test]
    fn the_ask_line_says_whether_it_was_asked_before() {
        assert_eq!(asked_line(&json!({"names": ["a", "b"], "ask": 3, "already": false})), "asked: a, b (ask 3)\n");
        assert_eq!(asked_line(&json!({"names": ["a"], "ask": 3, "already": true})), "already asked: a (ask 3)\n");
    }
}
