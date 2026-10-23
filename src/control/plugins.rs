use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Fields, Ids, Item};
use crate::plugins::manifest::{Action, Process};
use crate::plugins::run::{Ended, Ran};
use crate::plugins::supervise::{Seen, restart_in};
use crate::plugins::{Context, Places, Plugin};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PluginList {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PluginActions {
    pub(crate) context: Option<Context>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PluginRun {
    pub(crate) plugin: String,
    pub(crate) action: String,
    pub(crate) revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) item: Option<Item>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PluginPopupClose {}

impl PluginActions {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("context", self.context.map_or("all", Context::name));
    }
}

impl PluginRun {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("plugin", &self.plugin).add("action", &self.action);
        fields.add("revision", self.revision).maybe("item", self.item);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PluginsInfo {
    pub(crate) revision: Option<u32>,
    pub(crate) problem: Option<String>,
    pub(crate) plugins: Vec<PluginInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PluginInfo {
    pub(crate) id: String,
    pub(crate) enabled: bool,
    pub(crate) state: String,
    pub(crate) error: Option<String>,
    pub(crate) label: String,
    pub(crate) note: String,
    pub(crate) source: Value,
    pub(crate) log: PathBuf,
    pub(crate) manifest: Option<Value>,
    pub(crate) services: Vec<ServiceInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) button: Option<ButtonInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ButtonInfo {
    pub(crate) title: String,
    pub(crate) icon: String,
    pub(crate) actions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ServiceInfo {
    pub(crate) id: String,
    pub(crate) state: String,
    pub(crate) pid: Option<u32>,
    pub(crate) restart_in: Option<u64>,
}

impl ServiceInfo {
    pub(crate) fn of(declared: &[Process], seen: &[(String, Seen)], now: Instant) -> Vec<Self> {
        declared
            .iter()
            .map(|service| {
                let found = seen.iter().find(|(entry, _)| *entry == service.id).map(|(_, seen)| *seen);
                let (state, pid, restart) = match found {
                    Some(Seen::Running { pid }) => ("running", Some(pid), None),
                    Some(Seen::Stopped { until }) => ("stopped", None, Some(restart_in(until, now))),
                    None => ("starting", None, None),
                };
                Self { id: service.id.clone(), state: state.into(), pid, restart_in: restart }
            })
            .collect()
    }
}

impl PluginInfo {
    pub(crate) fn of(plugin: &Plugin, places: &Places, running: Option<&str>, services: Vec<ServiceInfo>) -> Self {
        let (manifest, error) = match &plugin.manifest {
            Ok(manifest) => (serde_json::to_value(manifest).ok(), None),
            Err(e) => (None, Some(e.clone())),
        };
        let button = plugin.manifest.as_ref().ok().and_then(|manifest| {
            let face = manifest.face()?;
            let actions = manifest.button_actions().map(|action| action.id.clone()).collect();
            Some(ButtonInfo { title: face.title, icon: face.icon, actions })
        });
        let state = match (&error, plugin.entry.enabled) {
            (Some(_), _) => "cannot-run",
            (None, true) => "on",
            (None, false) => "off",
        };
        Self {
            id: plugin.entry.id.clone(),
            enabled: plugin.entry.enabled,
            state: state.into(),
            error,
            label: plugin.label(),
            note: plugin.note(running),
            source: serde_json::to_value(&plugin.entry.source).unwrap_or_default(),
            log: places.log(&plugin.entry.id),
            manifest,
            services,
            button,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ActionsInfo {
    pub(crate) revision: u32,
    pub(crate) actions: Vec<ActionInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ActionInfo {
    pub(crate) plugin: String,
    pub(crate) name: String,
    pub(crate) action: String,
    pub(crate) title: String,
    pub(crate) contexts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) placement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<u16>,
}

impl ActionInfo {
    pub(crate) fn of(plugin: &Plugin, action: &Action) -> Self {
        let placement = serde_json::to_value(action.placement).ok().and_then(|v| v.as_str().map(str::to_string));
        Self {
            plugin: plugin.entry.id.clone(),
            name: plugin.name(),
            action: action.id.clone(),
            title: action.title.clone(),
            contexts: action.contexts.iter().map(|c| c.name().to_string()).collect(),
            placement,
            width: action.width,
            height: action.height,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ActionRan {
    pub(crate) plugin: String,
    pub(crate) action: String,
    pub(crate) context: String,
    #[serde(flatten)]
    pub(crate) ids: Ids,
    pub(crate) ended: String,
    pub(crate) code: Option<i32>,
    pub(crate) signal: Option<i32>,
    pub(crate) error: Option<String>,
    pub(crate) ms: u64,
    pub(crate) log: PathBuf,
}

#[derive(Clone, Copy)]
pub(crate) struct Invoked<'a> {
    pub(crate) plugin: &'a str,
    pub(crate) action: &'a str,
    pub(crate) context: Context,
    pub(crate) ids: Ids,
}

impl ActionRan {
    pub(crate) fn of(invoked: Invoked, ran: &Ran, log: PathBuf) -> Self {
        use std::os::unix::process::ExitStatusExt as _;
        let Invoked { plugin, action, context, ids } = invoked;
        let (ended, code, signal) = match &ran.ended {
            Ended::Status(status) => match status.signal() {
                Some(signal) => ("signal", None, Some(signal)),
                None => ("exited", status.code(), None),
            },
            Ended::Timeout(_) => ("timeout", None, None),
            Ended::Failed(_) => ("error", None, None),
        };
        Self {
            plugin: plugin.into(),
            action: action.into(),
            context: context.name().into(),
            ids,
            ended: ended.into(),
            code,
            signal,
            error: ran.failure(),
            ms: u64::try_from(ran.took.as_millis()).unwrap_or(u64::MAX),
            log,
        }
    }
}

#[cfg(test)]
mod tests;
