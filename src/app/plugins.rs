use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::App;
use super::catalogue_dialog::Fetch;
use super::event::AppEvent;
use super::menus::MenuAction;
use super::overlay::Overlay;
use crate::control;
use crate::error::{Error, Result};
use crate::log::{self, Job, Level};
use crate::plugins::catalogue::{self, Index};
use crate::plugins::install::Fetched;
use crate::plugins::manifest::Action;
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
    fn context(self) -> Context {
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
    Catalogue(Result<Index>),
    Fetched { id: String, result: std::result::Result<Box<Fetched>, String> },
    Installed { id: String, result: std::result::Result<String, String> },
}

impl PluginEvent {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Failed { .. } => "plugin action",
            Self::Catalogue(_) => "plugin catalogue",
            Self::Fetched { .. } => "plugin fetched",
            Self::Installed { .. } => "plugin installed",
        }
    }
}

pub(super) struct Plugins {
    pub(super) places: Places,
    pub(super) catalogue: Fetch,
    pub(super) catalogue_url: String,
    pub(super) github: String,
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
            catalogue: Fetch::Closed,
            catalogue_url: catalogue::url(),
            github: catalogue::GITHUB.into(),
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

    fn actions(&self) -> impl Iterator<Item = (&Plugin, &Action)> {
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

fn run_to_end(
    places: &Places,
    plugin: &Plugin,
    argv: &[String],
    env: &[(String, String)],
    cwd: Option<&Path>,
    within: Duration,
) -> Result<()> {
    let child = places.command(plugin, argv, env, cwd).and_then(|mut c| c.spawn());
    let child =
        child.map_err(|e| Error::Plugin(format!("cannot start {}: {e}", argv.first().map_or("", String::as_str))))?;
    match run::finish(child, within)? {
        Some(status) if status.success() => Ok(()),
        Some(status) => Err(Error::Plugin(status.to_string())),
        None => Err(Error::Plugin(format!("stopped after {} s", within.as_secs()))),
    }
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

    fn reload_plugins(&mut self) {
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

    fn spot_env(&self, spot: Spot) -> Option<(Vec<(String, String)>, PathBuf)> {
        let (p, w, t, pane) = match spot {
            Spot::Project(project) => {
                let p = self.projects.project_index(project)?;
                return Some((vec![(PROJECT_ENV.into(), project.to_string())], self.projects[p].path.clone()));
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
        let mut env = vec![
            (PROJECT_ENV.to_string(), self.projects[p].id.to_string()),
            (WORKSPACE_ENV.to_string(), workspace.id.to_string()),
        ];
        env.extend(t.map(|t| (TAB_ENV.to_string(), workspace.tabs[t].id.to_string())));
        if let Some(pane) = pane {
            env.push((control::PANE_ENV.to_string(), pane.to_string()));
            env.push((control::SERVER_ENV.to_string(), control::server_token().to_string()));
        }
        Some((env, workspace.path.clone()))
    }

    pub(super) fn run_plugin_action(&self, spot: Spot, index: u16, revision: u32) {
        if revision != self.plugins.revision {
            return;
        }
        let Some((plugin, action)) = self.plugins.actions().nth(usize::from(index)) else { return };
        let Some((spot_env, cwd)) = self.spot_env(spot) else { return };
        let (plugin, action) = (plugin.clone(), action.clone());
        let mut env = server_env();
        env.extend(spot_env);
        env.push((ACTION_ENV.into(), action.id.clone()));
        env.push((CONTEXT_ENV.into(), spot.context().name().into()));
        let places = self.plugins.places.clone();
        let job =
            Job::new(Level::Info, "plugins", "action").with("plugin", &plugin.entry.id).with("action", &action.id);
        let (name, title) = (plugin.name().to_string(), action.title.clone());
        self.spawn_job(
            job,
            move || {
                places.note(&plugin.entry.id, "action started", &[("action", &action.id)]);
                let ran = run_to_end(&places, &plugin, &action.command, &env, Some(&cwd), ACTION_WITHIN);
                if let Err(e) = &ran {
                    places.note(&plugin.entry.id, "action failed", &[("action", &action.id), ("error", e)]);
                }
                ran.map_err(|e| Error::Plugin(format!("{e}; `cornercase plugin logs {}` says more", plugin.entry.id)))
            },
            move |ran, _| {
                let reason = ran.err()?.to_string();
                Some(AppEvent::Plugin(PluginEvent::Failed { plugin: name, title, reason }))
            },
        );
    }

    pub(super) fn plugin_answered(&mut self, event: PluginEvent) {
        match event {
            PluginEvent::Failed { plugin, title, reason } => {
                let message = format!("{plugin}: {title} failed: {reason}");
                self.toast = Some(super::Toast::new(message, ui::ToastIcon::Bug));
            }
            PluginEvent::Catalogue(fetched) => self.catalogue_fetched(fetched),
            PluginEvent::Fetched { id, result } => self.plugin_fetched(&id, result),
            PluginEvent::Installed { id, result } => self.plugin_installed(&id, result),
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
                let ran = run_to_end(&places, &plugin, &command, &env, None, HOOK_WITHIN);
                if let Err(e) = &ran {
                    places.note(&plugin.entry.id, "hook failed", &[("on", &on), ("error", e)]);
                }
                running.fetch_sub(1, Ordering::SeqCst);
                ran
            },
            |_, _| None,
        );
    }

    pub(super) fn plugin_rows(&self) -> Vec<PluginRow> {
        let registry = Registry::load(&self.plugins.places.registry);
        registry
            .plugins()
            .into_iter()
            .map(|plugin| {
                let running = self.plugins.services.as_ref().and_then(|s| s.note(&plugin.entry.id));
                plugin_row(&plugin, running)
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
        control::What::Dropped { .. } => None,
    }
}

fn plugin_row(plugin: &Plugin, running: Option<String>) -> PluginRow {
    let id = plugin.entry.id.clone();
    match &plugin.manifest {
        Err(e) => PluginRow { id, label: plugin.entry.id.clone(), on: false, note: e.clone(), broken: true },
        Ok(manifest) => {
            let on = plugin.entry.enabled;
            let windows = if manifest.window.is_empty() { "" } else { " · its window process runs in local windows" };
            let note = match running {
                Some(running) if on => running,
                _ if on => format!("{}{windows}", manifest.description),
                _ => format!("{} · runs as you", manifest.asks()),
            };
            let label = format!("{} {}", manifest.name, manifest.version);
            PluginRow { id, label, on, note, broken: false }
        }
    }
}
