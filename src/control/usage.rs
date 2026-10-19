use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::agents::Known;
use crate::agents::usage::{Probe, Window};
use crate::time::Stamp;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Usage {
    pub(crate) cached: bool,
}

impl Usage {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("cached", self.cached);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Plans {
    pub(crate) agents: Vec<PlanInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PlanInfo {
    pub(crate) agent: String,
    pub(crate) title: String,
    pub(crate) loading: bool,
    pub(crate) read_at: Option<String>,
    pub(crate) age: Option<u64>,
    pub(crate) plan: Option<String>,
    pub(crate) limited: Option<bool>,
    pub(crate) limits: Vec<LimitInfo>,
    pub(crate) extra: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LimitInfo {
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) scope: Option<String>,
    pub(crate) percent: u16,
    pub(crate) severity: String,
    pub(crate) resets_at: Option<String>,
}

impl PlanInfo {
    pub(crate) fn of(agent: Known, probe: &Probe, now: Instant, clock: SystemTime) -> Self {
        let last = probe.last();
        let report = last.map(|(report, _)| report);
        let age = last.map(|(_, at)| now.saturating_duration_since(*at));
        Self {
            agent: agent.name().into(),
            title: agent.title().into(),
            loading: probe.loading(),
            read_at: age.and_then(|age| clock.checked_sub(age)).map(|at| Stamp(at).to_string()),
            age: age.map(|age| age.as_secs()),
            plan: report.and_then(|r| r.plan.clone()),
            limited: report.map(|r| r.limited),
            limits: report.map(|r| r.windows.iter().map(LimitInfo::from).collect()).unwrap_or_default(),
            extra: report.and_then(|r| r.extra.clone()),
            notice: report.and_then(|r| r.notice_for(agent)).map(str::to_string),
            error: probe.error().map(str::to_string),
            account: None,
            label: None,
            active: false,
        }
    }

    #[must_use]
    pub(crate) fn for_account(self, account: Option<&str>, label: Option<String>, active: bool) -> Self {
        Self { account: account.map(str::to_string), label, active, ..self }
    }

    pub(crate) fn heading(&self) -> String {
        let shown = self.label.as_deref().or(self.account.as_deref());
        crate::agents::usage::heading(&self.title, shown, self.active)
    }
}

impl From<&Window> for LimitInfo {
    fn from(window: &Window) -> Self {
        Self {
            label: window.label.clone(),
            kind: window.kind.clone(),
            scope: window.scope.clone(),
            percent: window.percent,
            severity: window.severity.name().into(),
            resets_at: window.resets_at.and_then(utc),
        }
    }
}

fn utc(secs: i64) -> Option<String> {
    let secs = u64::try_from(secs).ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs)).map(|at| Stamp(at).to_string())
}

#[cfg(test)]
mod tests;
