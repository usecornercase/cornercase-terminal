use super::*;

#[test]
fn an_ask_round_trips_with_every_field_present() {
    let ask = RecapAsk { range: "yesterday".into(), fresh: true, agent: Some("codex".into()), ..RecapAsk::default() };
    let value = serde_json::to_value(&ask).expect("json");
    for key in ["range", "fresh", "dossier", "agent", "account", "model", "effort", "language", "zone"] {
        assert!(value.get(key).is_some(), "{key}");
    }
    assert_eq!(serde_json::from_value::<RecapAsk>(value).expect("back"), ask);
}

#[test]
fn the_answer_always_carries_every_key() {
    let value = serde_json::to_value(RecapInfo::default()).expect("json");
    for key in [
        "range",
        "day",
        "label",
        "from",
        "to",
        "zone",
        "written_at",
        "age",
        "agent",
        "title",
        "account",
        "model",
        "effort",
        "language",
        "tokens",
        "conversations",
        "text",
        "places",
        "dossier",
        "loading",
        "progress",
        "empty",
        "error",
        "outdated",
        "changed",
    ] {
        assert!(value.get(key).is_some(), "{key}");
    }
}

#[test]
fn an_override_is_told_from_the_defaults() {
    assert!(!RecapAsk::default().overridden());
    assert!(RecapAsk { model: Some("opus".into()), ..RecapAsk::default() }.overridden());
}
