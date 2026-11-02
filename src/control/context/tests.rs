use super::*;
use crate::control::Command;

#[test]
fn every_key_is_present_even_when_empty() {
    let value = serde_json::to_value(ContextInfo::default()).expect("serializes");
    let object = value.as_object().expect("an object");
    for key in [
        "pane",
        "agent",
        "title",
        "model",
        "percent",
        "compacted",
        "loading",
        "read_at",
        "age",
        "breakdown",
        "notice",
        "error",
    ] {
        assert!(object.contains_key(key), "{key} is missing from {value}");
    }
}

#[test]
fn a_category_kind_stays_a_string() {
    let json = r#"{"categories":[{"id":"x","name":"X","tokens":1,"kind":"never seen before"}]}"#;
    let breakdown: Breakdown = serde_json::from_str(json).expect("a lenient read");
    assert_eq!(breakdown.categories[0].kind, "never seen before");
}

#[test]
fn an_older_answer_is_read_without_the_newer_keys() {
    let info: ContextInfo = serde_json::from_str(r#"{"pane": 4}"#).expect("a lenient read");
    assert_eq!(info.pane, 4);
    assert!(info.breakdown.is_none());
}

#[test]
fn the_request_reads_from_arguments() {
    let parsed: crate::control::Request =
        serde_json::from_str(r#"{"command": "context", "args": {"pane": 4, "cached": true}}"#).expect("parses");
    assert_eq!(parsed.command, Command::Context(ContextRead { pane: Some(4), tab: None, cached: true }));
}

#[test]
fn the_age_dates_the_read() {
    let clock = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let info = ContextInfo::default().read(Some(Duration::from_secs(60)), clock);
    assert_eq!(info.age, Some(60));
    assert!(info.read_at.is_some());
    assert_eq!(ContextInfo::default().read(None, clock).age, None);
}
