use rstest::rstest;
use serde_json::json;

use super::*;
use crate::agents::usage::Severity;

pub(crate) const RECORDED: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"session":{"total_cost_usd":0},"subscription_type":"team","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":7,"resets_at":"2026-10-04T13:59:59.953605+00:00"},"seven_day":{"utilization":74,"resets_at":"2026-10-04T15:59:59.953630+00:00"},"extra_usage":{"is_enabled":false},"limits":[{"kind":"session","group":"session","percent":7,"severity":"normal","resets_at":"2026-10-04T13:59:59.953605+00:00","scope":null,"is_active":false},{"kind":"weekly_all","group":"weekly","percent":74,"severity":"normal","resets_at":"2026-10-04T15:59:59.953630+00:00","scope":null,"is_active":true},{"kind":"weekly_scoped","group":"weekly","percent":0,"severity":"normal","resets_at":"2026-10-04T16:00:00+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null},"is_active":false}],"spend":{"used":{"amount_minor":0,"currency":"USD","exponent":2},"limit":null,"percent":0,"severity":"normal","enabled":false}},"behaviors":{}}}}"#;

fn at(text: &str) -> Option<i64> {
    time::parse_time(text)
}

pub(crate) fn report(line: &str) -> Report {
    answer(line, &mut Heard::default()).expect("the usage answer").expect("a report")
}

#[test]
fn a_recorded_answer_gives_one_window_per_limit() {
    let expected = Report {
        plan: Some("team".into()),
        limited: true,
        windows: vec![
            Window {
                label: "session (5h)".into(),
                kind: "session".into(),
                scope: None,
                percent: 7,
                severity: Severity::Normal,
                resets_at: at("2026-10-04T13:59:59"),
            },
            Window {
                label: "week".into(),
                kind: "weekly_all".into(),
                scope: None,
                percent: 74,
                severity: Severity::Normal,
                resets_at: at("2026-10-04T15:59:59"),
            },
            Window {
                label: "week · Fable".into(),
                kind: "weekly_scoped".into(),
                scope: Some("Fable".into()),
                percent: 0,
                severity: Severity::Normal,
                resets_at: at("2026-10-04T16:00:00"),
            },
        ],
        extra: None,
        notice: None,
        identity: None,
    };
    assert_eq!(report(RECORDED), expected);
}

#[test]
fn an_account_without_rate_limits_has_no_windows() {
    let body = json!({"subscription_type": null, "rate_limits_available": false, "rate_limits": null});
    assert_eq!(parse(body).expect("a report"), Report::default());
}

#[rstest]
#[case::empty(json!({}), "unexpected answer: missing field `rate_limits_available`")]
#[case::available_but_missing(
    json!({"rate_limits_available": true, "rate_limits": null}),
    "unexpected answer: rate limits are available but missing"
)]
fn an_answer_without_the_usage_fields_is_unavailable(#[case] body: Value, #[case] expected: &str) {
    assert_eq!(parse(body).expect_err("no report").to_string(), expected);
}

#[test]
fn unknown_fields_and_kinds_are_tolerated() {
    let body = json!({
        "subscription_type": "max",
        "rate_limits_available": true,
        "something_new": [1, 2],
        "rate_limits": {
            "limits": [{"kind": "daily_burst", "percent": 12.6, "severity": "elevated", "brand_new": true}],
            "spend": {"enabled": true, "used": {"amount_minor": 1234, "currency": "USD", "exponent": 2},
                      "limit": {"amount_minor": 5000, "currency": "USD", "exponent": 2}}
        }
    });
    let expected = Report {
        plan: Some("max".into()),
        limited: true,
        windows: vec![Window {
            label: "daily burst".into(),
            kind: "daily_burst".into(),
            scope: None,
            percent: 13,
            severity: Severity::Normal,
            resets_at: None,
        }],
        extra: Some("extra usage: 12.34 USD of 50.00 USD".into()),
        notice: None,
        identity: None,
    };
    assert_eq!(parse(body).expect("a report"), expected);
}

#[test]
fn without_a_limits_list_the_five_hour_and_weekly_fields_are_used() {
    let body = json!({"rate_limits_available": true, "rate_limits": {
        "five_hour": {"utilization": 95.0, "resets_at": null},
        "seven_day": {"utilization": 80.0, "resets_at": "2026-10-04T16:00:00Z"}
    }});
    let found: Vec<(String, Severity)> =
        parse(body).expect("a report").windows.into_iter().map(|w| (w.label, w.severity)).collect();
    assert_eq!(found, [("session (5h)".into(), Severity::Critical), ("week".into(), Severity::Warning)]);
}

#[test]
fn the_fallback_fields_take_the_kinds_of_the_list() {
    let body = json!({"rate_limits_available": true, "rate_limits": {
        "five_hour": {"utilization": 5.0, "resets_at": null},
        "seven_day": {"utilization": 6.0, "resets_at": null}
    }});
    let found: Vec<(String, Option<String>)> =
        parse(body).expect("a report").windows.into_iter().map(|w| (w.kind, w.scope)).collect();
    assert_eq!(found, [("session".into(), None), ("weekly_all".into(), None)]);
}

#[rstest]
#[case::other_request(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"initialize","response":{}}}"#
)]
#[case::other_message(r#"{"type":"system","subtype":"init"}"#)]
#[case::not_json("warming up")]
fn other_lines_are_skipped(#[case] line: &str) {
    assert!(answer(line, &mut Heard::default()).is_none());
}

#[test]
fn an_error_response_carries_its_message() {
    let line =
        r#"{"type":"control_response","response":{"subtype":"error","request_id":"usage","error":"not logged in"}}"#;
    let error = answer(line, &mut Heard::default()).expect("the usage answer").expect_err("an error");
    assert_eq!(error.to_string(), "not logged in");
}

#[test]
fn the_usage_request_skips_the_transcript_scan() {
    let usage = REQUESTS
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a request"))
        .find(|request| request["request_id"] == USAGE_REQUEST)
        .expect("the usage request");
    assert_eq!(usage["request"], json!({"subtype": "get_usage", "skip_behaviors": true}));
}

#[test]
fn the_probe_drops_claude_session_variables() {
    let cmd = command("claude");
    let removed: Vec<_> = cmd.get_envs().filter(|(_, v)| v.is_none()).map(|(k, _)| k.to_owned()).collect();
    let mut expected: Vec<_> = crate::agents::claude::CLAUDE_SESSION_ENV.map(std::ffi::OsString::from).to_vec();
    expected.sort();
    assert_eq!(removed, expected);
}

mod the_identity {
    use super::*;

    const EMAIL: &str = "ana@acme.dev";

    fn initialize(account: &Value) -> String {
        json!({"type": "control_response", "response": {
            "subtype": "success",
            "request_id": "initialize",
            "response": {"commands": [], "account": account, "pid": 42}
        }})
        .to_string()
    }

    fn signed_in() -> Value {
        json!({"email": EMAIL, "organization": "Acme", "subscriptionType": "team", "apiProvider": "firstParty"})
    }

    fn after(lines: &[String]) -> Report {
        let mut heard = Heard::default();
        lines.iter().find_map(|line| answer(line, &mut heard)).expect("the usage answer").expect("a report")
    }

    #[test]
    fn comes_from_the_account_of_the_initialize_answer() {
        let report = after(&[initialize(&signed_in()), RECORDED.into()]);
        assert_eq!(report.identity, Some(fingerprint(Known::Claude, &[EMAIL, "Acme"])));
    }

    #[rstest]
    #[case::no_email("email")]
    #[case::no_organization("organization")]
    #[case::no_provider("apiProvider")]
    fn needs_every_part(#[case] missing: &str) {
        let mut account = signed_in();
        account.as_object_mut().expect("an object").remove(missing);
        assert_eq!(after(&[initialize(&account), RECORDED.into()]).identity, None);
    }

    #[rstest]
    #[case::bedrock("bedrock")]
    #[case::vertex("vertex")]
    #[case::gateway("gateway")]
    fn is_only_for_an_anthropic_login(#[case] provider: &str) {
        let mut account = signed_in();
        account["apiProvider"] = json!(provider);
        assert_eq!(after(&[initialize(&account), RECORDED.into()]).identity, None);
    }

    #[test]
    fn is_none_without_an_initialize_answer() {
        assert_eq!(after(&[RECORDED.into()]), report(RECORDED));
        assert_eq!(report(RECORDED).identity, None);
    }

    #[test]
    fn never_shows_the_email() {
        let report = after(&[initialize(&signed_in()), RECORDED.into()]);
        let shown = format!("{report:?}");
        assert!(report.identity.is_some());
        assert!(!shown.contains(EMAIL) && !shown.contains("Acme"), "{shown}");
    }
}
