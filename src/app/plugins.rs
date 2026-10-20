use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::App;
use super::event::AppEvent;
use super::menus::MenuAction;
use super::overlay::Overlay;
use crate::control;
use crate::control::plugins::{ActionInfo, ActionRan, ActionsInfo, PluginInfo, PluginsInfo, ServiceInfo};
use crate::error::Error;
use crate::log::{self, Job, Level};
use crate::plugins::manifest::Action;
use crate::plugins::registry::missing;
use crate::plugins::{Context, Kind, Places, Plugin, Registry, Supervisor, Watch, run};
use crate::project::Hierarchy;
use crate::protocol;
use crate::settings::PluginRow;
use crate::ui;

const CHECK_EVERY: Duration = Duration::from_secs(1);
const HOOK_WITHIN: Duration = Duration::from_secs(60);
const ACTION_WITHIN: Duration = Duration::from_secs(300);
const MAX_HOOKS: usize = 16;
pub(crate) const PROJECT_ENV: &str = "CORNERCASE_PROJECT";
pub(crate) const WORKSPACE_ENV: &str = "CORNERCASE_WORKSPACE";
pub(crate) const TAB_ENV: &str = "CORNERCASE_TAB";
pub(crate) const ACTION_ENV: &str = "CORNERCASE_PLUGIN_ACTION";
pub(crate) const CONTEXT_ENV: &str = "CORNERCASE_CONTEXT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Spot {
    Project(u64),
    Workspace(u64, u64),
    Tab(u64, u64, u64),
    Pane(u64),
}

impl Spot {
    pub(super) fn context(self) -> Context {
        match self {
            Self::Project(_) => Context::Project,
            Self::Workspace(..) => Context::Workspace,
            Self::Tab(..) => Context::Tab,
            Self::Pane(_) => Context::Pane,
        }
    }
}

#[derive(Debug)]
pub(crate) enum PluginEvent {
    Failed { plugin: String, title: String, reason: String },
    Ran { request: u64, ran: Box<Result<ActionRan, String>> },
}

impl PluginEvent {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Failed { .. } | Self::Ran { .. } => "plugin action",
        }
    }
}

pub(super) struct Plugins {
    places: Places,
    watch: Watch,
    loaded: Vec<Plugin>,
    revision: u32,
    checked: Option<Instant>,
    services: Option<Supervisor>,
    hooks: Arc<AtomicUsize>,
}

impl Plugins {
    pub(super) fn new(places: Places) -> Self {
        Self {
            watch: Watch::new(places.registry.clone()),
            places,
            loaded: Vec::new(),
            revision: 0,
            checked: None,
            services: None,
            hooks: Arc::default(),
        }
    }

    pub(super) fn hooked(&self) -> bool {
        hooked(&self.loaded)
    }

    pub(super) fn actions(&self) -> impl Iterator<Item = (&Plugin, &Action)> {
        self.loaded
            .iter()
            .filter_map(|p| p.runs().map(|m| (p, m)))
            .flat_map(|(p, m)| m.actions.iter().map(move |a| (p, a)))
    }

    fn changed(&mut self) -> Option<Vec<Plugin>> {
        self.watch.changed().filter(|loaded| *loaded != self.loaded)
    }
}

fn hooked(plugins: &[Plugin]) -> bool {
    plugins.iter().filter_map(Plugin::runs).any(|manifest| !manifest.events.is_empty())
}

fn server_env() -> Vec<(String, String)> {
    vec![(protocol::NESTED_ENV.to_string(), "1".to_string())]
}

fn action_env(ids: &control::Ids, action: &Action, context: Context) -> Vec<(String, String)> {
    let mut env = server_env();
    let pairs = [(PROJECT_ENV, ids.project), (WORKSPACE_ENV, ids.workspace), (TAB_ENV, ids.tab)];
    env.extend(pairs.into_iter().filter_map(|(key, id)| id.map(|id| (key.to_string(), id.to_string()))));
    if let Some(pane) = ids.pane {
        env.push((control::PANE_ENV.to_string(), pane.to_string()));
        env.push((control::SERVER_ENV.to_string(), control::server_token().to_string()));
    }
    env.push((ACTION_ENV.into(), action.id.clone()));
    env.push((CONTEXT_ENV.into(), context.name().into()));
    env
}

impl App {
    pub(crate) fn set_plugin_places(&mut self, places: Places) {
        self.plugins = Plugins::new(places);
    }

    pub(crate) fn start_plugins(&mut self) {
        if self.plugins.services.is_none() {
            let places = self.plugins.places.clone();
            self.plugins.services = Some(Supervisor::start(Kind::Service, places, server_env()));
        }
    }

    pub(crate) fn stop_plugins(&mut self) {
        self.plugins.services = None;
    }

    pub(super) fn following(&self) -> bool {
        self.events.listening() || self.plugins.hooked()
    }

    pub(super) fn watch_plugins(&mut self, now: Instant) {
        if self.plugins.checked.is_some_and(|at| now.saturating_duration_since(at) < CHECK_EVERY) {
            return;
        }
        self.plugins.checked = Some(now);
        self.reload_plugins();
    }

    pub(super) fn reload_plugins(&mut self) {
        let Some(loaded) = self.plugins.changed() else { return };
        if hooked(&loaded) && !self.following() {
            self.publish(log::enabled(Level::Info));
        }
        let on = loaded.iter().filter(|p| p.runs().is_some()).count();
        log::info!("plugins", "loaded", plugins = loaded.len(), enabled = on);
        self.plugins.loaded = loaded;
        self.plugins.revision = self.plugins.revision.wrapping_add(1);
    }

    pub(super) fn plugin_menu(&self, spot: Spot) -> Vec<MenuAction> {
        let revision = self.plugins.revision;
        self.plugins
            .actions()
            .enumerate()
            .filter(|(_, (_, action))| action.contexts.contains(&spot.context()))
            .filter_map(|(i, _)| u16::try_from(i).ok())
            .map(|index| MenuAction::Plugin { spot, index, revision })
            .collect()
    }

    pub(super) fn plugin_label(&self, index: u16) -> String {
        self.plugins.actions().nth(usize::from(index)).map(|(_, action)| action.title.clone()).unwrap_or_default()
    }

    pub(super) fn spot_env(&self, spot: Spot) -> Option<(control::Ids, PathBuf)> {
        let (p, w, t, pane) = match spot {
            Spot::Project(project) => {
                let p = self.projects.project_index(project)?;
                return Some((
                    control::Ids { project: Some(project), ..control::Ids::default() },
                    self.projects[p].path.clone(),
                ));
            }
            Spot::Workspace(project, workspace) => {
                let (p, w) = self.projects.workspace_in(project, workspace)?;
                (p, w, None, None)
            }
            Spot::Tab(project, workspace, tab) => {
                let (p, w, t) = self.projects.tab_in(project, workspace, tab)?;
                (p, w, Some(t), None)
            }
            Spot::Pane(pane) => {
                let (p, w, t) = self.projects.tab_of_pane(pane)?;
                (p, w, Some(t), Some(pane))
            }
        };
        let workspace = &self.projects[p].workspaces[w];
        let ids = control::Ids {
            project: Some(self.projects[p].id),
            workspace: Some(workspace.id),
            tab: t.map(|t| workspace.tabs[t].id),
            pane,
        };
        Some((ids, workspace.path.clone()))
    }

    pub(super) fn run_plugin_action(&self, spot: Spot, index: u16, revision: u32) {
        if revision != self.plugins.revision {
            return;
        }
        let Some((plugin, action)) = self.plugins.actions().nth(usize::from(index)) else { return };
        let (plugin, action) = (plugin.clone(), action.clone());
        self.start_plugin_action(spot, plugin, action, None);
    }

    pub(super) fn start_plugin_action(&self, spot: Spot, plugin: Plugin, action: Action, request: Option<u64>) -> bool {
        let Some((ids, cwd)) = self.spot_env(spot) else { return false };
        let context = spot.context();
        let env = action_env(&ids, &action, context);
        let places = self.plugins.places.clone();
        let job =
            Job::new(Level::Info, "plugins", "action").with("plugin", &plugin.entry.id).with("action", &action.id);
        let (name, title, id) = (plugin.name(), action.title.clone(), plugin.entry.id.clone());
        self.spawn_job(
            job,
            move || {
                places.note(&plugin.entry.id, "action started", &[("action", &action.id)]);
                let invocation = run::Invocation { argv: &action.command, env: &env, cwd: Some(&cwd) };
                let ran = run::run_to_end(&places, &plugin, invocation, ACTION_WITHIN);
                if let Some(e) = ran.failure() {
                    places.note(&plugin.entry.id, "action failed", &[("action", &action.id), ("error", &e)]);
                }
                let log = places.log(&plugin.entry.id);
                Ok((ActionRan::of(&plugin.entry.id, &action.id, context, ids, &ran, log), ran.failure()))
            },
            move |result, job| {
                let failure = result.as_ref().ok().and_then(|(_, failure)| failure.clone());
                if let Some(e) = &failure {
                    job.fail(e);
                }
                if let Some(request) = request {
                    let ran = result.map(|(ran, _)| ran).map_err(|e| e.to_string());
                    return Some(AppEvent::Plugin(PluginEvent::Ran { request, ran: Box::new(ran) }));
                }
                let reason = match result {
                    Ok((_, Some(e))) => format!("{e}; `cornercase plugin logs {id}` says more"),
                    Ok((_, None)) => return None,
                    Err(e) => e.to_string(),
                };
                Some(AppEvent::Plugin(PluginEvent::Failed { plugin: name, title, reason }))
            },
        );
        true
    }

    pub(super) fn plugin_answered(&mut self, event: PluginEvent) {
        match event {
            PluginEvent::Ran { request, ran } => self.plugin_ran(request, *ran),
            PluginEvent::Failed { plugin, title, reason } => {
                let message = format!("{plugin}: {title} failed: {reason}");
                self.toast = Some(super::Toast::new(message, ui::ToastIcon::Bug));
            }
        }
    }

    pub(super) fn run_hooks(&self, events: &[control::Event]) {
        for event in events {
            let Some(on) = hook_name(event) else { continue };
            let Ok(json) = serde_json::to_string(event) else { continue };
            for plugin in &self.plugins.loaded {
                let Some(manifest) = plugin.runs() else { continue };
                for hook in manifest.events.iter().filter(|hook| hook.on.name() == on) {
                    self.run_hook(plugin, &hook.command, on, &json);
                }
            }
        }
    }

    fn run_hook(&self, plugin: &Plugin, command: &[String], on: &'static str, json: &str) {
        let running = self.plugins.hooks.clone();
        if running.fetch_add(1, Ordering::SeqCst) >= MAX_HOOKS {
            running.fetch_sub(1, Ordering::SeqCst);
            log::warning!("plugins", "hook dropped", plugin = plugin.entry.id, on = on, reason = "too many running");
            return;
        }
        let (places, plugin, command) = (self.plugins.places.clone(), plugin.clone(), command.to_vec());
        let mut env = server_env();
        env.push((run::EVENT_ENV.into(), json.to_string()));
        let job = Job::new(Level::Info, "plugins", "hook").with("plugin", &plugin.entry.id).with("on", on);
        self.spawn_job(
            job,
            move || {
                let invocation = run::Invocation { argv: &command, env: &env, cwd: None };
                let ran = run::run_to_end(&places, &plugin, invocation, HOOK_WITHIN);
                let failure = ran.failure();
                if let Some(e) = &failure {
                    places.note(&plugin.entry.id, "hook failed", &[("on", &on), ("error", e)]);
                }
                running.fetch_sub(1, Ordering::SeqCst);
                failure.map_or(Ok(()), |e| Err(Error::Plugin(e)))
            },
            |_, _| None,
        );
    }

    pub(in crate::app) fn plugin_revision(&self) -> u32 {
        self.plugins.revision
    }

    pub(in crate::app) fn plugin_infos(&self, now: Instant) -> PluginsInfo {
        let places = &self.plugins.places;
        let plugins = self
            .plugins
            .loaded
            .iter()
            .map(|plugin| {
                let id = &plugin.entry.id;
                let running = self.plugins.services.as_ref().and_then(|s| s.note(id));
                let services = plugin.runs().map_or_else(Vec::new, |manifest| {
                    let seen = self.plugins.services.as_ref().map(|s| s.seen(id)).unwrap_or_default();
                    ServiceInfo::of(&manifest.service, &seen, now)
                });
                PluginInfo::of(plugin, places, running.as_deref(), services)
            })
            .collect();
        PluginsInfo { revision: Some(self.plugins.revision), problem: Registry::open(&places.registry).err(), plugins }
    }

    pub(in crate::app) fn plugin_action_infos(&self, context: Option<Context>) -> ActionsInfo {
        let actions = self
            .plugins
            .actions()
            .filter(|(_, action)| context.is_none_or(|c| action.contexts.contains(&c)))
            .map(|(plugin, action)| ActionInfo {
                plugin: plugin.entry.id.clone(),
                name: plugin.name(),
                action: action.id.clone(),
                title: action.title.clone(),
                contexts: action.contexts.iter().map(|c| c.name().to_string()).collect(),
            })
            .collect();
        ActionsInfo { revision: self.plugins.revision, actions }
    }

    pub(in crate::app) fn plugin_action(&self, plugin: &str, action: &str) -> Result<(Plugin, Action), String> {
        let found = self.plugins.loaded.iter().find(|p| p.entry.id == plugin).ok_or_else(|| missing(plugin))?;
        let manifest = match (&found.manifest, found.entry.enabled) {
            (Err(e), _) => return Err(format!("`{plugin}` cannot run: {e}")),
            (Ok(_), false) => {
                return Err(format!("`{plugin}` is off; `cornercase plugin enable {plugin}` turns it on"));
            }
            (Ok(manifest), true) => manifest,
        };
        let found_action =
            manifest.actions.iter().find(|a| a.id == action).ok_or_else(|| {
                format!("`{plugin}` has no action `{action}`; `cornercase plugin actions` lists them")
            })?;
        Ok((found.clone(), found_action.clone()))
    }

    pub(super) fn plugin_rows(&self) -> Vec<PluginRow> {
        let registry = Registry::load(&self.plugins.places.registry);
        registry
            .plugins()
            .into_iter()
            .map(|plugin| {
                let running = self.plugins.services.as_ref().and_then(|s| s.note(&plugin.entry.id));
                plugin_row(&plugin, running.as_deref())
            })
            .collect()
    }

    pub(super) fn switch_plugin(&mut self, id: &str, on: bool) {
        let notice = match Registry::open(&self.plugins.places.registry).and_then(|mut registry| {
            let plugin = registry.set_enabled(id, on)?;
            Ok((registry, plugin))
        }) {
            Ok((registry, plugin)) => match registry.save() {
                Ok(()) => {
                    log::info!("plugins", if on { "enabled" } else { "disabled" }, plugin = id);
                    self.reload_plugins();
                    if on {
                        format!("{} is on: it runs as you", plugin.name())
                    } else {
                        format!("{} is off", plugin.name())
                    }
                }
                Err(e) => format!("failed to save the plugins: {e}"),
            },
            Err(e) => e,
        };
        let rows = self.plugin_rows();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.plugins = rows;
            s.notice = Some(notice);
        }
    }
}

fn hook_name(event: &control::Event) -> Option<&'static str> {
    match event.what {
        control::What::Opened { .. } => Some("opened"),
        control::What::Closed { .. } => Some("closed"),
        control::What::Status { .. } => Some("status"),
        control::What::Exited { .. } => Some("exited"),
        control::What::Dropped { .. }
        | control::What::Clipboard { .. }
        | control::What::Bell { .. }
        | control::What::Notice { .. } => None,
    }
}

fn plugin_row(plugin: &Plugin, running: Option<&str>) -> PluginRow {
    PluginRow {
        id: plugin.entry.id.clone(),
        label: plugin.label(),
        on: plugin.runs().is_some(),
        note: plugin.note(running),
        broken: plugin.manifest.is_err(),
    }
}
