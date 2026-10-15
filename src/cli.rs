use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::CommandFactory;
use serde_json::Value;

use self::args::{
    Command, Control, EventsArgs, LogsArgs, Renamed, StartArgs, StatsArgs, Target, TodoAction, WaitArgs, WhenArgs,
    Which,
};
use self::status::run_status;
use crate::client::{self, answer, ask};
use crate::control::{self, Done, Event, Item, Report, TodoList, Until, What};
use crate::error::{Error, Result};
use crate::remote::{self, Remote};
use crate::{log, server, update::restart};

mod args;
mod flow;
mod help;
mod queue;
mod read;
mod search;
mod status;
mod update;

pub use args::Cli;
#[cfg(test)]
pub(crate) use status::{pane_table, render};

const FOLLOW_EVERY: Duration = Duration::from_millis(200);
pub(crate) const SKILL: &str = include_str!("../skills/cornercase/SKILL.md");

impl Which {
    fn item(&self, command: &'static str) -> Result<Option<Item>> {
        let workspace = workspace(command, self.workspace, self.worktree.as_deref())?;
        Ok(self
            .pane
            .map(Item::Pane)
            .or_else(|| self.tab.map(Item::Tab))
            .or_else(|| workspace.map(Item::Workspace))
            .or_else(|| self.project.map(Item::Project)))
    }
}

impl WhenArgs {
    fn when(&self) -> update::When {
        if self.when_idle { update::When::Idle { timeout: self.timeout } } else { update::When::Now }
    }
}

pub fn run(cli: Cli) -> Result<bool> {
    let Some(command) = cli.command else {
        client::run()?;
        return Ok(true);
    };
    match command {
        Command::Control(control) => return run_control(control),
        Command::Remote(args) => client::remote(&Remote { destination: args.destination, command: args.command })?,
        Command::Skill => print!("{SKILL}"),
        Command::Update(update) => return update::update(update.check, update.yes, update.when.when()),
        Command::Restart(restart) => update::restart(restart.yes, restart.when.when())?,
        Command::KillServer => {
            let running = client::running_now();
            if !client::kill_server()? {
                eprintln!("no cornercase server is running");
            } else if let Some(line) = running.as_deref().and_then(restart::stopped) {
                println!("{line}");
            }
        }
        Command::Logs(args) => return print_logs(&args),
        Command::Stats(args) => return print_stats(&args),
        Command::Server => server::run()?,
        Command::Proxy => remote::proxy(client::connect_or_start)?,
    }
    Ok(true)
}

fn print_logs(args: &LogsArgs) -> Result<bool> {
    let path = crate::paths::log();
    let mut out = io::stdout().lock();
    if args.path {
        writeln!(out, "{}", path.display())?;
        return Ok(true);
    }
    let follower = if args.follow { Some(log::Follower::at_end(&path)) } else { None };
    let tail = match log::tail(&path, args.lines) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("no log yet: {} does not exist", path.display());
            return Ok(false);
        }
        tail => tail?,
    };
    if quietly(out.write_all(tail.as_bytes()).and_then(|()| out.flush()))? {
        return Ok(true);
    }
    let Some(follower) = follower else { return Ok(true) };
    let mut follower = follower?;
    loop {
        let new = follower.read()?;
        if !new.is_empty() && quietly(out.write_all(&new).and_then(|()| out.flush()))? {
            return Ok(true);
        }
        std::thread::sleep(FOLLOW_EVERY);
    }
}

fn quietly(written: io::Result<()>) -> Result<bool> {
    match written {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(true),
        written => written.map(|()| false).map_err(Into::into),
    }
}

fn run_control(command: Control) -> Result<bool> {
    match command {
        Control::Status(status) => run_status(&status),
        Control::Open(open) => {
            let path = std::path::absolute(&open.path)?;
            let value = ask("open", control::Command::Open(control::Open { path, focus: open.create.focus }))?;
            say(value, open.create.print.json, |done| id(done.ids.project))
        }
        Control::NewWorkspace(new) => {
            let (name, project, worktree, focus) = (new.name, new.project, new.worktree, new.create.focus);
            let request = control::NewWorkspace { name, project, worktree, focus };
            let value = ask("new-workspace", control::Command::NewWorkspace(request))?;
            say(value, new.create.print.json, |done| id(done.ids.workspace))
        }
        Control::NewTab(new) => {
            let request = control::NewTab {
                workspace: workspace("new-tab", new.workspace, new.worktree.as_deref())?,
                name: new.name,
                command: typed(&new.command),
                focus: new.create.focus,
            };
            say(ask("new-tab", control::Command::NewTab(request))?, new.create.print.json, |done| id(done.ids.pane))
        }
        Control::Split(split) => {
            let request = control::Split {
                pane: split.pane,
                down: split.down,
                command: typed(&split.command),
                focus: split.create.focus,
            };
            say(ask("split", control::Command::Split(request))?, split.create.print.json, |done| id(done.ids.pane))
        }
        Control::Start(start) => run_start(start),
        Control::Queue(queue) => queue::run(queue.action),
        Control::Send(send) => {
            let text = send.text.map(|text| if text == "-" { stdin() } else { Ok(text) }).transpose()?;
            let Target { pane, tab } = send.target;
            let request = control::SendText {
                pane,
                tab,
                text,
                enter: send.enter,
                wait: send.wait,
                until: send.until.map_or(Until::Stops, Until::from),
                timeout: send.timeout,
                force: send.force,
            };
            say(ask("send", control::Command::Send(request))?, send.print.json, ending)
        }
        Control::Keys(keys) => {
            let Target { pane, tab } = keys.target;
            ask("keys", control::Command::Keys(control::Keys { pane, tab, keys: keys.keys })).map(drop)
        }
        Control::Mark(mark) => {
            let Target { pane, tab } = mark.target;
            let request = control::Mark { pane, tab, reviewed: mark.reviewed };
            ask("mark", control::Command::Mark(request)).map(drop)
        }
        Control::Read(read) => return read::run(&read),
        Control::Wait(wait) => run_wait(wait),
        Control::Close(close) => {
            let item = close.which.item("close")?.ok_or_else(|| Error::Control("say what to close".into()))?;
            let request = control::Close { item, remove_worktree: close.remove_worktree, force: close.force };
            ask("close", control::Command::Close(request)).map(drop)
        }
        Control::Rename(rename) => {
            let Renamed { tab, workspace: id, worktree, project, group } = rename.renamed;
            let workspace = workspace("rename", id, worktree.as_deref())?;
            let item = tab
                .map(Item::Tab)
                .or_else(|| workspace.map(Item::Workspace))
                .or_else(|| project.map(Item::Project))
                .or_else(|| group.map(Item::Group));
            ask("rename", control::Command::Rename(control::Rename { item, name: rename.name })).map(drop)
        }
        Control::Focus(focus) => {
            let item = focus.which.item("focus")?.ok_or_else(|| Error::Control("say what to show".into()))?;
            ask("focus", control::Command::Focus(control::Focus { item })).map(drop)
        }
        Control::Notify(notify) => {
            let text = notify.text.join(" ");
            ask("notify", control::Command::Notify(control::Notify { text })).map(drop)
        }
        Control::Search(search) => search::run(&search),
        Control::Away(away) => {
            let value = ask("away", control::Command::Away(control::Away { off: away.off }))?;
            print_out(&away_line(&value))
        }
        Control::Todo(todo) => run_todo(todo.action),
        Control::Events(events) => follow_events(events),
        Control::Flow(flow) => flow::run(flow.action),
    }?;
    Ok(true)
}

fn away_line(value: &Value) -> String {
    let on = value.get("away").and_then(Value::as_bool) == Some(true);
    let platforms: Option<Vec<&str>> =
        value.get("platforms").and_then(Value::as_array).map(|list| list.iter().filter_map(Value::as_str).collect());
    let names = match platforms {
        None => "Telegram".to_string(),
        Some(list) if list.is_empty() && !on => return "back: nothing was away\n".into(),
        Some(list) if list.is_empty() => "Telegram or Discord".to_string(),
        Some(list) => list.join(" and "),
    };
    if on {
        format!("away: agents message you on {names} until you turn it off\n")
    } else {
        format!("back: agents no longer message you on {names}\n")
    }
}

fn follow_events(args: EventsArgs) -> Result<()> {
    let mut out = io::stdout().lock();
    let command = control::Command::Events(control::Events { panes: args.panes });
    client::follow("events", command, |value| {
        let line = if args.json { value.to_string() } else { event_line(&value) };
        quietly(writeln!(out, "{line}").and_then(|()| out.flush()))
    })
}

fn event_line(value: &Value) -> String {
    let Ok(event) = serde_json::from_value::<Event>(value.clone()) else { return value.to_string() };
    let named = |kind: control::Kind| kind.id(&event.ids).map(|id| format!("{} {id}", kind.name()));
    let subject = event.kind().and_then(named);
    match (subject, &event.what) {
        (Some(subject), What::Opened { kind }) => {
            let parent = kind.parent().and_then(named).map(|parent| format!(" in {parent}")).unwrap_or_default();
            format!("{subject}  opened{parent}")
        }
        (Some(subject), What::Closed { .. }) => format!("{subject}  closed"),
        (Some(subject), What::Status { agent, from, to }) => {
            let state = |state: &Option<String>| state.clone().unwrap_or_else(|| "none".into());
            format!("{subject}  {agent}  {} → {}", state(from), state(to))
        }
        (Some(subject), What::Exited { program }) => format!("{subject}  {program}  exited"),
        (_, What::Dropped { count }) => format!(
            "{count} event{} dropped: this command fell behind; `cornercase status` shows where things are now",
            if *count == 1 { "" } else { "s" }
        ),
        (None, _) => value.to_string(),
    }
}

fn workspace(command: &'static str, id: Option<u64>, branch: Option<&str>) -> Result<Option<u64>> {
    let Some(branch) = branch else { return Ok(id) };
    let report: Report = answer(ask(command, control::Command::Status(control::Status {}))?)?;
    report.workspace_on(branch).map(Some).map_err(Error::WrongUsage)
}

fn read_prompt(prompt: Option<String>, file: Option<PathBuf>) -> Result<Option<String>> {
    Ok(match (prompt, file) {
        (Some(prompt), _) => Some(prompt),
        (None, Some(file)) if file == Path::new("-") => Some(stdin()?),
        (None, Some(file)) => Some(trimmed(
            &std::fs::read_to_string(&file)
                .map_err(|e| Error::Control(format!("cannot read the prompt from `{}`: {e}", file.display())))?,
        )),
        (None, None) => None,
    })
}

fn run_start(start: StartArgs) -> Result<()> {
    let prompt = read_prompt(start.prompt, start.prompt_file)?;
    let chosen = start.model.is_some() || start.mode.is_some() || start.effort.is_some();
    let name = if chosen { "start-with" } else { "start" };
    let request = control::Start {
        agent: start.agent,
        model: start.model,
        mode: start.mode,
        effort: start.effort,
        project: None,
        worktree: start.worktree,
        workspace: start.workspace,
        name: start.name,
        prompt,
        wait: start.wait,
        until: start.until.map_or(Until::Stops, Until::from),
        timeout: start.timeout,
        focus: start.create.focus,
    };
    let command = if name == "start" { control::Command::Start(request) } else { control::Command::StartWith(request) };
    say(ask(name, command)?, start.create.print.json, |done| {
        id(done.ids.pane).into_iter().chain(ending(done)).collect()
    })
}

fn run_wait(wait: WaitArgs) -> Result<()> {
    let several = several(&wait).unwrap_or_else(|e| e.exit());
    let until = match (wait.until, wait.text, wait.quiet) {
        (Some(until), ..) => until.into(),
        (None, Some(text), _) => Until::Text(text),
        (None, None, Some(quiet)) => Until::Quiet(quiet),
        (None, None, None) => Until::Stops,
    };
    let request = control::Wait { until, timeout: wait.timeout, ..control::Wait::default() };
    if !several {
        let (pane, tab) = (wait.panes.first().copied(), wait.tabs.first().copied());
        return say(
            ask("wait", control::Command::Wait(control::Wait { pane, tab, ..request }))?,
            wait.print.json,
            ending,
        );
    }
    let request = control::Wait { panes: wait.panes, tabs: wait.tabs, all: wait.all, ..request };
    say(ask("wait", control::Command::WaitSeveral(request))?, wait.print.json, endings)
}

fn several(wait: &WaitArgs) -> std::result::Result<bool, clap::Error> {
    if wait.any || wait.all {
        return Ok(true);
    }
    if wait.panes.len() + wait.tabs.len() > 1 {
        let command = Cli::command().find_subcommand("wait").cloned().unwrap_or_else(Cli::command);
        let mut command = command.bin_name("cornercase wait");
        return Err(command.error(clap::error::ErrorKind::MissingRequiredArgument, "several panes need --any or --all"));
    }
    Ok(false)
}

fn run_todo(action: TodoAction) -> Result<()> {
    match action {
        TodoAction::Add { text } => {
            let value = ask("todo", control::Command::Todo(control::Todo::Add(text.join(" "))))?;
            say(value, false, |done| id(done.todo))
        }
        TodoAction::List(print) => {
            let value = ask("todo", control::Command::Todo(control::Todo::List))?;
            if print.json {
                return print_json(&value);
            }
            let list: TodoList = answer(value)?;
            for item in list.todos {
                println!("{} [{}] {}", item.id, if item.done { 'x' } else { ' ' }, item.text);
            }
            Ok(())
        }
        TodoAction::Done { id } => ask("todo", control::Command::Todo(control::Todo::Done(id))).map(drop),
        TodoAction::Rm { id } => ask("todo", control::Command::Todo(control::Todo::Rm(id))).map(drop),
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::Control(e.to_string()))?;
    print_out(&(text + "\n"))
}

fn print_out(text: &str) -> Result<()> {
    let mut out = io::stdout().lock();
    quietly(out.write_all(text.as_bytes()).and_then(|()| out.flush())).map(drop)
}

fn say(value: Value, json: bool, lines: impl Fn(&Done) -> Vec<String>) -> Result<()> {
    if json {
        return print_json(&value);
    }
    for line in lines(&answer(value)?) {
        println!("{line}");
    }
    Ok(())
}

fn id(id: Option<u64>) -> Vec<String> {
    id.map(|id| id.to_string()).into_iter().collect()
}

fn ending(done: &Done) -> Vec<String> {
    let line = match done.ended.as_deref() {
        Some("text") => done.line.clone(),
        ended => ended.map(str::to_string),
    };
    line.into_iter().collect()
}

fn endings(done: &Done) -> Vec<String> {
    let line = |pane: &Done| Some(format!("{} {}", pane.ids.tab.or(pane.ids.pane)?, ending(pane).first()?));
    done.panes.iter().filter_map(line).collect()
}

fn typed(words: &[String]) -> Option<String> {
    (!words.is_empty()).then(|| words.join(" "))
}

fn trimmed(text: &str) -> String {
    text.trim_end_matches(['\n', '\r']).to_string()
}

fn stdin() -> Result<String> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    Ok(trimmed(&text))
}

fn print_stats(args: &StatsArgs) -> Result<bool> {
    let now = std::time::SystemTime::now();
    let path = crate::paths::stats(&crate::paths::session());
    let view = match ask("stats", control::Command::Stats(control::Stats {})) {
        Ok(value) => {
            let mut view: crate::agents::stats::View = answer(value)?;
            view.days = crate::time::Stamp::parse(&view.since)
                .map_or(0, |since| now.duration_since(since).unwrap_or_default().as_secs() / 86_400);
            view
        }
        Err(Error::NoServer) => crate::agents::stats::load(&path, now).view(now),
        Err(e) => return Err(e),
    };
    if args.json {
        print_json(&view)?;
    } else {
        print_out(&(if args.share { view.share_link() } else { view.text() } + "\n"))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
