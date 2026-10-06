use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use super::{Report, Window, window};
use crate::activity;
use crate::error::{Error, Result};
use crate::issues;

const ARGS: [&str; 9] = [
    "-p",
    "--setting-sources",
    "",
    "--no-session-persistence",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
];
const USAGE_REQUEST: &str = "usage";
pub const REQUESTS: &str = concat!(
    r#"{"type":"control_request","request_id":"initialize","request":{"subtype":"initialize"}}"#,
    "\n",
    r#"{"type":"control_request","request_id":"usage","request":{"subtype":"get_usage"}}"#,
    "\n",
);

#[derive(Deserialize)]
struct Body {
    subscription_type: Option<String>,
    rate_limits_available: bool,
    rate_limits: Option<Limits>,
}

#[derive(Deserialize)]
struct Limits {
    #[serde(default, rename = "limits")]
    list: Vec<Limit>,
    five_hour: Option<Utilization>,
    seven_day: Option<Utilization>,
    spend: Option<Spend>,
}

#[derive(Deserialize)]
struct Limit {
    kind: String,
    percent: Option<f64>,
    severity: Option<String>,
    resets_at: Option<String>,
    scope: Option<Scope>,
}

#[derive(Deserialize)]
struct Scope {
    model: Option<Model>,
}

#[derive(Deserialize)]
struct Model {
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct Utilization {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct Spend {
    #[serde(default)]
    enabled: bool,
    used: Option<Money>,
    limit: Option<Money>,
}

#[derive(Deserialize)]
struct Money {
    amount_minor: i64,
    currency: Option<String>,
    exponent: u32,
}

impl Money {
    fn text(&self) -> String {
        let scale = 10_i64.checked_pow(self.exponent).unwrap_or(1);
        let (units, minor) = (self.amount_minor / scale, (self.amount_minor % scale).abs());
        let amount = if self.exponent == 0 {
            units.to_string()
        } else {
            format!("{units}.{minor:0width$}", width = self.exponent as usize)
        };
        match &self.currency {
            Some(currency) => format!("{amount} {currency}"),
            None => amount,
        }
    }
}

fn time(text: Option<&str>) -> Option<i64> {
    text.and_then(issues::parse_time)
}

fn label(limit: &Limit) -> String {
    let model = limit.scope.as_ref().and_then(|s| s.model.as_ref()).and_then(|m| m.display_name.as_deref());
    match (limit.kind.as_str(), model) {
        ("session", _) => "session (5h)".into(),
        ("weekly_all", _) => "week".into(),
        ("weekly_scoped", Some(model)) => format!("week · {model}"),
        (kind, Some(model)) => format!("{} · {model}", kind.replace('_', " ")),
        (kind, None) => kind.replace('_', " "),
    }
}

fn windows(limits: &Limits) -> Vec<Window> {
    if !limits.list.is_empty() {
        return limits
            .list
            .iter()
            .map(|l| window(label(l), l.percent, l.severity.as_deref(), time(l.resets_at.as_deref())))
            .collect();
    }
    [("session (5h)", &limits.five_hour), ("week", &limits.seven_day)]
        .into_iter()
        .filter_map(|(name, u)| {
            u.as_ref().map(|u| window(name.into(), u.utilization, None, time(u.resets_at.as_deref())))
        })
        .collect()
}

fn extra(spend: &Spend) -> Option<String> {
    let used = spend.used.as_ref().filter(|_| spend.enabled)?.text();
    Some(match &spend.limit {
        Some(limit) => format!("extra usage: {used} of {}", limit.text()),
        None => format!("extra usage: {used}"),
    })
}

pub fn parse(body: Value) -> Result<Report> {
    let unexpected = |e: &dyn std::fmt::Display| Error::Usage(format!("unexpected answer: {e}"));
    let body: Body = serde_json::from_value(body).map_err(|e| unexpected(&e))?;
    let limits = match (body.rate_limits_available, body.rate_limits) {
        (false, _) => None,
        (true, Some(limits)) => Some(limits),
        (true, None) => return Err(unexpected(&"rate limits are available but missing")),
    };
    Ok(Report {
        plan: body.subscription_type,
        limited: limits.is_some(),
        windows: limits.as_ref().map(windows).unwrap_or_default(),
        extra: limits.as_ref().and_then(|l| l.spend.as_ref()).and_then(extra),
        notice: None,
    })
}

pub fn answer(line: &str) -> Option<Result<Report>> {
    let message: Value = serde_json::from_str(line).ok()?;
    let response = message.get("response").filter(|_| message["type"] == "control_response")?;
    if response["request_id"] != USAGE_REQUEST {
        return None;
    }
    if response["subtype"] == "success" {
        return Some(parse(response["response"].clone()));
    }
    let error = response["error"].as_str().unwrap_or("claude refused the request");
    Some(Err(Error::Usage(error.to_string())))
}

pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(ARGS);
    for key in activity::CLAUDE_SESSION_ENV {
        cmd.env_remove(key);
    }
    cmd
}

#[cfg(test)]
pub mod tests {
    use rstest::rstest;
    use serde_json::json;

    use super::super::Severity;
    use super::*;

    pub const RECORDED: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"session":{"total_cost_usd":0},"subscription_type":"team","rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":7,"resets_at":"2026-10-04T13:59:59.953605+00:00"},"seven_day":{"utilization":74,"resets_at":"2026-10-04T15:59:59.953630+00:00"},"extra_usage":{"is_enabled":false},"limits":[{"kind":"session","group":"session","percent":7,"severity":"normal","resets_at":"2026-10-04T13:59:59.953605+00:00","scope":null,"is_active":false},{"kind":"weekly_all","group":"weekly","percent":74,"severity":"normal","resets_at":"2026-10-04T15:59:59.953630+00:00","scope":null,"is_active":true},{"kind":"weekly_scoped","group":"weekly","percent":0,"severity":"normal","resets_at":"2026-10-04T16:00:00+00:00","scope":{"model":{"id":null,"display_name":"Fable"},"surface":null},"is_active":false}],"spend":{"used":{"amount_minor":0,"currency":"USD","exponent":2},"limit":null,"percent":0,"severity":"normal","enabled":false}},"behaviors":{}}}}"#;

    fn at(text: &str) -> Option<i64> {
        issues::parse_time(text)
    }

    pub fn report(line: &str) -> Report {
        answer(line).expect("the usage answer").expect("a report")
    }

    #[test]
    fn a_recorded_answer_gives_one_window_per_limit() {
        let expected = Report {
            plan: Some("team".into()),
            limited: true,
            windows: vec![
                Window {
                    label: "session (5h)".into(),
                    percent: 7,
                    severity: Severity::Normal,
                    resets_at: at("2026-10-04T13:59:59"),
                },
                Window {
                    label: "week".into(),
                    percent: 74,
                    severity: Severity::Normal,
                    resets_at: at("2026-10-04T15:59:59"),
                },
                Window {
                    label: "week · Fable".into(),
                    percent: 0,
                    severity: Severity::Normal,
                    resets_at: at("2026-10-04T16:00:00"),
                },
            ],
            extra: None,
            notice: None,
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
                percent: 13,
                severity: Severity::Normal,
                resets_at: None,
            }],
            extra: Some("extra usage: 12.34 USD of 50.00 USD".into()),
            notice: None,
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

    #[rstest]
    #[case::other_request(
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"initialize","response":{}}}"#
    )]
    #[case::other_message(r#"{"type":"system","subtype":"init"}"#)]
    #[case::not_json("warming up")]
    fn other_lines_are_skipped(#[case] line: &str) {
        assert!(answer(line).is_none());
    }

    #[test]
    fn an_error_response_carries_its_message() {
        let line = r#"{"type":"control_response","response":{"subtype":"error","request_id":"usage","error":"not logged in"}}"#;
        let error = answer(line).expect("the usage answer").expect_err("an error");
        assert_eq!(error.to_string(), "not logged in");
    }

    #[test]
    fn the_probe_drops_claude_session_variables() {
        let cmd = command("claude");
        let removed: Vec<_> = cmd.get_envs().filter(|(_, v)| v.is_none()).map(|(k, _)| k.to_owned()).collect();
        let mut expected: Vec<_> = activity::CLAUDE_SESSION_ENV.map(std::ffi::OsString::from).to_vec();
        expected.sort();
        assert_eq!(removed, expected);
    }
}
