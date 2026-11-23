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
    r#"{"action":"new","entry":{"name":"PRs","agent":"claude","model":null,"effort":null,"account":null,"access":null,"follow":null,"chat_apps":null,"about":"x"}}"#,
    BotRequest::New(NewBot { name: "PRs".into(), agent: Some("claude".into()), about: "x".into(), ..NewBot::default() })
)]
#[case::edit(
    r#"{"action":"edit","entry":{"bot":"prs","name":"Other","agent":null,"model":"","effort":null,"account":null,"access":null,"follow":null,"chat_apps":null,"about":null,"memory":null}}"#,
    BotRequest::Edit(EditBot { bot: "prs".into(), name: Some("Other".into()), model: Some(String::new()), ..EditBot::default() })
)]
#[case::edit_memory(
    r#"{"action":"edit","entry":{"bot":"prs","name":null,"agent":null,"model":null,"effort":null,"account":null,"access":null,"follow":null,"chat_apps":null,"about":null,"memory":"- squash"}}"#,
    BotRequest::Edit(EditBot { bot: "prs".into(), memory: Some("- squash".into()), ..EditBot::default() })
)]
#[case::remove(r#"{"action":"remove","entry":{"bot":"prs"}}"#, BotRequest::Remove(BotTarget { bot: "prs".into() }))]
#[case::reset(r#"{"action":"reset","entry":{"bot":"prs"}}"#, BotRequest::Reset(BotTarget { bot: "prs".into() }))]
#[case::seen(r#"{"action":"seen","entry":{"bot":"prs"}}"#, BotRequest::Seen(BotTarget { bot: "prs".into() }))]
#[case::stop(r#"{"action":"stop","entry":{"bot":"prs"}}"#, BotRequest::Stop(BotTarget { bot: "prs".into() }))]
#[case::setup(r#"{"action":"setup","entry":{"bot":"prs"}}"#, BotRequest::Setup(BotTarget { bot: "prs".into() }))]
#[case::about(r#"{"action":"about","entry":{"bot":"prs"}}"#, BotRequest::About(BotTarget { bot: "prs".into() }))]
#[case::memory(r#"{"action":"memory","entry":{"bot":"prs"}}"#, BotRequest::Memory(BotTarget { bot: "prs".into() }))]
#[case::show(r#"{"action":"show","entry":{"bot":"prs"}}"#, BotRequest::Show(BotTarget { bot: "prs".into() }))]
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
fn reading_what_a_bot_is_for_logs_the_bot_and_no_text() {
    let fields = Command::Bot(BotRequest::About(BotTarget { bot: "prs".into() })).fields();
    assert_eq!(fields, [("action", "about".to_string()), ("bot", "prs".to_string())]);
}

#[test]
fn a_memory_edit_logs_its_size_and_never_its_text() {
    let edit = BotRequest::Edit(EditBot { bot: "prs".into(), memory: Some("- private".into()), ..EditBot::default() });
    let text = format!("{:?}", Command::Bot(edit).fields());
    assert!(text.contains("(\"memory_bytes\", \"9\")") && !text.contains("private"), "{text}");
    let shown = Command::Bot(BotRequest::Show(BotTarget { bot: "prs".into() })).fields();
    assert_eq!(shown, [("action", "show".to_string()), ("bot", "prs".to_string())]);
}

#[test]
fn the_request_is_named_bot_in_the_list_of_commands() {
    assert_eq!(Command::Bot(BotRequest::List).name(), "bot");
}

#[test]
fn new_and_edit_serialize_follow() {
    let new = BotRequest::New(NewBot { name: "PRs".into(), follow: Some(false), ..NewBot::default() });
    assert_eq!(serde_json::to_value(&new).expect("json")["entry"]["follow"], Value::Bool(false));
    let edit = BotRequest::Edit(EditBot { bot: "prs".into(), follow: Some(true), ..EditBot::default() });
    assert_eq!(serde_json::to_value(&edit).expect("json")["entry"]["follow"], Value::Bool(true));
    let fields = Command::Bot(edit).fields();
    assert!(fields.contains(&("follow", "true".to_string())), "{fields:?}");
}

#[test]
fn new_and_edit_carry_chat_apps_and_log_it() {
    let new = BotRequest::New(NewBot { name: "PRs".into(), chat_apps: Some("always".into()), ..NewBot::default() });
    assert_eq!(serde_json::to_value(&new).expect("json")["entry"]["chat_apps"], Value::from("always"));
    assert!(Command::Bot(new).fields().contains(&("chat_apps", "always".to_string())));
    let edit = BotRequest::Edit(EditBot { bot: "prs".into(), chat_apps: Some("off".into()), ..EditBot::default() });
    assert_eq!(serde_json::to_value(&edit).expect("json")["entry"]["chat_apps"], Value::from("off"));
    assert!(Command::Bot(edit).fields().contains(&("chat_apps", "off".to_string())));
}

#[test]
fn bot_info_says_when_its_answers_reach_the_chat_apps() {
    let info = BotInfo { chat_apps: "away".into(), ..BotInfo::default() };
    assert_eq!(serde_json::to_value(&info).expect("json")["chat_apps"], Value::from("away"));
}

#[test]
fn bot_info_has_follow_and_following() {
    let info = BotInfo { follow: true, following: vec![4, 9], ..BotInfo::default() };
    let value = serde_json::to_value(&info).expect("json");
    assert_eq!((value["follow"].clone(), value["following"].clone()), (Value::Bool(true), serde_json::json!([4, 9])));
    let old: BotInfo = serde_json::from_str(r#"{"id":"prs"}"#).expect("an old answer");
    assert!(!old.follow && old.following.is_empty());
}

#[rstest]
#[case::set(r#"{"action":"secret-set","entry":{"bot":"prs","name":"pw","value":"hunter2-value"}}"#)]
#[case::list(r#"{"action":"secret-list","entry":{"bot":"prs"}}"#)]
#[case::remove_one(r#"{"action":"secret-remove","entry":{"bot":"prs","name":"pw","all":false}}"#)]
#[case::remove_all(r#"{"action":"secret-remove","entry":{"bot":"prs","name":null,"all":true}}"#)]
#[case::get(r#"{"action":"secret-get","entry":{"name":"pw"}}"#)]
#[case::ask(r#"{"action":"secret-ask","entry":{"names":["user","pw"],"about":"to log in","replace":false}}"#)]
#[case::give(r#"{"action":"secret-give","entry":{"bot":"prs","ask":3,"values":{"pw":"hunter2-value"}}}"#)]
#[case::decline(r#"{"action":"secret-decline","entry":{"bot":"prs","ask":3}}"#)]
fn each_secret_action_round_trips(#[case] args: &str) {
    let Command::Bot(parsed) = request(args).command else { panic!("not a bot request") };
    let again = serde_json::to_value(&parsed).expect("serialize");
    assert_eq!(again, serde_json::from_str::<Value>(args).expect("json"));
}

#[test]
fn the_log_line_of_a_secret_request_never_holds_a_value_or_a_name_that_is_not_one() {
    let set = BotRequest::SecretSet(SecretSet {
        bot: "prs".into(),
        name: "pw".into(),
        value: crate::issues::Secret("hunter2-value".into()),
    });
    let text = format!("{:?} {:?}", Command::Bot(set.clone()).fields(), set);
    assert!(
        text.contains("\"bytes\", \"13\"") && text.contains("\"name\", \"pw\"") && !text.contains("hunter2"),
        "{text}"
    );
    let mistaken = "hunter2 value with spaces";
    for request in [
        BotRequest::SecretGet(SecretGet { name: mistaken.into() }),
        BotRequest::SecretRemove(SecretRemove { bot: "prs".into(), name: Some(mistaken.into()), all: false }),
        BotRequest::SecretSet(SecretSet { bot: "prs".into(), name: mistaken.into(), ..SecretSet::default() }),
    ] {
        let text = format!("{:?}", Command::Bot(request).fields());
        assert!(text.contains("name_bytes") && !text.contains("hunter2"), "{text}");
    }
}

#[test]
fn a_name_equal_to_its_value_is_not_logged() {
    let same = BotRequest::SecretSet(SecretSet {
        bot: "prs".into(),
        name: "hunter2-value".into(),
        value: crate::issues::Secret("hunter2-value".into()),
    });
    let text = format!("{:?}", Command::Bot(same).fields());
    assert!(text.contains("name_bytes") && !text.contains("hunter2"), "{text}");
}

#[test]
fn the_log_line_of_an_ask_or_a_give_holds_counts_only() {
    let mistaken = "hunter2-value-as-a-key";
    let values = [(mistaken.to_string(), crate::issues::Secret("hunter2-value".into()))].into_iter().collect();
    let give = BotRequest::SecretGive(SecretGive { bot: "prs".into(), ask: 3, values });
    let text = format!("{:?}", Command::Bot(give).fields());
    assert!(text.contains("\"values\", \"1\"") && text.contains("\"ask\", \"3\""), "{text}");
    assert!(!text.contains("hunter2"), "{text}");
    let ask = BotRequest::SecretAsk(SecretAsk {
        names: vec!["a".into(), "b".into()],
        about: "hunter2 about".into(),
        replace: true,
    });
    let text = format!("{:?}", Command::Bot(ask).fields());
    assert!(text.contains("\"names\", \"2\"") && text.contains("about_bytes") && !text.contains("hunter2"), "{text}");
}

#[test]
fn bot_info_leaves_out_asks_when_there_are_none() {
    let value = serde_json::to_value(BotInfo::default()).expect("json");
    assert!(value.get("asks").is_none(), "{value}");
    let names = vec![String::from("pw"), String::from("user")];
    let ask = AskInfo { id: 3, names, about: "to log in".into(), missing: vec!["user".into()], ..AskInfo::default() };
    let value = serde_json::to_value(BotInfo { asks: vec![ask], ..BotInfo::default() }).expect("json");
    assert_eq!(value["asks"][0]["routine"], Value::Null);
    assert_eq!(value["asks"][0]["id"], serde_json::json!(3));
    assert_eq!(value["asks"][0]["missing"], serde_json::json!(["user"]));
}
