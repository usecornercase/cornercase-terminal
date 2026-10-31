use std::fmt::Write as _;
use std::io::{self, IsTerminal as _, Write as _};
use std::path::Path;

use serde::Serialize;
use serde_json::json;

use super::args::PluginAction;
use super::args::plugin::{InstallArgs, RunArgs, UpdatePluginArgs};
use super::print_log;
use crate::client::{answer, ask, confirm};
use crate::config;
use crate::control::plugins::{ActionRan, ActionsInfo, ButtonInfo, PluginInfo, PluginsInfo, ServiceInfo};
use crate::control::{Command, Item, PluginActions, PluginList, PluginPopupClose, PluginRun};
use crate::error::{Error, Result};
use crate::paths;
use crate::plugins::install::{self, Fetched, Removed, Update};
use crate::plugins::registry::{missing, short};
use crate::plugins::source::Origin;
use crate::plugins::supervise;
use crate::plugins::{Places, Plugin, Registry};

pub(super) fn run(action: PluginAction) -> Result<bool> {
    match action {
        PluginAction::Actions { context, print } => actions(context.as_deref(), print.json),
        PluginAction::Status(print) => status(print.json),
        PluginAction::Run(args) => run_action(&args),
        PluginAction::PopupClose => popup_close(),
        action => run_here(action),
    }
}

fn run_here(action: PluginAction) -> Result<bool> {
    let places = Places::new(&paths::config(), &paths::session());
    let mut registry = Registry::open(&places.registry).map_err(Error::Plugin)?;
    match action {
        PluginAction::Link { folder } => link(&mut registry, &folder)?,
        PluginAction::Install(args) => install_from(&places, &args)?,
        PluginAction::Update(args) => update(&places, &registry, &args)?,
        PluginAction::Remove { id } => remove(&places, &id)?,
        PluginAction::Unlink { id } => unlink(&mut registry, &id)?,
        PluginAction::List(print) => list(&registry, &places, print.json)?,
        PluginAction::Actions { .. } | PluginAction::Run(_) | PluginAction::PopupClose | PluginAction::Status(_) => {}
        PluginAction::Enable { id } => enable(&mut registry, &id)?,
        PluginAction::Disable { id } => disable(&mut registry, &id)?,
        PluginAction::Logs { id, logs } => {
            if !registry.entries.iter().any(|entry| entry.id == id) {
                return Err(Error::Plugin(missing(&id)));
            }
            return print_log(&places.log(&id), &logs);
        }
    }
    Ok(true)
}

fn link(registry: &mut Registry, folder: &Path) -> Result<()> {
    let (manifest, enabled) = registry.link(folder).map_err(Error::Plugin)?;
    registry.save()?;
    if enabled {
        println!("linked {} {} ({}), still on", manifest.name, manifest.version, manifest.id);
    } else {
        println!("linked {} {} ({}), turned off", manifest.name, manifest.version, manifest.id);
        println!("it {}; turn it on with `cornercase plugin enable {}`", manifest.asks(), manifest.id);
    }
    Ok(())
}

fn remove(places: &Places, id: &str) -> Result<()> {
    if supervise::running(places, id).is_ok_and(|pids| !pids.is_empty()) {
        println!("stopping {id}…");
    }
    match install::remove(places, id).map_err(Error::Plugin)? {
        Removed::Unlinked => println!("forgot {id}; its folder and its files stay where they are"),
        Removed::Deleted => println!("removed {id} and deleted its installed copy; its settings and data stay"),
    }
    Ok(())
}

fn unlink(registry: &mut Registry, id: &str) -> Result<()> {
    if registry.entry(id).is_ok_and(|entry| entry.source.installed()) {
        return Err(Error::Plugin(format!("`{id}` was installed; `cornercase plugin remove {id}` removes it")));
    }
    if !registry.unlink(id) {
        return Err(Error::Plugin(missing(id)));
    }
    registry.save()?;
    println!("unlinked {id}; its folder and its files stay where they are");
    Ok(())
}

fn list(registry: &Registry, places: &Places, json: bool) -> Result<()> {
    let plugins = registry.plugins();
    let mut out = io::stdout().lock();
    let text = if json { list_json(&plugins, places) } else { list_text(&plugins) };
    super::quietly(out.write_all(text.as_bytes()).and_then(|()| out.flush())).map(drop)
}

fn enable(registry: &mut Registry, id: &str) -> Result<()> {
    let plugin = registry.set_enabled(id, true).map_err(Error::Plugin)?;
    registry.save()?;
    let Ok(manifest) = &plugin.manifest else { return Ok(()) };
    println!("{} is on. It runs as you and can do anything you can, in every terminal.", manifest.name);
    println!("It {}; that is shown, not enforced.", manifest.asks());
    if !manifest.runs().is_empty() {
        println!("It runs {}.", manifest.runs().join(", "));
    }
    Ok(())
}

fn disable(registry: &mut Registry, id: &str) -> Result<()> {
    let plugin = registry.set_enabled(id, false).map_err(Error::Plugin)?;
    registry.save()?;
    println!("{} is off", plugin.name());
    Ok(())
}

fn status(json: bool) -> Result<bool> {
    let value = ask("plugin-list", Command::PluginList(PluginList {}))?;
    if json {
        super::print_json_text(&shown_json(&value))?;
    } else {
        super::print_out(&status_text(&answer(value)?))?;
    }
    Ok(true)
}

fn status_text(info: &PluginsInfo) -> String {
    let mut text = String::new();
    if let Some(revision) = info.revision {
        let _ = writeln!(text, "revision {revision}");
    }
    if let Some(problem) = &info.problem {
        let _ = writeln!(text, "{}", shown(problem));
    }
    if info.plugins.is_empty() && info.problem.is_none() {
        text.push_str("no plugins\n");
    }
    for plugin in &info.plugins {
        write_plugin(&mut text, plugin);
    }
    text
}

fn write_plugin(text: &mut String, plugin: &PluginInfo) {
    let _ = writeln!(text, "{}  {}  {}", shown(&plugin.id), shown(&plugin.state), shown(&plugin.label));
    if !plugin.note.is_empty() {
        let _ = writeln!(text, "  {}", shown(&plugin.note));
    }
    for service in &plugin.services {
        let _ = writeln!(text, "  {}", service_line(service));
    }
    if let Some(button) = &plugin.button {
        let _ = writeln!(text, "  {}", button_line(button));
    }
}

fn service_line(service: &ServiceInfo) -> String {
    let id = shown(&service.id);
    match service.state.as_str() {
        "running" => match service.pid {
            Some(pid) => format!("{id}: running, pid {pid}"),
            None => format!("{id}: running"),
        },
        "stopped" => match service.restart_in {
            Some(seconds) => format!("{id}: stopped, restarts in {seconds}s"),
            None => format!("{id}: stopped"),
        },
        "starting" => format!("{id}: starting"),
        other => format!("{id}: {}", shown(other)),
    }
}

fn button_line(button: &ButtonInfo) -> String {
    let mut line = format!("button {}", shown(&button.title));
    if !button.icon.is_empty() {
        let _ = write!(line, " ({})", shown(&button.icon));
    }
    if !button.actions.is_empty() {
        let actions = button.actions.iter().map(|action| shown(action)).collect::<Vec<_>>().join(", ");
        let _ = write!(line, ": {actions}");
    }
    line
}

fn shown(text: &str) -> String {
    crate::text::escape_hidden(&text.replace(|c: char| c.is_control(), " "))
}

fn actions(context: Option<&str>, json: bool) -> Result<bool> {
    let context = context
        .map(|name| serde_json::from_value(json!(name)))
        .transpose()
        .map_err(|e| Error::WrongUsage(e.to_string()))?;
    let value = ask("plugin-actions", Command::PluginActions(PluginActions { context }))?;
    if json {
        super::print_json(&value)?;
    } else {
        super::print_out(&actions_text(&answer(value)?))?;
    }
    Ok(true)
}

fn actions_text(info: &ActionsInfo) -> String {
    let cell = |text: &str| crate::text::escape_hidden(&text.replace(|c: char| c.is_control(), " "));
    let mut text = String::from("plugin\taction\tcontexts\ttitle\n");
    for action in &info.actions {
        let _ = writeln!(
            text,
            "{}\t{}\t{}\t{}",
            cell(&action.plugin),
            cell(&action.action),
            action.contexts.join(","),
            cell(&action.title)
        );
    }
    text
}

fn run_action(args: &RunArgs) -> Result<bool> {
    let item = args.which.item("plugin run")?;
    let revision = if let Some(revision) = args.revision {
        revision
    } else {
        let info: ActionsInfo = answer(ask("plugin-actions", Command::PluginActions(PluginActions::default()))?)?;
        info.revision
    };
    let request = PluginRun { plugin: args.plugin.clone(), action: args.action.clone(), revision, item };
    let value = ask("plugin-run", Command::PluginRun(request))?;
    let ran: ActionRan = answer(value.clone())?;
    if args.print.json {
        super::print_json(&value)?;
    } else {
        super::print_out(&ran_text(&ran, item))?;
    }
    Ok(ran.error.is_none())
}

fn popup_close() -> Result<bool> {
    ask("plugin-popup-close", Command::PluginPopupClose(PluginPopupClose {}))?;
    Ok(true)
}

fn ran_text(ran: &ActionRan, item: Option<Item>) -> String {
    let name = |text: &str| crate::text::escape_hidden(text);
    let mut head = format!("ran {} {}", name(&ran.plugin), name(&ran.action));
    if let Some((kind, id)) = item.map(|item| item.to_string()).as_deref().and_then(|item| item.split_once('-')) {
        let _ = write!(head, " on {kind} {id}");
    }
    match &ran.error {
        None => format!("{head}: exit status: 0\n"),
        Some(error) => format!("{head}: {error}; `cornercase plugin logs {}` says more\n", name(&ran.plugin)),
    }
}

fn install_from(places: &Places, args: &InstallArgs) -> Result<()> {
    let origin = Origin::parse(&folder_spec(&args.source)).map_err(Error::Plugin)?;
    progress(args.json, &format!("fetching {}…", crate::text::escape_hidden(&origin.spec())));
    let fetched = install::fetch(&origin, places).map_err(Error::Plugin)?;
    if args.preview {
        return previewed(&fetched, args.json, "nothing was installed");
    }
    let place = crate::text::escape_hidden(&origin.place());
    same_commit(&fetched.commit, args.commit.as_deref(), &place, "nothing was installed")?;
    if !confirmed(&fetched, args.yes, "Install it?")? {
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

fn progress(json: bool, line: &str) {
    if json {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

fn previewed(fetched: &Fetched, json: bool, kept: &str) -> Result<()> {
    if json {
        return super::print_out(&shown_json(&fetched.details()));
    }
    show(fetched);
    println!("{kept}; add --commit {} to install exactly this commit", fetched.commit);
    Ok(())
}

fn same_commit(found: &str, wanted: Option<&str>, what: &str, kept: &str) -> Result<()> {
    match wanted {
        Some(wanted) if !found.eq_ignore_ascii_case(wanted) => Err(Error::Plugin(format!(
            "{what} gives commit {} now, not {}, the one --commit names; {kept}; preview it again",
            short(found),
            short(wanted)
        ))),
        _ => Ok(()),
    }
}

fn folder_spec(spec: &str) -> String {
    if !spec.starts_with(['.', '~', '/']) {
        return spec.to_string();
    }
    let (place, at) = match spec.split_once('@') {
        Some((place, at)) => (place, format!("@{at}")),
        None => (spec, String::new()),
    };
    let (folder, subdir) = match place.split_once("//") {
        Some((folder, subdir)) => (folder, format!("//{subdir}")),
        None => (place, String::new()),
    };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let path = config::expand_home(folder, home.as_deref());
    let path = path.canonicalize().unwrap_or(path);
    format!("{}{subdir}{at}", Path::new(&path).display())
}

fn update(places: &Places, registry: &Registry, args: &UpdatePluginArgs) -> Result<()> {
    let ids: Vec<String> = match &args.id {
        Some(id) => vec![id.clone()],
        None => registry.entries.iter().filter(|e| e.source.installed()).map(|e| e.id.clone()).collect(),
    };
    if ids.is_empty() {
        println!("no installed plugins to update");
    }
    for id in &ids {
        update_one(places, id, args)?;
    }
    Ok(())
}

fn update_one(places: &Places, id: &str, args: &UpdatePluginArgs) -> Result<()> {
    progress(args.json, &format!("fetching {id}…"));
    match install::check_update(places, id).map_err(Error::Plugin)? {
        Update::UpToDate(commit) => {
            same_commit(&commit, args.commit.as_deref(), id, "nothing was changed")?;
            if args.json {
                return super::print_out(&shown_json(&json!({ "id": id, "commit": commit, "up_to_date": true })));
            }
            println!("{id} is up to date at {}", short(&commit));
            Ok(())
        }
        Update::Changed(fetched) => changed(places, id, *fetched, args),
    }
}

#[derive(Serialize)]
struct UpdatePreview<'a> {
    #[serde(flatten)]
    details: install::Details<'a>,
    up_to_date: bool,
}

fn changed(places: &Places, id: &str, fetched: Fetched, args: &UpdatePluginArgs) -> Result<()> {
    if args.json {
        return super::print_out(&shown_json(&UpdatePreview { details: fetched.details(), up_to_date: false }));
    }
    let kept = format!("{id} stays as it was");
    if !args.preview {
        same_commit(&fetched.commit, args.commit.as_deref(), id, &kept)?;
    }
    let old = fetched.replacing.as_ref().map_or("", |replaced| short(&replaced.commit));
    println!("{id} changed: {old} → {}", short(&fetched.commit));
    if args.preview {
        return previewed(&fetched, false, &kept);
    }
    if !confirmed(&fetched, args.yes, "Update it?")? {
        println!("{kept}");
        return Ok(());
    }
    let installed = install::install(fetched, places).map_err(Error::Plugin)?;
    let on = if installed.enabled { "on" } else { "off" };
    println!("updated {id} to {}, still {on}", installed.manifest.version);
    Ok(())
}

fn show(fetched: &Fetched) {
    println!();
    for line in fetched.preview() {
        println!("  {line}");
    }
    println!();
}

fn confirmed(fetched: &Fetched, yes: bool, question: &str) -> Result<bool> {
    show(fetched);
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
                let id = crate::text::escape_hidden(&plugin.entry.id);
                let _ = writeln!(text, "{id}  {}  {on}  {}", manifest.version, manifest.name);
                let _ = writeln!(text, "  {}\n  {}", plugin.entry.source.describe(), manifest.asks());
                if !manifest.runs().is_empty() {
                    let _ = writeln!(text, "  runs {}", manifest.runs().join(", "));
                }
            }
            Err(e) => {
                let _ = writeln!(text, "{}  -  {on}  cannot run", plugin.name());
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
    shown_json(&json!({ "plugins": rows }))
}

fn shown_json(value: &impl Serialize) -> String {
    let json = serde_json::to_string_pretty(value).unwrap_or_default();
    let mut text = json.chars().fold(String::with_capacity(json.len()), |mut out, c| {
        if c != '\n' && crate::text::hidden(c) {
            c.encode_utf16(&mut [0; 2]).iter().for_each(|unit| {
                let _ = write!(out, "\\u{unit:04x}");
            });
        } else {
            out.push(c);
        }
        out
    });
    text.push('\n');
    text
}

#[cfg(test)]
mod tests;
