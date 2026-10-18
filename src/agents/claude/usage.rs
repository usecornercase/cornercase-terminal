use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::agents::usage::{Report, Window, window};
use crate::error::{Error, Result};
use crate::time;

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
pub(in crate::agents) const REQUESTS: &str = concat!(
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
    text.and_then(time::parse_time)
}

fn model(limit: &Limit) -> Option<&str> {
    limit.scope.as_ref().and_then(|s| s.model.as_ref()).and_then(|m| m.display_name.as_deref())
}

fn label(limit: &Limit) -> String {
    let model = model(limit);
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
            .map(|l| {
                window(
                    label(l),
                    l.kind.clone(),
                    model(l).map(str::to_string),
                    l.percent,
                    l.severity.as_deref(),
                    time(l.resets_at.as_deref()),
                )
            })
            .collect();
    }
    [("session (5h)", "session", &limits.five_hour), ("week", "weekly_all", &limits.seven_day)]
        .into_iter()
        .filter_map(|(name, kind, u)| {
            u.as_ref()
                .map(|u| window(name.into(), kind.into(), None, u.utilization, None, time(u.resets_at.as_deref())))
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

pub(super) fn parse(body: Value) -> Result<Report> {
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

pub(in crate::agents) fn answer(line: &str) -> Option<Result<Report>> {
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

pub(in crate::agents) fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(ARGS);
    for key in crate::agents::claude::CLAUDE_SESSION_ENV {
        cmd.env_remove(key);
    }
    cmd
}

pub(in crate::agents) fn requests() -> String {
    REQUESTS.to_string()
}

#[cfg(test)]
pub(crate) mod tests;
