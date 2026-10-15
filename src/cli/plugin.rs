use std::fmt::Write as _;
use std::io::{self, IsTerminal as _, Write as _};
use std::path::Path;

use serde_json::json;

use super::args::PluginAction;
use super::print_log;
use crate::client::confirm;
use crate::config;
use crate::error::{Error, Result};
use crate::paths;
use crate::plugins::install::{self, Fetched, Removed, Update};
use crate::plugins::registry::{Source, missing, short};
use crate::plugins::source::Origin;
use crate::plugins::{Places, Plugin, Registry};

pub(super) fn run(action: PluginAction) -> Result<bool> {
    let places = Places::new(&paths::config(), &paths::session());
    let mut registry = Registry::open(&places.registry).map_err(Error::Plugin)?;
    match action {
        PluginAction::Link { folder } => {
            let (manifest, enabled) = registry.link(&folder).map_err(Error::Plugin)?;
            registry.save()?;
            if enabled {
                println!("linked {} {} ({}), still on", manifest.name, manifest.version, manifest.id);
            } else {
                println!("linked {} {} ({}), turned off", manifest.name, manifest.version, manifest.id);
                println!("it {}; turn it on with `cornercase plugin enable {}`", manifest.asks(), manifest.id);
            }
        }
        PluginAction::Install { source, yes } => install_from(&places, &source, yes)?,
        PluginAction::Update { id, yes } => update(&places, &registry, id, yes)?,
        PluginAction::Remove { id } => match install::remove(&places, &id).map_err(Error::Plugin)? {
            Removed::Unlinked => println!("forgot {id}; its folder and its files stay where they are"),
            Removed::Deleted => println!("removed {id} and deleted its installed copy; its settings and data stay"),
        },
        PluginAction::Unlink { id } => {
            if registry.entry(&id).is_ok_and(|entry| entry.source.installed()) {
                return Err(Error::Plugin(format!("`{id}` was installed; `cornercase plugin remove {id}` removes it")));
            }
            if !registry.unlink(&id) {
                return Err(Error::Plugin(missing(&id)));
            }
            registry.save()?;
            println!("unlinked {id}; its folder and its files stay where they are");
        }
        PluginAction::List(print) => {
            let plugins = registry.plugins();
            let mut out = io::stdout().lock();
            let text = if print.json { list_json(&plugins, &places) } else { list_text(&plugins) };
            if super::quietly(out.write_all(text.as_bytes()).and_then(|()| out.flush()))? {
                return Ok(true);
            }
        }
        PluginAction::Enable { id } => {
            let plugin = registry.set_enabled(&id, true).map_err(Error::Plugin)?;
            registry.save()?;
            let Ok(manifest) = &plugin.manifest else { return Ok(true) };
            println!("{} is on. It runs as you and can do anything you can, in every terminal.", manifest.name);
            println!("It {}; that is shown, not enforced.", manifest.asks());
            if !manifest.runs().is_empty() {
                println!("It runs {}.", manifest.runs().join(", "));
            }
        }
        PluginAction::Disable { id } => {
            let plugin = registry.set_enabled(&id, false).map_err(Error::Plugin)?;
            registry.save()?;
            println!("{} is off", plugin.name());
        }
        PluginAction::Logs { id, logs } => {
            if !registry.entries.iter().any(|entry| entry.id == id) {
                return Err(Error::Plugin(missing(&id)));
            }
            return print_log(&places.log(&id), &logs);
        }
    }
    Ok(true)
}

fn install_from(places: &Places, spec: &str, yes: bool) -> Result<()> {
    let origin = Origin::parse(&folder_spec(spec)).map_err(Error::Plugin)?;
    println!("fetching {}…", origin.spec());
    let fetched = install::fetch(&origin, places).map_err(Error::Plugin)?;
    if !confirmed(&fetched, yes, "Install it?")? {
        println!("nothing was installed");
        return Ok(());
    }
    let installed = install::install(fetched, places).map_err(Error::Plugin)?;
    let m = &installed.manifest;
    if let Some(previous) = installed.previous {
        println!("updated {} from {} to {}", m.id, short(&previous), m.version);
    } else {
        println!("installed {} {} ({}), turned off", m.name, m.version, m.id);
        println!("turn it on with `cornercase plugin enable {}`", m.id);
    }
    Ok(())
}

fn folder_spec(spec: &str) -> String {
    if !spec.starts_with(['.', '~', '/']) {
        return spec.to_string();
    }
    let (place, at) = match spec.split_once('@') {
        Some((place, at)) => (place, Some(at)),
        None => (spec, None),
    };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let path = config::expand_home(place, home.as_deref());
    let path = path.canonicalize().unwrap_or(path);
    let path = Path::new(&path).to_string_lossy().into_owned();
    at.map_or_else(|| path.clone(), |at| format!("{path}@{at}"))
}

fn update(places: &Places, registry: &Registry, id: Option<String>, yes: bool) -> Result<()> {
    let ids: Vec<String> = match id {
        Some(id) => vec![id],
        None => registry.entries.iter().filter(|e| e.source.installed()).map(|e| e.id.clone()).collect(),
    };
    if ids.is_empty() {
        println!("no installed plugins to update");
    }
    for id in ids {
        println!("fetching {id}…");
        match install::check_update(places, &id).map_err(Error::Plugin)? {
            Update::UpToDate(commit) => println!("{id} is up to date at {}", short(&commit)),
            Update::Changed(fetched) => {
                let old = registry.entry(&id).ok().and_then(|entry| match &entry.source {
                    Source::Git { commit, .. } => Some(short(commit).to_string()),
                    Source::Link { .. } => None,
                });
                println!("{id} changed: {} → {}", old.unwrap_or_default(), short(&fetched.commit));
                if !confirmed(&fetched, yes, "Update it?")? {
                    println!("{id} stays as it was");
                    continue;
                }
                let installed = install::install(*fetched, places).map_err(Error::Plugin)?;
                let on = if installed.enabled { "on" } else { "off" };
                println!("updated {id} to {}, still {on}", installed.manifest.version);
            }
        }
    }
    Ok(())
}

fn confirmed(fetched: &Fetched, yes: bool, question: &str) -> Result<bool> {
    println!();
    for line in fetched.preview() {
        println!("  {line}");
    }
    println!();
    if yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        return Err(Error::Plugin("no terminal to ask in; pass --yes once you have read the above".into()));
    }
    Ok(confirm(&format!("{question} [y/N] ")))
}

fn list_text(plugins: &[Plugin]) -> String {
    if plugins.is_empty() {
        return "no plugins; `cornercase plugin install <owner/repo>` or `link <folder>` adds one\n".into();
    }
    let mut text = String::new();
    for plugin in plugins {
        let on = if plugin.entry.enabled { "on" } else { "off" };
        match &plugin.manifest {
            Ok(manifest) => {
                let _ = writeln!(text, "{}  {}  {on}  {}", plugin.entry.id, manifest.version, manifest.name);
                let _ = writeln!(text, "  {}\n  {}", plugin.entry.source.describe(), manifest.asks());
                if !manifest.runs().is_empty() {
                    let _ = writeln!(text, "  runs {}", manifest.runs().join(", "));
                }
            }
            Err(e) => {
                let _ = writeln!(text, "{}  -  {on}  cannot run", plugin.entry.id);
                let _ = writeln!(text, "  {}\n  {e}", plugin.entry.source.describe());
            }
        }
    }
    text
}

fn list_json(plugins: &[Plugin], places: &Places) -> String {
    let rows: Vec<_> = plugins
        .iter()
        .map(|plugin| {
            let (manifest, error) = match &plugin.manifest {
                Ok(manifest) => (serde_json::to_value(manifest).ok(), None),
                Err(e) => (None, Some(e.clone())),
            };
            json!({
                "id": plugin.entry.id,
                "enabled": plugin.entry.enabled,
                "source": plugin.entry.source,
                "log": places.log(&plugin.entry.id),
                "manifest": manifest,
                "error": error,
            })
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&json!({ "plugins": rows })).unwrap_or_default();
    text.push('\n');
    text
}
