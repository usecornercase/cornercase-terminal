use std::collections::BTreeMap;
use std::process::Command;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Report, Window, window};
use crate::error::{Error, Result};

const ARGS: [&str; 1] = ["app-server"];
const INITIALIZE: u64 = 1;
const USAGE_REQUEST: u64 = 2;
const DEFAULT_LIMIT: &str = "codex";
const UNKNOWN_PLAN: &str = "unknown";
const SIGNED_OUT: &str = "authentication required";
const SIGN_IN: &str = "not signed in · run codex login";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    rate_limits: Snapshot,
    rate_limits_by_limit_id: Option<BTreeMap<String, Snapshot>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    primary: Option<Limit>,
    secondary: Option<Limit>,
    credits: Option<Credits>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limit {
    used_percent: f64,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credits {
    #[serde(rename = "hasCredits")]
    available: bool,
    unlimited: bool,
    balance: Option<String>,
}

pub fn requests() -> String {
    let client = json!({"name": "cornercase", "version": env!("CARGO_PKG_VERSION")});
    let initialize = json!({"id": INITIALIZE, "method": "initialize", "params": {"clientInfo": client}});
    let initialized = json!({"method": "initialized"});
    let read = json!({"id": USAGE_REQUEST, "method": "account/rateLimits/read", "params": {"excludeResetCreditDetails": true}});
    format!("{initialize}\n{initialized}\n{read}\n")
}

fn duration(minutes: Option<i64>, slot: &str) -> String {
    match minutes {
        Some(300) => "session (5h)".into(),
        Some(10_080) => "week".into(),
        Some(m) if m > 0 && m % 1_440 == 0 => format!("{}d", m / 1_440),
        Some(m) if m > 0 && m % 60 == 0 => format!("{}h", m / 60),
        Some(m) if m > 0 => format!("{m}m"),
        _ => slot.into(),
    }
}

fn snapshot_windows(id: &str, snapshot: &Snapshot) -> impl Iterator<Item = Window> {
    let scope = (id != DEFAULT_LIMIT).then(|| snapshot.limit_name.clone().unwrap_or_else(|| id.to_string()));
    [("primary", &snapshot.primary), ("secondary", &snapshot.secondary)].into_iter().filter_map(move |(slot, limit)| {
        let limit = limit.as_ref()?;
        let label = duration(limit.window_duration_mins, slot);
        let label = scope.as_ref().map_or_else(|| label.clone(), |scope| format!("{label} · {scope}"));
        Some(window(label, Some(limit.used_percent), None, limit.resets_at))
    })
}

fn windows(body: &Body) -> Vec<Window> {
    let mut buckets: Vec<(&str, &Snapshot)> = match &body.rate_limits_by_limit_id {
        Some(buckets) if !buckets.is_empty() => buckets.iter().map(|(id, s)| (id.as_str(), s)).collect(),
        _ => vec![(body.rate_limits.limit_id.as_deref().unwrap_or(DEFAULT_LIMIT), &body.rate_limits)],
    };
    buckets.sort_by_key(|(id, _)| *id != DEFAULT_LIMIT);
    buckets.into_iter().flat_map(|(id, snapshot)| snapshot_windows(id, snapshot)).collect()
}

fn credits(credits: &Credits) -> Option<String> {
    if credits.unlimited {
        return Some("credits: unlimited".into());
    }
    credits.balance.as_ref().filter(|_| credits.available).map(|balance| format!("credits: {balance}"))
}

pub fn parse(result: Value) -> Result<Report> {
    let body: Body = serde_json::from_value(result).map_err(|e| Error::Usage(format!("unexpected answer: {e}")))?;
    let windows = windows(&body);
    Ok(Report {
        plan: body.rate_limits.plan_type.clone().filter(|plan| plan != UNKNOWN_PLAN),
        limited: !windows.is_empty(),
        windows,
        extra: body.rate_limits.credits.as_ref().and_then(credits),
        notice: None,
    })
}

pub fn answer(line: &str) -> Option<Result<Report>> {
    let message: Value = serde_json::from_str(line).ok()?;
    if message.get("method").is_some() {
        return None;
    }
    let id = message.get("id")?.as_u64()?;
    if let Some(error) = message.get("error") {
        let text = error["message"].as_str().unwrap_or("codex refused the request");
        if id == USAGE_REQUEST && text.contains(SIGNED_OUT) {
            return Some(Ok(Report { notice: Some(SIGN_IN), ..Report::default() }));
        }
        return Some(Err(Error::Usage(text.to_string())));
    }
    (id == USAGE_REQUEST).then(|| parse(message["result"].clone()))
}

pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(ARGS);
    cmd
}

#[cfg(test)]
pub mod tests {
    use rstest::rstest;

    use super::super::Severity;
    use super::*;

    pub const RECORDED: &str = r#"{"id":2,"result":{"ordinaryUsageAllowed":true,"rateLimits":{"limitId":"codex","limitName":null,"normalModelSlug":null,"primary":{"usedPercent":53,"windowDurationMins":300,"resetsAt":1791218970},"secondary":{"usedPercent":8,"windowDurationMins":10080,"resetsAt":1791805770},"credits":{"hasCredits":false,"unlimited":false,"balance":"0"},"individualLimit":null,"spendControlReached":false,"planType":"plus","rateLimitReachedType":null},"rateLimitsByLimitId":{"codex":{"limitId":"codex","limitName":null,"normalModelSlug":null,"primary":{"usedPercent":53,"windowDurationMins":300,"resetsAt":1791218970},"secondary":{"usedPercent":8,"windowDurationMins":10080,"resetsAt":1791805770},"credits":{"hasCredits":false,"unlimited":false,"balance":"0"},"individualLimit":null,"spendControlReached":false,"planType":"plus","rateLimitReachedType":null}},"rateLimitResetCredits":{"availableCount":2,"credits":null},"accountId":"00000000-0000-0000-0000-000000000000","rateLimitUpsell":null}}"#;

    pub const SIGNED_OUT: &str =
        r#"{"error":{"code":-32600,"message":"codex account authentication required to read rate limits"},"id":2}"#;

    pub fn report(line: &str) -> Report {
        answer(line).expect("the usage answer").expect("a report")
    }

    fn limit(percent: u8, minutes: Option<i64>) -> Value {
        json!({"usedPercent": percent, "windowDurationMins": minutes, "resetsAt": null})
    }

    fn labels(result: &Value) -> Vec<(String, u16)> {
        parse(result.clone()).expect("a report").windows.into_iter().map(|w| (w.label, w.percent)).collect()
    }

    #[test]
    fn a_recorded_answer_gives_the_session_and_the_week() {
        let expected = Report {
            plan: Some("plus".into()),
            limited: true,
            windows: vec![
                Window {
                    label: "session (5h)".into(),
                    percent: 53,
                    severity: Severity::Normal,
                    resets_at: Some(1_791_218_970),
                },
                Window { label: "week".into(), percent: 8, severity: Severity::Normal, resets_at: Some(1_791_805_770) },
            ],
            extra: None,
            notice: None,
        };
        assert_eq!(report(RECORDED), expected);
    }

    #[test]
    fn the_requests_initialize_then_read_the_rate_limits() {
        let methods: Vec<Value> = requests()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("one request per line")["method"].clone())
            .collect();
        assert_eq!(methods, ["initialize", "initialized", "account/rateLimits/read"]);
    }

    #[rstest]
    #[case::day(Some(1_440), "1d")]
    #[case::hours(Some(120), "2h")]
    #[case::minutes(Some(90), "90m")]
    #[case::unknown(None, "primary")]
    #[case::zero(Some(0), "primary")]
    fn other_windows_are_named_by_their_length(#[case] minutes: Option<i64>, #[case] expected: &str) {
        assert_eq!(duration(minutes, "primary"), expected);
    }

    #[test]
    fn other_buckets_follow_the_codex_one_with_their_name() {
        let result = json!({
            "rateLimits": {"limitId": "codex", "primary": limit(10, Some(300))},
            "rateLimitsByLimitId": {
                "a_model": {"limitId": "a_model", "limitName": "Spark", "primary": limit(40, Some(300))},
                "codex": {"limitId": "codex", "primary": limit(10, Some(300)), "secondary": limit(20, Some(10_080))},
                "b_model": {"limitId": "b_model", "secondary": limit(30, Some(10_080))}
            }
        });
        assert_eq!(
            labels(&result),
            [
                ("session (5h)".into(), 10),
                ("week".into(), 20),
                ("session (5h) · Spark".into(), 40),
                ("week · b_model".into(), 30),
            ]
        );
    }

    #[test]
    fn without_buckets_the_single_snapshot_is_used() {
        let result = json!({"rateLimits": {"primary": limit(97, Some(300)), "planType": "pro", "newField": 1}});
        let report = parse(result).expect("a report");
        assert_eq!(
            (report.plan.as_deref(), report.windows[0].severity, report.windows.len()),
            (Some("pro"), Severity::Critical, 1)
        );
    }

    #[test]
    fn an_account_without_windows_has_no_plan_limits() {
        let report = parse(json!({"rateLimits": {"planType": "unknown"}})).expect("a report");
        assert_eq!(report, Report::default());
    }

    #[rstest]
    #[case::unlimited(json!({"hasCredits": true, "unlimited": true, "balance": null}), Some("credits: unlimited"))]
    #[case::balance(json!({"hasCredits": true, "unlimited": false, "balance": "120"}), Some("credits: 120"))]
    #[case::none(json!({"hasCredits": false, "unlimited": false, "balance": "0"}), None)]
    fn credits_show_under_the_windows(#[case] credits: Value, #[case] expected: Option<&str>) {
        let report = parse(json!({"rateLimits": {"credits": credits}})).expect("a report");
        assert_eq!(report.extra.as_deref(), expected);
    }

    #[test]
    fn an_answer_without_rate_limits_is_unavailable() {
        let error = parse(json!({})).expect_err("no report");
        assert_eq!(error.to_string(), "unexpected answer: missing field `rateLimits`");
    }

    #[rstest]
    #[case::initialize(r#"{"id":1,"result":{"userAgent":"x"}}"#)]
    #[case::notification(r#"{"method":"account/updated","params":{"planType":"plus"}}"#)]
    #[case::server_request(r#"{"id":2,"method":"item/tool/requestUserInput","params":{}}"#)]
    #[case::not_json("starting")]
    fn other_lines_are_skipped(#[case] line: &str) {
        assert!(answer(line).is_none());
    }

    #[test]
    fn a_codex_without_a_login_is_signed_out_not_failed() {
        assert_eq!(report(SIGNED_OUT), Report { notice: Some(SIGN_IN), ..Report::default() });
    }

    #[rstest]
    #[case::usage(r#"{"error":{"code":-32603,"message":"backend unavailable"},"id":2}"#, "backend unavailable")]
    #[case::initialize(r#"{"error":{"code":-32603},"id":1}"#, "codex refused the request")]
    fn an_error_carries_its_message(#[case] line: &str, #[case] expected: &str) {
        let error = answer(line).expect("an answer").expect_err("an error");
        assert_eq!(error.to_string(), expected);
    }
}
