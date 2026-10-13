use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use super::help::{
    ABOUT, AFTER_HELP, BY_BRANCH, CLOSE_HELP, COMMAND_HELP, EVENTS_HELP, FOCUS_HELP, HERE_PANE, HERE_WORKSPACE,
    KEY_NAMES, KEYS_HELP, LOGS_HELP, LONG_ABOUT, NEW_TAB_HELP, NEW_WORKSPACE_HELP, NOTIFY_HELP, OPEN_HELP, READ_HELP,
    REMOTE_HELP, RENAME_HELP, RESTART_HELP, RESTART_TIMEOUT, SEND_HELP, SKILL_HELP, SPLIT_HELP, START_HELP,
    STATUS_HELP, TODO_HELP, UNTIL_AFTER, UPDATE_HELP, WAIT_HELP, WAIT_PANE, WAIT_TAB, WHEN_IDLE,
};
use crate::control::Until;
use crate::term::keys;

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
    #[command(about = "Install the latest release, then offer to restart the server", after_help = UPDATE_HELP)]
    Update(UpdateArgs),
    #[command(
        about = "Restart the server: every program in its terminals stops, and the session comes back with new shells",
        after_help = RESTART_HELP
    )]
    Restart(RestartArgs),
    #[command(about = "Stop the server and every shell in it")]
    KillServer,
    #[command(about = "Print the server's log: what it did, step by step", after_help = LOGS_HELP)]
    Logs(LogsArgs),
    #[command(
        about = "Show work by agents running in cornercase",
        after_help = "Counts only agents in cornercase, from when counting started. With no server, reads stats.json.

Examples:
  cornercase stats
  cornercase stats --json
  cornercase stats --share"
    )]
    Stats(StatsArgs),
    #[command(hide = true)]
    Server,
    #[command(hide = true)]
    Proxy,
}

#[derive(Debug, Subcommand)]
pub enum Control {
    #[command(about = "List the projects, workspaces, tabs and panes, with their ids", after_help = STATUS_HELP)]
    Status(StatusArgs),
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
    #[command(about = "Follow agents, programs, panes and tabs as they change, one line per event", after_help = EVENTS_HELP)]
    Events(EventsArgs),
}

#[derive(Debug, Args)]
pub struct Print {
    #[arg(long, help = "Print JSON instead of text")]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, help = "One line per pane, its columns separated by tabs, for scripts")]
    pub panes: bool,
    #[command(flatten)]
    pub print: Print,
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
        requires = "enter",
        value_parser = seconds,
        help = "Give up waiting after this long, with status 1"
    )]
    pub timeout: Option<f64>,
    #[arg(
        long,
        help = "Send even when the agent shows a dialog, a panel or its shell mode instead of its input box, or its \
                feedback survey"
    )]
    pub force: bool,
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
    #[arg(
        long = "pane",
        value_name = "ID",
        help = "The pane; with --last-message, repeat it to read several [default: the one this runs in, else the shown one]"
    )]
    pub panes: Vec<u64>,
    #[arg(
        long = "tab",
        value_name = "ID",
        help = "A tab: its agent's pane, else its active one; with --last-message, repeat it"
    )]
    pub tabs: Vec<u64>,
    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "The last N lines, scrollback included (a pane keeps its last 10 MB of output)"
    )]
    pub lines: Option<u64>,
    #[arg(long, conflicts_with = "lines", help = "The last message of the agent in the pane, from its own record")]
    pub last_message: bool,
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
    #[arg(long = "pane", value_name = "ID", help = WAIT_PANE)]
    pub panes: Vec<u64>,
    #[arg(long = "tab", value_name = "ID", help = WAIT_TAB)]
    pub tabs: Vec<u64>,
    #[arg(long, conflicts_with = "all", help = "Return once any of the panes meets the condition")]
    pub any: bool,
    #[arg(long, help = "Return once every one of the panes has met the condition")]
    pub all: bool,
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
pub struct EventsArgs {
    #[arg(
        long = "pane",
        value_name = "ID",
        num_args = 1..,
        help = "Only what happens to these panes, and stop once they all closed"
    )]
    pub panes: Vec<u64>,
    #[arg(long, help = "Print one JSON object per line instead of text")]
    pub json: bool,
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
    #[command(flatten)]
    pub when: WhenArgs,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    #[arg(long, conflicts_with = "when_idle", help = "Only say whether a newer version is out")]
    pub check: bool,
    #[arg(short, long, help = "Restart the server without asking")]
    pub yes: bool,
    #[command(flatten)]
    pub when: WhenArgs,
}

#[derive(Debug, Args)]
pub struct WhenArgs {
    #[arg(long, help = WHEN_IDLE)]
    pub when_idle: bool,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, requires = "when_idle", help = RESTART_TIMEOUT)]
    pub timeout: Option<f64>,
}

fn seconds(text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => Ok(n),
        _ => Err("use a positive number of seconds".into()),
    }
}

fn pattern(text: &str) -> Result<String, String> {
    regex::Regex::new(text).map(|_| text.to_string()).map_err(|e| e.to_string())
}

fn branch(text: &str) -> Result<String, String> {
    let branch = text.trim();
    if branch.is_empty() { Err("the branch needs a name".into()) } else { Ok(branch.to_string()) }
}

fn key(text: &str) -> Result<String, String> {
    keys::named(text).map(|_| text.to_string()).ok_or_else(|| format!("unknown key; use {KEY_NAMES}"))
}

#[derive(Debug, Args)]
pub struct StatsArgs {
    #[arg(long, conflicts_with = "share", help = "Print every counter and the heatmap blocks as JSON")]
    pub(super) json: bool,
    #[arg(long, help = "Print a link to a card made locally in the browser")]
    pub(super) share: bool,
}
