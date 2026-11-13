use std::io::{self, Read as _, Write as _};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::CommandFactory;
use serde_json::Value;

use self::args::{
    Command, Control, EventsArgs, LogsArgs, RemoteArgs, Renamed, StartArgs, SurfaceArgs, Target, WaitArgs, WhenArgs,
    Which,
};
use self::status::run_status;
use crate::client::{self, answer, ask};
use crate::control::{self, Done, Event, Item, Report, Until, What};
use crate::error::{Error, Result};
use crate::machines::send;
use crate::os::console;
use crate::os::socket::UnixStream;
use crate::remote::{self, Remote};
use crate::{git, log, server};

mod accounts;
mod agents;
mod args;
mod arrange;
mod changes;
mod clone;
mod config;
mod context;
mod contract;
mod files;
mod flow;
mod held;
mod help;
mod issue;
mod known;
mod line;
mod lines;
mod machines;
mod older;
mod open;
mod plugin;
mod ports;
mod processes;
mod pull_request;
mod queue;
mod reach;
mod read;
mod recap;
mod requests;
mod search;
mod setup;
mod start_server;
mod stats;
mod status;
mod terminal;
mod theme;
mod todo;
mod tour;
mod tunnels;
mod update;
mod usage;
mod view;
mod worktrees;

pub use args::Cli;
pub use older::{older, parse, print_older};
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
    let remote = reach::chosen(&cli, &|name| std::env::var_os(name))?;
    crate::update::remove_old_binaries();
    match (cli.command, remote) {
        (None, None) => client::run()?,
        (None, Some(remote)) => client::remote(&remote)?,
        (Some(command), Some(remote)) if let Some(args) = reach::there(&command, &remote) => {
            return reach::run_there(&remote, &args?);
        }
        (Some(command), remote) => {
            if let Some(remote) = remote {
                client::use_remote(remote);
            }
            return run_command(command);
        }
    }
    Ok(true)
}

fn run_remote(args: RemoteArgs) -> Result<()> {
    if args.known {
        return known::run(args.json);
    }
    if let Some(destination) = args.remember {
        return known::remember(destination, args.command);
    }
    if let Some(destination) = args.forget {
        return known::forget(&destination);
    }
    let destination = args.destination.ok_or_else(|| Error::WrongUsage("say which machine to reach".into()))?;
    client::remote(&Remote { destination, command: args.command, option: "--command" })
}

fn run_command(command: Command) -> Result<bool> {
    match command {
        Command::Control(control) => return run_control(control),
        Command::Remote(args) => run_remote(args)?,
        Command::Skill => print!("{SKILL}"),
        Command::Update(update) => return update::update(&update),
        Command::Restart(restart) => update::restart(restart.yes, restart.when.when())?,
        Command::StartServer(start) => start_server::run(&start)?,
        Command::KillServer(kill) => return update::kill(kill.yes),
        Command::Logs(args) => return print_logs(&args),
        Command::Stats(args) => return stats::run(&args),
        Command::Plugin(args) => return plugin::run(args.action),
        Command::Server => server::run()?,
        Command::Proxy => remote::proxy(proxied)?,
    }
    Ok(true)
}

fn proxied(starting: bool) -> Result<UnixStream> {
    if starting { client::find_or_start().map(|found| found.stream) } else { client::running() }
}

fn print_logs(args: &LogsArgs) -> Result<bool> {
    print_log(&crate::paths::log(), args)
}

fn print_log(path: &Path, args: &LogsArgs) -> Result<bool> {
    let mut out = io::stdout().lock();
    if args.path {
        writeln!(out, "{}", path.display())?;
        return Ok(true);
    }
    let follower = if args.follow { Some(log::Follower::at_end(path)) } else { None };
    let tail = match log::tail(path, args.lines) {
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
    let mut reader = client::watched(console::stdout());
    loop {
        let new = follower.read()?;
        if !new.is_empty() && quietly(out.write_all(&new).and_then(|()| out.flush()))? {
            return Ok(true);
        }
        if client::reader_gone_within(&mut reader, FOLLOW_EVERY) {
            return Ok(true);
        }
    }
}

fn stream_surface(args: &SurfaceArgs) -> Result<()> {
    let surface = match args.view {
        Some(_) => control::Surface::default(),
        None => control::Surface {
            tab: args.tab,
            cols: args.cols.unwrap_or_default(),
            rows: args.rows.unwrap_or_default(),
            ..control::Surface::default()
        },
    };
    let surface = control::Surface { patches: args.patches, popup: args.popup, ..surface };
    if args.patches {
        client::use_surface_patches()?;
    }
    if args.popup {
        client::use_popup_terminal()?;
    }
    if args.view.is_none() && !args.popup {
        surface.area().map_err(Error::Control)?;
    }
    stream_json("surface", control::Command::Surface(surface))
}

pub(super) fn stream_json(name: &'static str, command: control::Command) -> Result<()> {
    let mut out = io::stdout().lock();
    client::follow_all(name, command, |value| quietly(writeln!(out, "{value}").and_then(|()| out.flush())))
}

pub(super) fn quietly(written: io::Result<()>) -> Result<bool> {
    match written {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(true),
        written => written.map(|()| false).map_err(Into::into),
    }
}

fn run_control(command: Control) -> Result<bool> {
    if let Some(view) = command.view() {
        client::use_view(view)?;
    }
    if let Some(boot) = command.boot() {
        client::use_boot(boot)?;
    }
    let done = match control_route(command) {
        ControlFlow::Break(done) => done,
        ControlFlow::Continue(_) => Err(Error::Control("no area runs this command".into())),
    };
    client::close_unused_link();
    done
}

fn control_route(command: Control) -> ControlFlow<Result<bool>, Control> {
    let command = create_commands(command)?;
    let command = talk_commands(command)?;
    let command = row_commands(command)?;
    let command = stream_commands(command)?;
    let command = terminal_commands(command)?;
    other_commands(command)
}

fn create_commands(command: Control) -> ControlFlow<Result<bool>, Control> {
    ControlFlow::Break(match command {
        Control::Open(args) => open::run(&args).map(|()| true),
        Control::Clone(args) => clone::run(&args).map(|()| true),
        Control::NewWorkspace(new) => run_new_workspace(new).map(|()| true),
        Control::NewTab(new) => run_new_tab(new).map(|()| true),
        Control::Split(split) => run_split(&split).map(|()| true),
        Control::Setup(args) => setup::run(&args).map(|()| true),
        Control::Start(start) => run_start(*start).map(|()| true),
        Control::Go(go) => run_go(go).map(|()| true),
        other => return ControlFlow::Continue(other),
    })
}

fn talk_commands(command: Control) -> ControlFlow<Result<bool>, Control> {
    ControlFlow::Break(match command {
        Control::Send(send) => run_send(send).map(|()| true),
        Control::Keys(keys) => run_keys(keys).map(|()| true),
        Control::Held(args) => held::run(&args).map(|()| true),
        Control::Mark(mark) => run_mark(&mark).map(|()| true),
        Control::Read(read) => read::run(&read),
        Control::Wait(wait) => run_wait(wait).map(|()| true),
        Control::Notify(notify) => run_notify(&notify).map(|()| true),
        Control::Focus(focus) => run_focus(&focus).map(|()| true),
        other => return ControlFlow::Continue(other),
    })
}

fn row_commands(command: Control) -> ControlFlow<Result<bool>, Control> {
    ControlFlow::Break(
        match command {
            Control::Close(close) => return ControlFlow::Break(run_close(&close)),
            Control::Rename(rename) => run_rename(rename),
            Control::Move(args) => arrange::move_row(&args),
            Control::MovePane(args) => arrange::move_pane(&args),
            Control::Collapse(args) => arrange::fold(&args, "collapse"),
            Control::Expand(args) => arrange::fold(&args, "expand"),
            Control::Group(args) => arrange::group(args.action),
            Control::Todo(todo) => todo::run(todo.action),
            other => return ControlFlow::Continue(other),
        }
        .map(|()| true),
    )
}

fn stream_commands(command: Control) -> ControlFlow<Result<bool>, Control> {
    ControlFlow::Break(
        match command {
            Control::Status(status) => run_status(&status),
            Control::Events(events) => follow_events(events),
            Control::Contract(contract) => contract::run(contract.print.json),
            Control::Requests(args) => requests::run(&args),
            Control::Surface(args) => stream_surface(&args),
            Control::View(args) => view::open(&args),
            Control::ViewSet(args) => view::set(&args),
            Control::Search(search) => search::run(&search),
            other => return ControlFlow::Continue(other),
        }
        .map(|()| true),
    )
}

fn terminal_commands(command: Control) -> ControlFlow<Result<bool>, Control> {
    ControlFlow::Break(
        match command {
            Control::Input(args) => terminal::input(&args),
            Control::Scroll(args) => terminal::scroll(&args),
            Control::Selection(args) => terminal::selection(&args),
            Control::Resize(args) => terminal::resize(&args),
            Control::Redraw(args) => terminal::redraw(&args),
            Control::Ratio(args) => terminal::ratio(&args),
            Control::Link(args) => terminal::link(&args),
            Control::PasteFile(args) => terminal::paste_file(&args),
            other => return ControlFlow::Continue(other),
        }
        .map(|()| true),
    )
}

fn other_commands(command: Control) -> ControlFlow<Result<bool>, Control> {
    ControlFlow::Break(match command {
        Control::Issue(args) => return ControlFlow::Break(issue::run(args.action)),
        Control::Usage(args) => return ControlFlow::Break(usage::run(&args)),
        Control::Ports(args) => return ControlFlow::Break(ports::run(&args)),
        Control::Tunnels(args) => return ControlFlow::Break(tunnels::run_list(&args)),
        Control::Tunnel(args) => return ControlFlow::Break(tunnels::run(&args)),
        Control::Processes(args) => return ControlFlow::Break(processes::run(&args)),
        Control::PullRequest(args) => return ControlFlow::Break(pull_request::run(&args)),
        Control::Context(args) => return ControlFlow::Break(context::run(&args)),
        Control::Recap(args) => return ControlFlow::Break(recap::run(&args)),
        Control::Worktrees(args) => worktrees::run(&args).map(|()| true),
        Control::Machines(args) => machines::run(&args).map(|()| true),
        Control::Tour(args) => tour::run(&args).map(|()| true),
        Control::Queue(queue) => queue::run(queue.action).map(|()| true),
        Control::Away(away) => run_away(&away).map(|()| true),
        Control::Flow(flow) => flow::run(flow.action).map(|()| true),
        Control::Theme(args) => theme::run(&args).map(|()| true),
        Control::Config(args) => config::run(args).map(|()| true),
        Control::Changes(args) => changes::run(args).map(|()| true),
        Control::Files(args) => files::run(args).map(|()| true),
        Control::Agents(args) => agents::run(args).map(|()| true),
        Control::Account(args) => accounts::account(args.action).map(|()| true),
        Control::Profile(args) => accounts::profile(args.action).map(|()| true),
        other => return ControlFlow::Continue(other),
    })
}

fn clone_parent(folder: Option<&Path>, remote: Option<&Remote>) -> Result<PathBuf> {
    match folder {
        Some(folder) => reach::open_path_on(folder, remote),
        None if remote.is_some() => Ok(PathBuf::from("~")),
        None => Ok(std::env::current_dir()?),
    }
}

fn clone_url(url: &str, remote: Option<&Remote>) -> Result<String> {
    let path = Path::new(url);
    if remote.is_some() || !git::clone::is_local_path(url) || path.is_absolute() || url.starts_with('~') {
        return Ok(url.to_string());
    }
    Ok(std::path::absolute(path)?.to_string_lossy().into_owned())
}

fn run_new_workspace(new: args::NewWorkspaceArgs) -> Result<()> {
    let (name, project, worktree, focus) = (new.name, new.project, new.worktree, new.create.focus);
    let fresh = fresh(&new.fresh)?;
    let request = control::NewWorkspace { name, project, worktree, focus, machine: machine(new.on)?, fresh };
    let value = ask("new-workspace", control::Command::NewWorkspace(request))?;
    say(value, new.create.print.json, |done| id(done.ids.workspace))
}

fn run_new_tab(new: args::NewTabArgs) -> Result<()> {
    let (profile, accounts) = accounts::profile_arg(new.profile)?;
    let request = control::NewTab {
        workspace: workspace("new-tab", new.workspace, new.worktree.as_deref())?,
        name: new.name,
        command: typed(&new.command),
        focus: new.create.focus,
        profile,
        accounts,
    };
    say(ask("new-tab", control::Command::NewTab(request))?, new.create.print.json, |done| id(done.ids.pane))
}

fn run_split(split: &args::SplitArgs) -> Result<()> {
    let request = control::Split {
        pane: split.pane,
        down: split.down,
        command: typed(&split.command),
        focus: split.create.focus,
    };
    say(ask("split", control::Command::Split(request))?, split.create.print.json, |done| id(done.ids.pane))
}

fn run_send(send: args::SendArgs) -> Result<()> {
    let text = send.text.map(|text| if text == "-" { stdin() } else { Ok(text) }).transpose()?;
    let Target { pane, tab, .. } = send.target;
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
    let value = ask("send", control::Command::Send(request))?;
    if !send.print.json {
        warn(&value);
    }
    say(value, send.print.json, ending)
}

fn warn(value: &Value) {
    if let Some(warning) = value.get("warning").and_then(Value::as_str) {
        note(&format!("warning: {warning}\n"));
    }
}

fn run_keys(keys: args::KeysArgs) -> Result<()> {
    let Target { pane, tab, .. } = keys.target;
    ask("keys", control::Command::Keys(control::Keys { pane, tab, keys: keys.keys })).map(drop)
}

fn run_mark(mark: &args::MarkArgs) -> Result<()> {
    let Target { pane, tab, .. } = mark.target;
    let request = control::Mark { pane, tab, reviewed: mark.reviewed };
    warn(&ask("mark", control::Command::Mark(request))?);
    Ok(())
}

fn run_close(close: &args::CloseArgs) -> Result<bool> {
    if close.agent {
        return arrange::close_agent(close).map(|()| true);
    }
    let item = close.which.item("close")?.ok_or_else(|| Error::Control("say what to close".into()))?;
    if close.check {
        arrange::check_removal(item, close.json)?;
        return Ok(true);
    }
    if close.unlock {
        client::use_close_unlock()?;
    }
    let request =
        control::Close { item, remove_worktree: close.remove_worktree, force: close.force, unlock: close.unlock };
    warn(&ask("close", control::Command::Close(request))?);
    Ok(true)
}

fn run_focus(focus: &args::FocusArgs) -> Result<()> {
    if let Some(machine) = &focus.machine {
        return run_focus_machine(machine, focus);
    }
    let item = focus.which.item("focus")?.ok_or_else(|| Error::Control("say what to show".into()))?;
    let reveal = focus.reveal.clone().map(|query| control::Reveal { query, line: focus.line });
    if reveal.is_some() {
        client::use_focus_reveal()?;
    }
    let value = ask("focus", control::Command::Focus(control::Focus { item, reveal }))?;
    if focus.print.json {
        return print_json(&value);
    }
    if let Some(warning) = search::unrevealed(&value, item) {
        note(&format!("warning: {warning}\n"));
    }
    Ok(())
}

fn run_focus_machine(machine: &str, focus: &args::FocusArgs) -> Result<()> {
    let item = focus.which.item("focus")?.ok_or_else(|| Error::Control("say what to show".into()))?;
    let request = control::FocusMachine { machine: machine.to_string(), item };
    let value = ask("focus-machine", control::Command::FocusMachine(request))?;
    if focus.print.json { print_json(&value) } else { Ok(()) }
}

fn run_go(go: args::GoArgs) -> Result<()> {
    let command = control::Command::Go(control::Go { token: go.token });
    say(ask("go", command)?, go.print.json, |done| id(done.ids.pane))
}

fn run_notify(notify: &args::NotifyArgs) -> Result<()> {
    let text = notify.text.join(" ");
    ask("notify", control::Command::Notify(control::Notify { text })).map(drop)
}

fn run_away(away: &args::AwayArgs) -> Result<()> {
    let value = ask("away", control::Command::Away(control::Away { off: away.off }))?;
    print_out(&away_line(&value))
}

fn run_rename(rename: args::RenameArgs) -> Result<()> {
    let Renamed { tab, workspace: id, worktree, project, group } = rename.renamed;
    let workspace = workspace("rename", id, worktree.as_deref())?;
    let item = tab
        .map(Item::Tab)
        .or_else(|| workspace.map(Item::Workspace))
        .or_else(|| project.map(Item::Project))
        .or_else(|| group.map(Item::Group));
    ask("rename", control::Command::Rename(control::Rename { item, name: rename.name })).map(drop)
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
    if args.host {
        client::use_host_events()?;
    }
    if args.notices {
        client::use_notices()?;
    }
    let mut out = io::stdout().lock();
    let events = control::Events { panes: args.panes, host: args.host, notices: args.notices };
    let command = control::Command::Events(events);
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
        (Some(subject), What::Opened { kind, parent: opener, .. }) => {
            let parent = kind.parent().and_then(named).map(|parent| format!(" in {parent}")).unwrap_or_default();
            let opener = opener.map(|pane| format!(" from pane {pane}")).unwrap_or_default();
            format!("{subject}  opened{parent}{opener}")
        }
        (Some(subject), What::Closed { .. }) => format!("{subject}  closed"),
        (Some(subject), What::Status { agent, from, to }) => {
            let state = |state: &Option<String>| state.clone().unwrap_or_else(|| "none".into());
            format!("{subject}  {agent}  {} → {}", state(from), state(to))
        }
        (Some(subject), What::Exited { program }) => format!("{subject}  {program}  exited"),
        (Some(subject), What::Clipboard { text }) => {
            let bytes = text.len();
            format!("{subject}  clipboard  {bytes} byte{}", if bytes == 1 { "" } else { "s" })
        }
        (Some(subject), What::Bell { count }) if *count > 1 => format!("{subject}  bell  {count} times"),
        (Some(subject), What::Bell { .. }) => format!("{subject}  bell"),
        (Some(subject), What::Notice { token, message, .. }) => format!("{subject}  notice  {message}  {token}"),
        (None, What::Notice { token, message, machine: Some(machine), .. }) => {
            format!("machine {machine}  notice  {message}  {token}")
        }
        (subject, What::Message { kind, message, .. }) => message_line(subject, kind, message),
        (_, What::Recap { state, range, agent, error }) => {
            let failure = error.as_ref().map(|error| format!(": {error}")).unwrap_or_default();
            format!("recap {range}  {state} ({agent}){failure}")
        }
        (_, What::Tunnel { port, local, state, error, holder, kind }) => {
            tunnel_line(*port, *local, (state, error.as_deref()), (*holder, kind.as_deref()))
        }
        (_, What::Dropped { count }) => format!(
            "{count} event{} dropped: this command fell behind; `cornercase status` shows where things are now",
            if *count == 1 { "" } else { "s" }
        ),
        (None, _) => value.to_string(),
    }
}

fn tunnel_line(
    port: u16,
    local: Option<u16>,
    (state, error): (&str, Option<&str>),
    (holder, kind): (u64, Option<&str>),
) -> String {
    let local = local.map(|local| format!(" → localhost:{local}")).unwrap_or_default();
    let error = error.map(|error| format!(": {error}")).unwrap_or_default();
    let who = if kind == Some("hold") { "hold" } else { "holder" };
    format!("tunnel {port}  {state}{local}{error}  ({who} {holder})")
}

fn message_line(subject: Option<String>, kind: &str, message: &str) -> String {
    match subject {
        Some(subject) => format!("{subject}  {kind}  {message}"),
        None => format!("{kind}  {message}"),
    }
}

pub(super) fn workspace(command: &'static str, id: Option<u64>, branch: Option<&str>) -> Result<Option<u64>> {
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

fn issue_features(issue: &args::issue::IssueArg) -> Result<()> {
    if issue.key.is_some() {
        client::use_issues()?;
    }
    if issue.workspace_name.is_some() {
        client::use_issue_names()?;
    }
    Ok(())
}

fn machine(on: args::machine::MachineArg) -> Result<Option<String>> {
    if on.machine.is_some() {
        client::use_machine_start()?;
    }
    Ok(on.machine)
}

fn placed(asked: bool) -> Result<bool> {
    if asked {
        client::use_placed_start()?;
    }
    Ok(asked)
}

fn fresh(arg: &args::FreshArg) -> Result<Option<bool>> {
    let asked = arg.asked();
    if asked == Some(true) {
        client::use_fresh_branches()?;
    }
    Ok(asked)
}

fn unwaited(start: &StartArgs) -> Result<()> {
    let waits = start.wait || start.until.is_some() || start.timeout.is_some();
    if waits && start.on.machine.is_some() {
        return Err(Error::WrongUsage(send::UNWAITED.into()));
    }
    Ok(())
}

fn run_start(start: StartArgs) -> Result<()> {
    unwaited(&start)?;
    let prompt = read_prompt(start.prompt, start.prompt_file)?;
    let chosen = start.model.is_some() || start.mode.is_some() || start.effort.is_some();
    let name = if chosen { "start-with" } else { "start" };
    issue_features(&start.issue)?;
    let (profile, accounts) = accounts::profile_arg(start.profile)?;
    let request = control::Start {
        agent: start.agent,
        model: start.model,
        mode: start.mode,
        effort: start.effort,
        project: start.project,
        worktree: start.worktree,
        workspace: start.workspace,
        name: start.name,
        prompt,
        issue: start.issue.key,
        source: start.issue.source,
        workspace_name: start.issue.workspace_name,
        wait: start.wait,
        until: start.until.map_or(Until::Stops, Until::from),
        timeout: start.timeout,
        focus: start.create.focus,
        profile,
        accounts,
        machine: machine(start.on)?,
        placed: placed(start.placed)?,
        fresh: fresh(&start.fresh)?,
    };
    let command = if name == "start" { control::Command::Start(request) } else { control::Command::StartWith(request) };
    say(ask(name, command)?, start.create.print.json, |done| {
        id(done.ids.pane).into_iter().chain(ending(done)).collect()
    })
}

fn run_wait(wait: WaitArgs) -> Result<()> {
    let several = several(&wait).map_err(|e| Error::WrongUsage(usage(&e)))?;
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

pub(super) fn usage(error: &clap::Error) -> String {
    let text = error.to_string();
    text.strip_prefix("error: ").unwrap_or(&text).trim_end().to_string()
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

fn partial_unless(all_answered: bool) -> Result<bool> {
    if all_answered { Ok(true) } else { Err(Error::Partial) }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::Control(e.to_string()))? + "\n";
    print_json_text(&text)
}

fn print_json_text(text: &str) -> Result<()> {
    if line::json(text) {
        return Ok(());
    }
    print_out(text)
}

fn print_out(text: &str) -> Result<()> {
    if line::text(text) {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    quietly(out.write_all(text.as_bytes()).and_then(|()| out.flush())).map(drop)
}

fn say(value: Value, json: bool, lines: impl Fn(&Done) -> Vec<String>) -> Result<()> {
    if json {
        return print_json(&value);
    }
    for line in lines(&answer(value)?) {
        print_out(&format!("{line}\n"))?;
    }
    Ok(())
}

pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os(crate::paths::HOME_VAR).map(PathBuf::from)
}

pub(super) fn note(text: &str) {
    if !line::stderr(text) {
        eprint!("{text}");
    }
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

fn read_capped(file: &Path, limit: usize) -> Result<Vec<u8>> {
    let most = limit as u64 + 1;
    let mut bytes = Vec::new();
    if file == Path::new("-") {
        line::input().take(most).read_to_end(&mut bytes)?;
    } else {
        let open =
            std::fs::File::open(file).map_err(|e| Error::Control(format!("cannot read `{}`: {e}", file.display())))?;
        open.take(most).read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn read_limited(file: &Path, limit: usize, too_long: &str) -> Result<String> {
    let bytes = read_capped(file, limit)?;
    if bytes.len() > limit {
        return Err(Error::Control(format!("`{}` {too_long}", file.display())));
    }
    String::from_utf8(bytes).map_err(|_| Error::Control(format!("`{}` is not text", file.display())))
}

fn stdin() -> Result<String> {
    let mut text = String::new();
    line::input().read_to_string(&mut text)?;
    Ok(trimmed(&text))
}

#[cfg(test)]
mod tests;
