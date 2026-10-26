use rstest::rstest;
use serde_json::Value;

use super::away_line;

#[rstest]
#[case::one_platform(serde_json::json!({"away": true, "platforms": ["Telegram"]}), "away: agents message you on Telegram until you turn it off\n")]
#[case::both(serde_json::json!({"away": true, "platforms": ["Telegram", "Discord"]}), "away: agents message you on Telegram and Discord until you turn it off\n")]
#[case::back(serde_json::json!({"away": false, "platforms": ["Discord"]}), "back: agents no longer message you on Discord\n")]
#[case::an_older_server(serde_json::json!({"away": true}), "away: agents message you on Telegram until you turn it off\n")]
#[case::nothing_was_away(serde_json::json!({"away": false, "platforms": []}), "back: nothing was away\n")]
fn away_names_the_platforms_it_changed(#[case] value: Value, #[case] expected: &str) {
    assert_eq!(away_line(&value), expected);
}
