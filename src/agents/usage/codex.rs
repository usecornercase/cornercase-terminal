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

pub(super) fn requests() -> String {
    let client = json!({"name": "cornercase", "version": crate::version::CURRENT});
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

pub(super) fn parse(result: Value) -> Result<Report> {
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

pub(super) fn answer(line: &str) -> Option<Result<Report>> {
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

pub(super) fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(ARGS);
    cmd
}

#[cfg(test)]
pub(crate) mod tests;
