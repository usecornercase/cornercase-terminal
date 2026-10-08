use std::fmt::Write as _;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;

use crate::client::{answer, ask};
use crate::control::{self, Done, Item, ProjectInfo, Report, TodoList, Until};
use crate::error::{Error, Result};
use crate::keys;
use crate::log;
use crate::remote::{self, Remote};
use crate::{client, restart, server, ui};

const FOLLOW_EVERY: Duration = Duration::from_millis(200);
pub const SKILL: &str = include_str!("../skills/cornercase/SKILL.md");

const ABOUT: &str = "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse";
const LONG_ABOUT: &str = "A terminal multiplexer for projects, git worktrees and coding agents, driven by the mouse.

Without a command, cornercase opens its window, starting the server if none is running. The other
commands drive the running server from scripts, git hooks and the agents in its panes; they never
start a server.";
const AFTER_HELP: &str = "Ids come from `cornercase status` and last while the server runs. Inside a pane, the commands
act on that pane, its tab, workspace and project unless told otherwise; elsewhere on what the window
shows. Where a command takes --workspace ID, --worktree BRANCH names the workspace on that branch
instead, in that same project. Only `focus` and `--focus` change what the window shows. The
commands exit with 1 on errors and timeouts, and 2 on wrong usage, a branch no workspace is on
included, or one several are on (both list the workspaces to pick from).

Examples:
  cornercase status
  pane=$(cornercase new-tab --name tests -- cargo test)
  cornercase wait --pane \"$pane\" --until shell --timeout 900
  cornercase read --pane \"$pane\" --lines 40
  cornercase start claude --worktree fix-login --prompt 'Fix the login form' --wait";
const STATUS_HELP: &str = "Each pane shows its program and folder, and for a coding agent what it is doing (working,
waiting for an answer, done out of sight, idle), its model and how full its context is. `(you)`
marks the pane running the command, `(shown)` what the window shows. `working (background shell)`
is a Claude Code agent whose turn is over while a shell it started in the background still runs:
it wakes up when that shell ends (`background_shell` in the JSON, where the status stays working).

Examples:
  cornercase status
  cornercase status --json";
const OPEN_HELP: &str = "Examples:
  cornercase open ~/src/shop
  cornercase open . --focus";
const NEW_WORKSPACE_HELP: &str =
    "The workspace gets one tab. A worktree goes in the worktrees folder from settings, on a new
branch from HEAD when the branch does not exist yet; the command returns once git made it.

Examples:
  cornercase new-workspace experiments
  cornercase new-workspace fix/login --worktree --project 3";
const COMMAND_HELP: &str =
    "The words of COMMAND are joined with spaces and typed at the prompt once the shell is ready,
as ssh does, so quote what the shell should read as one argument. The command returns once it
is typed.";
const NEW_TAB_HELP: &str = "Examples:
  pane=$(cornercase new-tab --name server -- npm run dev)
  cornercase new-tab --workspace 5 -- 'cargo test 2>&1 | tee test.log'";
const SPLIT_HELP: &str = "Examples:
  cornercase split -- htop
  cornercase split --pane 7 --down -- tail -f server.log";
const START_HELP: &str = "The agent starts with its command and arguments from settings → agents. If it asks whether
you trust the folder, the question waits for your answer unless settings → agents accept it for
you. Once it is ready, the prompt is pasted and
submitted, and the command prints the pane's id; it fails if the agent never shows up. With
--wait, a second line says how the wait ended: idle, done (it finished out of sight) or waiting
(it needs an answer); with --until it waits for that state instead, as `cornercase wait --until` does.

Examples:
  cornercase start claude --worktree fix-login --prompt 'Fix the login form, then commit'
  cornercase start codex --prompt-file task.md --wait --timeout 1800";
const SEND_HELP: &str = "The text goes in as one paste, bracketed when the program asked for it. A pane whose agent
waits for an answer to a question or a permission prompt is refused, since the text would answer
it; use `cornercase keys` for that. With --wait, the command fails if the agent does not start
working within 10 seconds of the Enter, and otherwise prints how the wait ended; with --until it waits
for that state instead, as `cornercase wait --until` does.

Examples:
  cornercase send --pane 12 --enter 'Now add tests for it'
  cornercase send --pane 12 --enter --wait --timeout 900 - < next-step.md
  cornercase send --pane 12 --enter --wait --until turn-over 'Run the tests'";
const KEYS_HELP: &str = "Keys are encoded the way the program in the pane asked for.

Examples:
  cornercase keys --pane 12 ctrl+c
  cornercase keys --pane 12 down enter";
const UNTIL_AFTER: &str = "Wait until the pane is in this state instead";
const KEY_NAMES: &str = "enter, esc, tab, backspace, space, up, down, left, right, home, end, pageup,
pagedown, delete, insert, f1 to f12 or one character, each after any of ctrl+, alt+ and shift+";
const READ_HELP: &str = "Lines the terminal wrapped come back joined, and empty lines at the end are left out.

Examples:
  cornercase read --pane 12
  cornercase read --pane 7 --lines 200 > build.log";
const WAIT_HELP: &str = "By default it waits until the agent stops working: idle, done or waiting. A Claude Code agent
whose turn is over while a background shell it started still runs counts as working, since it
wakes up when the shell ends; --until turn-over also ends there, and prints shell. It then prints
how it ended (idle, done, waiting, working, shell or quiet), or the line that matched. Waiting for
an agent fails on a pane without one. A timeout exits with 1 and does not prove that the agent missed
what you sent: read the pane before sending it again.

Examples:
  cornercase wait --pane 12 --timeout 600
  cornercase wait --pane 7 --until shell
  cornercase wait --pane 12 --until turn-over --timeout 600
  cornercase wait --pane 7 --text 'test result: (ok|FAILED)'";
const CLOSE_HELP: &str = "Its shells stop, as with its ×, but nothing asks first, not even for a project. A worktree's
workspace stays listed while its worktree exists, and its branch is never deleted.

Examples:
  cornercase close --tab 9
  cornercase close --workspace 5 --remove-worktree
  cornercase close --worktree fix/login --remove-worktree";
const RENAME_HELP: &str = "Examples:
  cornercase rename 'review #42'
  cornercase rename --workspace 5 ''";
const FOCUS_HELP: &str = "Examples:
  cornercase focus --pane 12";
const NOTIFY_HELP: &str = "The desktop notification goes through the terminal of every open window, like the one for an
agent that needs you.

Examples:
  cornercase notify the release build is ready";
const TODO_HELP: &str = "Examples:
  cornercase todo add review the login fix
  cornercase todo list
  cornercase todo done 3";
const SKILL_HELP: &str = "Install them for Claude Code, Codex and other agents with
  npx skills add usecornercase/cornercase-terminal --skill cornercase -g
or put this text in an AGENTS.md or CLAUDE.md.";
const REMOTE_HELP: &str = "cornercase runs `ssh DESTINATION cornercase proxy`, so your ssh config, keys, agent and
ProxyJump apply, and starts the server there if none is running. The shells, agents and files stay
on that machine; this one only shows them, and copies and notifications go through this terminal.
If the connection drops, the window stays and reconnects by itself; Esc or quit gives up. Both
machines need the same version of cornercase. Reconnecting never asks for a password, so use a
key or an agent.

Examples:
  cornercase remote devbox
  cornercase remote me@10.0.0.5 --command '~/bin/cornercase'";
const LOGS_HELP: &str = "The server writes one line per event: the time in UTC, the level, where it happened and what,
then key=value details. It says which windows attached, which commands ran and how they were
answered, what started and stopped, how long background jobs took, what each agent was doing and
each step of starting one. It never holds what is typed, pasted or shown in a pane, nor tokens.

The log is server.log in the state folder ($XDG_STATE_HOME/cornercase, else ~/.local/state/cornercase),
or next to the socket when CORNERCASE_SOCKET is set. Past 8 MiB it moves to server.log.1 and starts
over. Start the server with CORNERCASE_LOG=debug for more: every event it handled, every git run,
slow frames.

Examples:
  cornercase logs -n 50
  cornercase logs --follow
  cornercase kill-server && CORNERCASE_LOG=debug cornercase";
const HERE_PANE: &str = "The pane [default: the one this runs in, else the shown one]";
const HERE_WORKSPACE: &str = "The workspace [default: the one this runs in, else the shown one]";
const BY_BRANCH: &str = "The workspace on this branch, looked up in the project this runs in, else the shown one";

#[derive(Debug, Parser)]
#[command(name = "cornercase", version, about = ABOUT, long_about = LONG_ABOUT, after_help = AFTER_HELP)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(flatten)]
    Control(Control),
    #[command(
        about = "Open the window on the cornercase server of another machine, over ssh",
        after_help = REMOTE_HELP
    )]
    Remote(RemoteArgs),
    #[command(about = "Print the instructions that teach coding agents these commands", after_help = SKILL_HELP)]
    Skill,
    #[command(about = "Install the latest release, then offer to restart the server")]
    Update(UpdateArgs),
    #[command(
        about = "Restart the server: every program in its terminals stops, and the session comes back with new shells"
    )]
    Restart(RestartArgs),
    #[command(about = "Stop the server and every shell in it")]
    KillServer,
    #[command(about = "Print the server's log: what it did, step by step", after_help = LOGS_HELP)]
    Logs(LogsArgs),
    #[command(hide = true)]
    Server,
    #[command(hide = true)]
    Proxy,
}

#[derive(Debug, Subcommand)]
pub enum Control {
    #[command(about = "List the projects, workspaces, tabs and panes, with their ids", after_help = STATUS_HELP)]
    Status(Print),
    #[command(about = "Open a folder as a project, or find the open one, and print its id", after_help = OPEN_HELP)]
    Open(OpenArgs),
    #[command(
        about = "Add a workspace to a project, or a git worktree, and print its id",
        after_help = NEW_WORKSPACE_HELP
    )]
    NewWorkspace(NewWorkspaceArgs),
    #[command(
        about = "Open a tab, optionally typing a command into it, and print its pane's id",
        long_about = COMMAND_HELP,
        after_help = NEW_TAB_HELP
    )]
    NewTab(NewTabArgs),
    #[command(
        about = "Split a pane right or down, optionally typing a command, and print the new pane's id",
        long_about = COMMAND_HELP,
        after_help = SPLIT_HELP
    )]
    Split(SplitArgs),
    #[command(
        about = "Start a coding agent in a new tab, give it a prompt, and print its pane's id",
        after_help = START_HELP
    )]
    Start(StartArgs),
    #[command(about = "Paste text into a pane, and press Enter with --enter", after_help = SEND_HELP)]
    Send(SendArgs),
    #[command(about = "Press keys in a pane", after_help = KEYS_HELP)]
    Keys(KeysArgs),
    #[command(about = "Print the screen of a pane as text, or its last lines", after_help = READ_HELP)]
    Read(ReadArgs),
    #[command(
        about = "Wait until an agent stops working, a program ends, text shows up or the output stops",
        after_help = WAIT_HELP
    )]
    Wait(WaitArgs),
    #[command(about = "Close a pane, tab, workspace or project", after_help = CLOSE_HELP)]
    Close(CloseArgs),
    #[command(about = "Rename a tab, workspace, project or group", after_help = RENAME_HELP)]
    Rename(RenameArgs),
    #[command(about = "Show a pane, tab, workspace or project in the window", after_help = FOCUS_HELP)]
    Focus(FocusArgs),
    #[command(about = "Show a message in the window, with a desktop notification", after_help = NOTIFY_HELP)]
    Notify(NotifyArgs),
    #[command(about = "Add, list, check off and remove items of the TODO list", after_help = TODO_HELP)]
    Todo(TodoArgs),
}

#[derive(Debug, Args)]
pub struct Print {
    #[arg(long, help = "Print JSON instead of text")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct Create {
    #[arg(long, help = "Also show it in the window, which otherwise stays as it is")]
    pub focus: bool,
    #[command(flatten)]
    pub print: Print,
}

#[derive(Debug, Args)]
pub struct Target {
    #[arg(long, value_name = "ID", help = HERE_PANE)]
    pub pane: Option<u64>,
    #[arg(long, value_name = "ID", conflicts_with = "pane", help = "A tab: its agent's pane, else its active one")]
    pub tab: Option<u64>,
}

#[derive(Debug, Args)]
pub struct OpenArgs {
    #[arg(value_name = "PATH", help = "The folder")]
    pub path: PathBuf,
    #[command(flatten)]
    pub create: Create,
}

#[derive(Debug, Args)]
pub struct NewWorkspaceArgs {
    #[arg(value_name = "NAME", help = "Its name, or its branch with --worktree")]
    pub name: String,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub project: Option<u64>,
    #[arg(long, help = "Check the branch out in a git worktree of its own, at a repository's root only")]
    pub worktree: bool,
    #[command(flatten)]
    pub create: Create,
}

#[derive(Debug, Args)]
pub struct NewTabArgs {
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with = "workspace", help = BY_BRANCH)]
    pub worktree: Option<String>,
    #[arg(long, help = "The tab's name [default: the name of the program it runs]")]
    pub name: Option<String>,
    #[command(flatten)]
    pub create: Create,
    #[arg(last = true, value_name = "COMMAND", help = "What to type into its shell")]
    pub command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SplitArgs {
    #[arg(long, value_name = "ID", help = HERE_PANE)]
    pub pane: Option<u64>,
    #[arg(long, help = "Split it down instead of right")]
    pub down: bool,
    #[command(flatten)]
    pub create: Create,
    #[arg(last = true, value_name = "COMMAND", help = "What to type into the new shell")]
    pub command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct StartArgs {
    #[arg(
        value_name = "AGENT",
        help = "The agent, such as claude or codex [default: the one chosen in settings → agents, else the one \
                running here]"
    )]
    pub agent: Option<String>,
    #[arg(
        long,
        value_name = "BRANCH",
        conflicts_with = "workspace",
        help = "Start it in the worktree of this branch, made when missing"
    )]
    pub worktree: Option<String>,
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub workspace: Option<u64>,
    #[arg(long, help = "The tab's name [default: the agent's]")]
    pub name: Option<String>,
    #[arg(long, value_name = "TEXT", conflicts_with = "prompt_file", help = "What to ask it")]
    pub prompt: Option<String>,
    #[arg(long, value_name = "FILE", help = "Read the prompt from a file, or from standard input with -")]
    pub prompt_file: Option<PathBuf>,
    #[arg(long, help = "Then wait until the agent stops working, and print how it ended")]
    pub wait: bool,
    #[arg(long, value_enum, requires = "wait", help = UNTIL_AFTER)]
    pub until: Option<UntilArg>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Give up after this long, with status 1")]
    pub timeout: Option<f64>,
    #[command(flatten)]
    pub create: Create,
}

#[derive(Debug, Args)]
pub struct SendArgs {
    #[command(flatten)]
    pub target: Target,
    #[arg(long, help = "Press Enter once the screen settles")]
    pub enter: bool,
    #[arg(long, requires = "enter", help = "Then wait until the agent stops working, and print how it ended")]
    pub wait: bool,
    #[arg(long, value_enum, requires = "wait", help = UNTIL_AFTER)]
    pub until: Option<UntilArg>,
    #[arg(
        long,
        value_name = "SECONDS",
        requires = "wait",
        value_parser = seconds,
        help = "Give up waiting after this long, with status 1"
    )]
    pub timeout: Option<f64>,
    #[command(flatten)]
    pub print: Print,
    #[arg(
        value_name = "TEXT",
        required_unless_present = "enter",
        help = "The text, or - to read it from standard input"
    )]
    pub text: Option<String>,
}

#[derive(Debug, Args)]
pub struct KeysArgs {
    #[command(flatten)]
    pub target: Target,
    #[arg(required = true, value_name = "KEY", value_parser = key, help = KEY_NAMES)]
    pub keys: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ReadArgs {
    #[command(flatten)]
    pub target: Target,
    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "The last N lines, scrollback included (a pane keeps 5,000)"
    )]
    pub lines: Option<u64>,
    #[command(flatten)]
    pub print: Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum UntilArg {
    #[value(help = "The agent is idle or done")]
    Idle,
    #[value(help = "The agent is working")]
    Working,
    #[value(help = "The agent waits for an answer to a question or a permission prompt")]
    Waiting,
    #[value(help = "The program in the foreground ended and the shell is back")]
    Shell,
    #[value(help = "The agent's turn is over: it stopped working, or only a background shell it started still runs")]
    TurnOver,
}

impl From<UntilArg> for Until {
    fn from(until: UntilArg) -> Self {
        match until {
            UntilArg::Idle => Self::Idle,
            UntilArg::Working => Self::Working,
            UntilArg::Waiting => Self::Waiting,
            UntilArg::Shell => Self::Shell,
            UntilArg::TurnOver => Self::TurnOver,
        }
    }
}

#[derive(Debug, Args)]
pub struct WaitArgs {
    #[command(flatten)]
    pub target: Target,
    #[arg(long, value_enum, conflicts_with_all = ["text", "quiet"], help = "Until the pane is in this state")]
    pub until: Option<UntilArg>,
    #[arg(
        long,
        value_name = "REGEX",
        conflicts_with = "quiet",
        value_parser = pattern,
        help = "Until a line on its screen matches, one already there included"
    )]
    pub text: Option<String>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Until it writes nothing for this long")]
    pub quiet: Option<f64>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Give up after this long, with status 1")]
    pub timeout: Option<f64>,
    #[command(flatten)]
    pub print: Print,
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct Which {
    #[arg(long, value_name = "ID", help = "A pane")]
    pub pane: Option<u64>,
    #[arg(long, value_name = "ID", help = "A tab and its panes")]
    pub tab: Option<u64>,
    #[arg(long, value_name = "ID", help = "A workspace and its tabs")]
    pub workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, help = BY_BRANCH)]
    pub worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "A project and its workspaces")]
    pub project: Option<u64>,
}

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

#[derive(Debug, Args)]
pub struct CloseArgs {
    #[command(flatten)]
    pub which: Which,
    #[arg(
        long,
        conflicts_with_all = ["pane", "tab", "project"],
        help = "Also remove the workspace's git worktree from disk"
    )]
    pub remove_worktree: bool,
    #[arg(long, requires = "remove_worktree", help = "Remove it even when git would lose changes")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct FocusArgs {
    #[command(flatten)]
    pub which: Which,
}

#[derive(Debug, Args)]
#[group(multiple = false)]
pub struct Renamed {
    #[arg(long, value_name = "ID", help = "The tab [default: the one this runs in, else the shown one]")]
    pub tab: Option<u64>,
    #[arg(long, value_name = "ID", help = "Rename this workspace instead")]
    pub workspace: Option<u64>,
    #[arg(
        long,
        value_name = "BRANCH",
        value_parser = branch,
        help = "Rename the workspace on this branch instead, looked up in the project this runs in, else the shown one"
    )]
    pub worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "Rename this project instead")]
    pub project: Option<u64>,
    #[arg(long, value_name = "ID", help = "Rename this group instead")]
    pub group: Option<u64>,
}

#[derive(Debug, Args)]
pub struct RenameArgs {
    #[command(flatten)]
    pub renamed: Renamed,
    #[arg(value_name = "NAME", help = "The new name; an empty one goes back to the default")]
    pub name: String,
}

#[derive(Debug, Args)]
pub struct NotifyArgs {
    #[arg(required = true, value_name = "TEXT", help = "The message")]
    pub text: Vec<String>,
}

#[derive(Debug, Args)]
pub struct TodoArgs {
    #[command(subcommand)]
    pub action: TodoAction,
}

#[derive(Debug, Subcommand)]
pub enum TodoAction {
    #[command(about = "Add an item and print its id")]
    Add {
        #[arg(required = true, value_name = "TEXT", help = "What to do")]
        text: Vec<String>,
    },
    #[command(about = "List the items with their ids, pending ones first")]
    List(Print),
    #[command(about = "Check an item off, or back on")]
    Done {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
    },
    #[command(about = "Remove an item")]
    Rm {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
    },
}

#[derive(Debug, Args)]
pub struct RemoteArgs {
    #[arg(value_name = "DESTINATION", help = "Where ssh connects: a host from ~/.ssh/config, or user@host")]
    pub destination: String,
    #[arg(
        long,
        value_name = "COMMAND",
        help = "How to run cornercase there, as sh reads it [default: cornercase, also looked for in \
                ~/.local/bin, ~/.cargo/bin and Homebrew's folders]"
    )]
    pub command: Option<String>,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(short = 'n', long, value_name = "N", default_value_t = 200, help = "How many of the last lines to print")]
    pub lines: usize,
    #[arg(short, long, help = "Keep printing what the server writes, until interrupted")]
    pub follow: bool,
    #[arg(long, conflicts_with_all = ["lines", "follow"], help = "Print only where the log is")]
    pub path: bool,
}

#[derive(Debug, Args)]
pub struct RestartArgs {
    #[arg(short, long, help = "Restart the server without asking")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    #[arg(long, help = "Only say whether a newer version is out")]
    pub check: bool,
    #[arg(short, long, help = "Restart the server without asking")]
    pub yes: bool,
}

fn seconds(text: &str) -> std::result::Result<f64, String> {
    match text.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => Ok(n),
        _ => Err("use a positive number of seconds".into()),
    }
}

fn pattern(text: &str) -> std::result::Result<String, String> {
    regex::Regex::new(text).map(|_| text.to_string()).map_err(|e| e.to_string())
}

fn branch(text: &str) -> std::result::Result<String, String> {
    let branch = text.trim();
    if branch.is_empty() { Err("the branch needs a name".into()) } else { Ok(branch.to_string()) }
}

fn key(text: &str) -> std::result::Result<String, String> {
    keys::named(text).map(|_| text.to_string()).ok_or_else(|| format!("unknown key; use {KEY_NAMES}"))
}

pub fn run(cli: Cli) -> Result<bool> {
    let Some(command) = cli.command else {
        client::run()?;
        return Ok(true);
    };
    match command {
        Command::Control(control) => run_control(control)?,
        Command::Remote(args) => client::remote(&Remote { destination: args.destination, command: args.command })?,
        Command::Skill => print!("{SKILL}"),
        Command::Update(update) => return client::update(update.check, update.yes),
        Command::Restart(restart) => client::restart(restart.yes)?,
        Command::KillServer => {
            let running = client::running_now();
            if !client::kill_server()? {
                eprintln!("no cornercase server is running");
            } else if let Some(line) = running.as_deref().and_then(restart::stopped) {
                println!("{line}");
            }
        }
        Command::Logs(args) => return print_logs(&args),
        Command::Server => server::run()?,
        Command::Proxy => remote::proxy()?,
    }
    Ok(true)
}

fn print_logs(args: &LogsArgs) -> Result<bool> {
    let path = log::path();
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

fn run_control(command: Control) -> Result<()> {
    match command {
        Control::Status(print) => {
            let value = ask("status", control::Command::Status(control::Status {}))?;
            if print.json {
                return print_json(&value);
            }
            let report: Report = answer(value)?;
            print!("{}", render(&report, home().as_deref()));
            Ok(())
        }
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
            };
            say(ask("send", control::Command::Send(request))?, send.print.json, ending)
        }
        Control::Keys(keys) => {
            let Target { pane, tab } = keys.target;
            ask("keys", control::Command::Keys(control::Keys { pane, tab, keys: keys.keys })).map(drop)
        }
        Control::Read(read) => {
            let Target { pane, tab } = read.target;
            let lines = read.lines.and_then(|n| usize::try_from(n).ok());
            let value = ask("read", control::Command::Read(control::Read { pane, tab, lines }))?;
            say(value, read.print.json, |done| done.text.clone().into_iter().collect())
        }
        Control::Wait(wait) => {
            let until = match (wait.until, wait.text, wait.quiet) {
                (Some(until), ..) => until.into(),
                (None, Some(text), _) => Until::Text(text),
                (None, None, Some(quiet)) => Until::Quiet(quiet),
                (None, None, None) => Until::Stops,
            };
            let Target { pane, tab } = wait.target;
            let request = control::Wait { pane, tab, until, timeout: wait.timeout };
            say(ask("wait", control::Command::Wait(request))?, wait.print.json, ending)
        }
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
        Control::Todo(todo) => run_todo(todo.action),
    }
}

fn workspace(command: &'static str, id: Option<u64>, branch: Option<&str>) -> Result<Option<u64>> {
    let Some(branch) = branch else { return Ok(id) };
    let report: Report = answer(ask(command, control::Command::Status(control::Status {}))?)?;
    report.workspace_on(branch).map(Some).map_err(Error::WrongUsage)
}

fn run_start(start: StartArgs) -> Result<()> {
    let prompt = match (start.prompt, start.prompt_file) {
        (Some(prompt), _) => Some(prompt),
        (None, Some(file)) if file == Path::new("-") => Some(stdin()?),
        (None, Some(file)) => Some(trimmed(
            &std::fs::read_to_string(&file)
                .map_err(|e| Error::Control(format!("cannot read the prompt from `{}`: {e}", file.display())))?,
        )),
        (None, None) => None,
    };
    let request = control::Start {
        agent: start.agent,
        worktree: start.worktree,
        workspace: start.workspace,
        name: start.name,
        prompt,
        wait: start.wait,
        until: start.until.map_or(Until::Stops, Until::from),
        timeout: start.timeout,
        focus: start.create.focus,
    };
    say(ask("start", control::Command::Start(request))?, start.create.print.json, |done| {
        id(done.ids.pane).into_iter().chain(ending(done)).collect()
    })
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

fn print_json(value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::Control(e.to_string()))?;
    println!("{text}");
    Ok(())
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

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn render(report: &Report, home: Option<&Path>) -> String {
    let mut lines = Vec::new();
    let grouped = |p: &&ProjectInfo| p.group.is_some_and(|g| report.groups.iter().any(|group| group.id == g));
    for project in report.projects.iter().filter(|p| !grouped(p)) {
        project_lines(&mut lines, report, project, 0, home);
    }
    for group in &report.groups {
        let state = if group.collapsed { "collapsed" } else { "" };
        lines.push(line(0, "group", group.id, &[group.name.clone(), state.into()], &[]));
        for project in report.projects.iter().filter(|p| p.group == Some(group.id)) {
            project_lines(&mut lines, report, project, 1, home);
        }
    }
    lines.into_iter().map(|line| line + "\n").collect()
}

fn project_lines(lines: &mut Vec<String>, report: &Report, project: &ProjectInfo, depth: usize, home: Option<&Path>) {
    let shown = report.shown;
    let place = |path: &Path| ui::display_path(path, home);
    let marks = |is_shown: bool| if is_shown { vec!["shown"] } else { Vec::new() };
    let parts = [project.name.clone(), place(&project.path)];
    lines.push(line(depth, "project", project.id, &parts, &marks(shown.project == Some(project.id))));
    for workspace in &project.workspaces {
        let mut parts = vec![workspace.name.clone(), place(&workspace.path)];
        parts.extend(workspace.branch.as_ref().map(|branch| format!("branch {branch}")));
        parts.extend(workspace.worktree.then(|| "worktree".to_string()));
        parts.extend((workspace.behind > 0).then(|| format!("{} behind", workspace.behind)));
        lines.push(line(depth + 1, "workspace", workspace.id, &parts, &marks(shown.workspace == Some(workspace.id))));
        for tab in &workspace.tabs {
            let parts = [tab.name.clone(), tab.status.clone().unwrap_or_default()];
            lines.push(line(depth + 2, "tab", tab.id, &parts, &marks(shown.tab == Some(tab.id))));
            for pane in &tab.panes {
                let details = match (&pane.model, pane.context) {
                    (Some(model), Some(percent)) => format!("{model} · {percent}%"),
                    (Some(model), None) => model.clone(),
                    (None, Some(percent)) => format!("{percent}%"),
                    (None, None) => String::new(),
                };
                let status = pane.status.clone().unwrap_or_default();
                let parts = [
                    pane.program.clone().unwrap_or_else(|| "?".into()),
                    if pane.background_shell { format!("{status} (background shell)") } else { status },
                    details,
                    pane.path.as_deref().map(place).unwrap_or_default(),
                ];
                let mut marked = marks(shown.pane == Some(pane.id));
                marked.extend(pane.caller.then_some("you"));
                lines.push(line(depth + 3, "pane", pane.id, &parts, &marked));
            }
        }
    }
}

fn line(depth: usize, kind: &str, id: u64, parts: &[String], marks: &[&str]) -> String {
    let mut text = format!("{}{kind} {id}", "  ".repeat(depth));
    for part in parts.iter().filter(|part| !part.is_empty()) {
        text.push_str("  ");
        text.push_str(part);
    }
    if !marks.is_empty() {
        let _ = write!(text, "  ({})", marks.join(", "));
    }
    text
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use rstest::rstest;

    use super::*;
    use crate::control::{GroupInfo, Ids, PaneInfo, TabInfo, WorkspaceInfo};

    fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
    }

    mod parsing {
        use super::*;

        #[test]
        fn the_definitions_are_consistent() {
            Cli::command().debug_assert();
        }

        #[test]
        fn no_command_opens_the_window() {
            assert!(parse(&[]).expect("parse").command.is_none());
        }

        #[test]
        fn a_command_after_two_dashes_keeps_its_words() {
            let cli = parse(&["new-tab", "--name", "tests", "--", "cargo", "test", "--locked"]).expect("parse");

            let Some(Command::Control(Control::NewTab(new))) = cli.command else { panic!("not new-tab") };
            assert_eq!((new.name.as_deref(), typed(&new.command)), (Some("tests"), Some("cargo test --locked".into())));
        }

        #[test]
        fn a_dash_reads_the_text_from_standard_input() {
            let cli = parse(&["send", "--pane", "4", "--enter", "-"]).expect("parse");

            let Some(Command::Control(Control::Send(send))) = cli.command else { panic!("not send") };
            assert_eq!((send.target.pane, send.text.as_deref()), (Some(4), Some("-")));
        }

        #[test]
        fn the_old_commands_stay() {
            let update = parse(&["update", "--check", "-y"]).expect("parse");
            let kill = parse(&["kill-server"]).expect("parse");
            let restart = parse(&["restart", "--yes"]).expect("parse");

            assert!(matches!(update.command, Some(Command::Update(UpdateArgs { check: true, yes: true }))));
            assert!(matches!(kill.command, Some(Command::KillServer)));
            assert!(matches!(restart.command, Some(Command::Restart(RestartArgs { yes: true }))));
        }

        #[rstest]
        #[case::wait_without_enter(&["send", "--wait", "hi"])]
        #[case::sending_until_without_waiting(&["send", "--enter", "--until", "turn-over", "hi"])]
        #[case::starting_until_without_waiting(&["start", "--until", "turn-over"])]
        #[case::nothing_to_send(&["send", "--pane", "1"])]
        #[case::a_pane_and_a_tab(&["read", "--pane", "1", "--tab", "2"])]
        #[case::nothing_to_close(&["close"])]
        #[case::two_things_to_close(&["close", "--pane", "1", "--tab", "2"])]
        #[case::removing_the_worktree_of_a_tab(&["close", "--tab", "1", "--remove-worktree"])]
        #[case::forcing_without_removing(&["close", "--workspace", "1", "--force"])]
        #[case::two_conditions(&["wait", "--until", "idle", "--text", "x"])]
        #[case::an_unknown_state(&["wait", "--until", "sleeping"])]
        #[case::a_broken_pattern(&["wait", "--text", "("])]
        #[case::a_negative_timeout(&["wait", "--timeout", "-1"])]
        #[case::no_lines(&["read", "--lines", "0"])]
        #[case::an_unknown_key(&["keys", "hello"])]
        #[case::two_prompts(&["start", "--prompt", "a", "--prompt-file", "b"])]
        #[case::a_worktree_and_a_workspace(&["start", "--worktree", "x", "--workspace", "1"])]
        #[case::two_things_to_rename(&["rename", "--tab", "1", "--group", "2", "x"])]
        #[case::a_branch_and_a_workspace_to_close(&["close", "--worktree", "x", "--workspace", "1"])]
        #[case::a_branch_and_a_tab_to_show(&["focus", "--worktree", "x", "--tab", "1"])]
        #[case::a_branch_and_a_workspace_for_a_tab(&["new-tab", "--worktree", "x", "--workspace", "1"])]
        #[case::a_branch_and_a_project_to_rename(&["rename", "--worktree", "x", "--project", "1", "y"])]
        #[case::a_blank_branch(&["close", "--worktree", " "])]
        #[case::an_unknown_command(&["frobnicate"])]
        fn wrong_usage_exits_with_2(#[case] args: &[&str]) {
            assert_eq!(parse(args).expect_err("wrong usage").exit_code(), 2);
        }

        #[rstest]
        #[case::wait(&["wait", "--until", "turn-over"])]
        #[case::send(&["send", "--enter", "--wait", "--until", "turn-over", "hi"])]
        #[case::start(&["start", "claude", "--wait", "--until", "turn-over"])]
        fn turn_over_is_a_state_to_wait_for(#[case] args: &[&str]) {
            let until = match parse(args).expect("parse").command {
                Some(Command::Control(Control::Wait(wait))) => wait.until,
                Some(Command::Control(Control::Send(send))) => send.until,
                Some(Command::Control(Control::Start(start))) => start.until,
                _ => None,
            };

            assert_eq!(until.map(Until::from), Some(Until::TurnOver));
        }

        #[test]
        fn a_worktree_is_closed_by_its_branch() {
            let cli = parse(&["close", "--worktree", " fix/login ", "--remove-worktree", "--force"]).expect("parse");

            let Some(Command::Control(Control::Close(close))) = cli.command else { panic!("not close") };
            assert_eq!(close.which.worktree.as_deref(), Some("fix/login"));
            assert!(close.remove_worktree && close.force);
        }

        #[test]
        fn the_server_stays_out_of_the_help() {
            let help = Cli::command().render_help().to_string();

            assert!(!help.lines().any(|line| line.trim_start().starts_with("server ")), "{help}");
        }
    }

    mod output {
        use super::*;

        fn report() -> Report {
            let pane = |id, program: &str, active| PaneInfo {
                id,
                path: Some(PathBuf::from("/home/ana/shop")),
                program: Some(program.into()),
                active,
                ..PaneInfo::default()
            };
            let claude = PaneInfo {
                agent: Some("claude".into()),
                status: Some("working".into()),
                model: Some("Opus 5.5".into()),
                context: Some(23),
                caller: true,
                ..pane(5, "claude", false)
            };
            let tab = TabInfo {
                id: 3,
                name: "claude".into(),
                status: Some("working".into()),
                active: true,
                panes: vec![pane(4, "zsh", true), claude],
            };
            let workspace = WorkspaceInfo {
                id: 2,
                name: "fix/login".into(),
                path: PathBuf::from("/home/ana/.cornercase/worktrees/shop/fix-login"),
                branch: Some("fix/login".into()),
                worktree: true,
                behind: 2,
                active: true,
                tabs: vec![tab],
            };
            let project = |id, name: &str, group| ProjectInfo {
                id,
                name: name.into(),
                path: format!("/home/ana/{name}").into(),
                group,
                ..ProjectInfo::default()
            };
            Report {
                version: "0.9.0".into(),
                caller: Some(5),
                shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
                groups: vec![GroupInfo { id: 8, name: "work".into(), collapsed: true }],
                projects: vec![
                    ProjectInfo { workspaces: vec![workspace], ..project(1, "shop", None) },
                    project(9, "api", Some(8)),
                ],
            }
        }

        #[test]
        fn status_is_a_tree_of_ids_with_what_matters() {
            let text = render(&report(), Some(Path::new("/home/ana")));

            assert_eq!(
                text,
                "project 1  shop  ~/shop  (shown)\n\
                 \x20 workspace 2  fix/login  ~/.cornercase/worktrees/shop/fix-login  branch fix/login  worktree  2 behind  (shown)\n\
                 \x20   tab 3  claude  working  (shown)\n\
                 \x20     pane 4  zsh  ~/shop  (shown)\n\
                 \x20     pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (you)\n\
                 group 8  work  collapsed\n\
                 \x20 project 9  api  ~/api\n"
            );
        }

        #[test]
        fn an_agent_left_with_a_background_shell_still_reads_working() {
            let mut report = report();
            report.projects[0].workspaces[0].tabs[0].panes[1].background_shell = true;

            let text = render(&report, Some(Path::new("/home/ana")));

            assert!(
                text.contains("pane 5  claude  working (background shell)  Opus 5.5 · 23%  ~/shop  (you)\n"),
                "{text}"
            );
        }

        #[rstest]
        #[case::a_state(Some("idle"), None, &["idle"])]
        #[case::a_matching_line(Some("text"), Some("test result: ok"), &["test result: ok"])]
        #[case::no_wait(None, None, &[])]
        fn the_ending_is_the_state_or_the_line(
            #[case] ended: Option<&str>,
            #[case] line: Option<&str>,
            #[case] expected: &[&str],
        ) {
            let done = Done { ended: ended.map(str::to_string), line: line.map(str::to_string), ..Done::default() };

            assert_eq!(ending(&done), expected);
        }
    }
}
