use rstest::rstest;
use serde_json::Value;

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};

fn request(args: &str) -> Request {
    let text = format!(r#"{{"command":"bot","args":{args}}}"#);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{text}: {error}"))
}

#[rstest]
#[case::agents(r#"{"action":"agents"}"#, BotRequest::Agents)]
#[case::list(r#"{"action":"list"}"#, BotRequest::List)]
#[case::new(
    r#"{"action":"new","entry":{"name":"PRs","agent":"claude","model":null,"effort":null,"account":null,"access":null,"about":"x"}}"#,
    BotRequest::New(NewBot { name: "PRs".into(), agent: Some("claude".into()), about: "x".into(), ..NewBot::default() })
)]
#[case::edit(
    r#"{"action":"edit","entry":{"bot":"prs","name":"Other","agent":null,"model":"","effort":null,"account":null,"access":null}}"#,
    BotRequest::Edit(EditBot { bot: "prs".into(), name: Some("Other".into()), model: Some(String::new()), ..EditBot::default() })
)]
#[case::remove(r#"{"action":"remove","entry":{"bot":"prs"}}"#, BotRequest::Remove(BotTarget { bot: "prs".into() }))]
#[case::reset(r#"{"action":"reset","entry":{"bot":"prs"}}"#, BotRequest::Reset(BotTarget { bot: "prs".into() }))]
#[case::seen(r#"{"action":"seen","entry":{"bot":"prs"}}"#, BotRequest::Seen(BotTarget { bot: "prs".into() }))]
#[case::stop(r#"{"action":"stop","entry":{"bot":"prs"}}"#, BotRequest::Stop(BotTarget { bot: "prs".into() }))]
#[case::say(
    r#"{"action":"say","entry":{"bot":"prs","text":"hi","wait":true}}"#,
    BotRequest::Say(BotSay { bot: "prs".into(), text: "hi".into(), wait: true })
)]
#[case::history(
    r#"{"action":"history","entry":{"bot":"prs","last":5,"after":9}}"#,
    BotRequest::History(BotHistory { bot: "prs".into(), last: Some(5), after: Some(9) })
)]
fn every_action_travels_as_the_request_it_names(#[case] args: &str, #[case] wanted: BotRequest) {
    let read = request(args);
    assert_eq!(read.command, Command::Bot(wanted));
    let raw: Value = serde_json::from_str(args).expect("json");
    let Command::Bot(parsed) = read.command else { panic!("a bot request") };
    assert_eq!(unknown_fields(&raw, &parsed), None, "{args}");
    assert_eq!(serde_json::to_value(&parsed).expect("json"), raw);
}

#[test]
fn a_key_the_server_does_not_know_is_named_to_a_strict_client() {
    let raw: Value =
        serde_json::from_str(r#"{"action":"say","entry":{"bot":"prs","text":"hi","wait":false,"loud":true}}"#)
            .expect("json");
    let parsed: BotRequest = serde_json::from_value(raw.clone()).expect("a request");
    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("entry.loud"));
}

#[test]
fn the_log_line_says_how_long_a_message_is_and_never_what_it_says() {
    let say = Command::Bot(BotRequest::Say(BotSay { bot: "prs".into(), text: "secret plan".into(), wait: true }));
    let fields = say.fields();
    assert_eq!(
        fields,
        [
            ("action", "say".to_string()),
            ("bot", "prs".to_string()),
            ("bytes", "11".to_string()),
            ("wait", "true".to_string())
        ]
    );
    let new =
        Command::Bot(BotRequest::New(NewBot { name: "PRs".into(), about: "private".into(), ..NewBot::default() }));
    let text = format!("{:?}", new.fields());
    assert!(text.contains("about_bytes") && !text.contains("private"), "{text}");
}

#[test]
fn the_request_is_named_bot_in_the_list_of_commands() {
    assert_eq!(Command::Bot(BotRequest::List).name(), "bot");
}
