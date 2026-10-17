pub(crate) mod queue;
pub(crate) mod terminal;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use super::help::{
    ABOUT, AFTER_HELP, AWAY_HELP, BY_BRANCH, CLOSE_HELP, COMMAND_HELP, CONTRACT_HELP, EVENTS_HELP, FLOW_EXPORT_HELP,
    FLOW_HELP, FLOW_IMPORT_HELP, FLOW_RUN_HELP, FOCUS_HELP, HERE_PANE, HERE_WORKSPACE, INPUT_HELP, KEY_NAMES,
    KEYS_HELP, KILL_SERVER_HELP, LOGS_HELP, LONG_ABOUT, MARK_HELP, NEW_TAB_HELP, NEW_WORKSPACE_HELP, NOTIFY_HELP,
    OPEN_HELP, PLUGIN_HELP, RATIO_HELP, READ_HELP, REMOTE_HELP, RENAME_HELP, RESIZE_HELP, RESTART_HELP,
    RESTART_TIMEOUT, SCROLL_HELP, SEARCH_HELP, SELECTION_HELP, SEND_HELP, SKILL_HELP, SPLIT_HELP, START_HELP,
    START_SERVER_HELP, STATUS_HELP, SURFACE_HELP, TODO_HELP, TODO_SEND_HELP, UNTIL_AFTER, UPDATE_HELP, VIEW_ARG,
    VIEW_HELP, VIEW_SET_HELP, WAIT_HELP, WAIT_PANE, WAIT_TAB, WHEN_IDLE,
};
use crate::control::Until;
use crate::term::keys;

#[derive(Debug, Parser)]
#[command(name = "cornercase", version, about = ABOUT, long_about = LONG_ABOUT, after_help = AFTER_HELP)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
#[expect(clippy::large_enum_variant, reason = "parsed once per process")]
pub(crate) enum Command {
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
    #[command(
        about = "Start the server without a window, or find the running one, and restore the session",
        after_help = START_SERVER_HELP
    )]
    StartServer(StartServerArgs),
    #[command(about = "Stop the server and every shell in it", after_help = KILL_SERVER_HELP)]
    KillServer(KillServerArgs),
    #[command(about = "Print the server's log: what it did, step by step", after_help = LOGS_HELP)]
    Logs(LogsArgs),
    #[command(
        about = "Show work by agents running in cornercase",
        after_help = "Counts only agents in cornercase, from when counting started. With no server, reads stats.json. Merged pull requests need a running server and gh or glab.

Examples:
  cornercase stats
  cornercase stats --json
  cornercase stats --share"
    )]
    Stats(StatsArgs),
    #[command(about = "Install, add, list, turn on and off the plugins that extend cornercase", after_help = PLUGIN_HELP)]
    Plugin(PluginArgs),
    #[command(hide = true)]
    Server,
    #[command(hide = true)]
    Proxy,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Control {
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
    #[command(about = "Have agents message you on Telegram or Discord until you turn it off", after_help = AWAY_HELP)]
    Away(AwayArgs),
    #[command(about = "Mark an agent's pane as pending review, or as reviewed", after_help = MARK_HELP)]
    Mark(MarkArgs),
    #[command(about = "Find the panes where some text was said: agent transcripts and every pane's history", after_help = SEARCH_HELP)]
    Search(SearchArgs),
    #[command(about = "Add, list, check off and remove items of the TODO list", after_help = TODO_HELP)]
    Todo(TodoArgs),
    #[command(about = "Schedule issues or prompts for later; queued agents accept folder trust prompts")]
    Queue(queue::QueueArgs),
    #[command(about = "Follow agents, programs, panes and tabs as they change, one line per event", after_help = EVENTS_HELP)]
    Events(EventsArgs),
    #[command(about = "List, start, export or import flows: teams of agents in a worktree, run by an orchestrator agent", after_help = FLOW_HELP)]
    Flow(FlowArgs),
    #[command(
        about = "Print what this server offers to scripts: its version, contract, boot, commands and streams",
        after_help = CONTRACT_HELP
    )]
    Contract(ContractArgs),
    #[command(
        about = "Stream a tab's terminals as cells, with their panes and dividers, for programs that draw them",
        after_help = SURFACE_HELP
    )]
    Surface(SurfaceArgs),
    #[command(
        about = "Open a view of your own: its own tab and size, for programs that draw cornercase themselves",
        after_help = VIEW_HELP
    )]
    View(ViewArgs),
    #[command(about = "Change a view: its size, whether it is active, or take back the tab it shows", after_help = VIEW_SET_HELP)]
    ViewSet(ViewSetArgs),
    #[command(
        about = "Type keys, text, pastes and mouse events into a pane, for programs that draw its terminal",
        after_help = INPUT_HELP
    )]
    Input(terminal::InputArgs),
    #[command(about = "Scroll a pane's view through its history", after_help = SCROLL_HELP)]
    Scroll(terminal::ScrollArgs),
    #[command(about = "Select text in a pane, print or copy the selection, or clear it", after_help = SELECTION_HELP)]
    Selection(terminal::SelectionArgs),
    #[command(about = "Lay a running `cornercase surface` out at another size", after_help = RESIZE_HELP)]
    Resize(terminal::ResizeArgs),
    #[command(about = "Move a divider of a tab's split", after_help = RATIO_HELP)]
    Ratio(terminal::RatioArgs),
}

impl Control {
    pub(crate) fn view(&self) -> Option<u64> {
        match self {
            Self::Status(a) => a.view.view,
            Self::Open(a) => a.create.view.view,
            Self::NewWorkspace(a) => a.create.view.view,
            Self::NewTab(a) => a.create.view.view,
            Self::Split(a) => a.create.view.view,
            Self::Start(a) => a.create.view.view,
            Self::Send(a) => a.target.view.view,
            Self::Keys(a) => a.target.view.view,
            Self::Mark(a) => a.target.view.view,
            Self::Read(a) => a.view.view,
            Self::Wait(a) => a.view.view,
            Self::Focus(a) => a.view.view,
            Self::Rename(a) => a.view.view,
            Self::Surface(a) => a.view,
            Self::ViewSet(a) => Some(a.view),
            _ => None,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct ViewArg {
    #[arg(long, value_name = "ID", help = VIEW_ARG)]
    pub(crate) view: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct ViewArgs {
    #[arg(
        long,
        value_name = "N",
        requires = "rows",
        help = "Columns of the view's surface (1 to 1000); without a size it only follows the session"
    )]
    pub(crate) cols: Option<u16>,
    #[arg(long, value_name = "N", requires = "cols", help = "Rows of the view's surface (1 to 500)")]
    pub(crate) rows: Option<u16>,
    #[arg(long, help = "Start inactive: it sizes nothing and streams no terminals until `view-set --active`")]
    pub(crate) inactive: bool,
    #[arg(long, value_name = "ID", conflicts_with_all = ["workspace", "project"], help = "Start on this tab [default: what the window shows]")]
    pub(crate) tab: Option<u64>,
    #[arg(long, value_name = "ID", conflicts_with = "project", help = "Start on this workspace's active tab")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "ID", help = "Start on this project's active workspace")]
    pub(crate) project: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct ViewSetArgs {
    #[arg(long, value_name = "ID", help = "The view, as the first line of `cornercase view` prints it")]
    pub(crate) view: u64,
    #[arg(long, value_name = "N", requires = "rows", help = "New columns (1 to 1000)")]
    pub(crate) cols: Option<u16>,
    #[arg(long, value_name = "N", requires = "cols", help = "New rows (1 to 500)")]
    pub(crate) rows: Option<u16>,
    #[arg(long, conflicts_with = "inactive", help = "Make it active again: it sizes its tab and streams terminals")]
    pub(crate) active: bool,
    #[arg(long, help = "Make it inactive: it sizes nothing and streams no terminals, and the tab goes back")]
    pub(crate) inactive: bool,
    #[arg(long, help = "Size the tab it shows from now on, even if the window or another view did last")]
    pub(crate) claim: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct Print {
    #[arg(long, help = "Print JSON instead of text")]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
    #[arg(long, help = "One line per pane, its columns separated by tabs, for scripts")]
    pub(crate) panes: bool,
    #[arg(
        long,
        conflicts_with_all = ["panes", "view"],
        help = "Keep running and print the whole state as one JSON line each time it changes (`cornercase view` does it for a view)"
    )]
    pub(crate) follow: bool,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct Create {
    #[arg(long, help = "Also show it in the window, or in the view with --view, which otherwise stay as they are")]
    pub(crate) focus: bool,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct Target {
    #[arg(long, value_name = "ID", help = HERE_PANE)]
    pub(crate) pane: Option<u64>,
    #[arg(long, value_name = "ID", conflicts_with = "pane", help = "A tab: its agent's pane, else its active one")]
    pub(crate) tab: Option<u64>,
    #[command(flatten)]
    pub(crate) view: ViewArg,
}

#[derive(Debug, Args)]
pub(crate) struct OpenArgs {
    #[arg(value_name = "PATH", help = "The folder")]
    pub(crate) path: PathBuf,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct NewWorkspaceArgs {
    #[arg(value_name = "NAME", help = "Its name, or its branch with --worktree")]
    pub(crate) name: String,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "Check the branch out in a git worktree of its own, at a repository's root only")]
    pub(crate) worktree: bool,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct NewTabArgs {
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with = "workspace", help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, help = "The tab's name [default: the name of the program it runs]")]
    pub(crate) name: Option<String>,
    #[command(flatten)]
    pub(crate) create: Create,
    #[arg(last = true, value_name = "COMMAND", help = "What to type into its shell")]
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct SplitArgs {
    #[arg(long, value_name = "ID", help = HERE_PANE)]
    pub(crate) pane: Option<u64>,
    #[arg(long, help = "Split it down instead of right")]
    pub(crate) down: bool,
    #[command(flatten)]
    pub(crate) create: Create,
    #[arg(last = true, value_name = "COMMAND", help = "What to type into the new shell")]
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct StartArgs {
    #[arg(
        value_name = "AGENT",
        help = "The agent, such as claude or codex [default: the one chosen in settings → agents, else the one \
                running here]"
    )]
    pub(crate) agent: Option<String>,
    #[arg(long, value_name = "MODEL", help = "The model, such as opus or gpt-5.5 [default: the agent's own]")]
    pub(crate) model: Option<String>,
    #[arg(
        long,
        value_name = "NAME",
        help = "One of the agent's modes, as settings → agents lists them, such as plan [default: the one chosen there]"
    )]
    pub(crate) mode: Option<String>,
    #[arg(
        long,
        value_name = "LEVEL",
        help = "The agent's effort, such as high: Claude Code, Codex and Grok Build take one [default: the agent's own]"
    )]
    pub(crate) effort: Option<String>,
    #[arg(
        long,
        value_name = "BRANCH",
        conflicts_with = "workspace",
        help = "Start it in the worktree of this branch, made when missing"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, help = "The tab's name [default: the agent's]")]
    pub(crate) name: Option<String>,
    #[arg(long, value_name = "TEXT", conflicts_with = "prompt_file", help = "What to ask it")]
    pub(crate) prompt: Option<String>,
    #[arg(long, value_name = "FILE", help = "Read the prompt from a file, or from standard input with -")]
    pub(crate) prompt_file: Option<PathBuf>,
    #[arg(long, help = "Then wait until the agent stops working, and print how it ended")]
    pub(crate) wait: bool,
    #[arg(long, value_enum, requires = "wait", help = UNTIL_AFTER)]
    pub(crate) until: Option<UntilArg>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Give up after this long, with status 1")]
    pub(crate) timeout: Option<f64>,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct SendArgs {
    #[command(flatten)]
    pub(crate) target: Target,
    #[arg(long, help = "Press Enter once the screen settles")]
    pub(crate) enter: bool,
    #[arg(long, requires = "enter", help = "Then wait until the agent stops working, and print how it ended")]
    pub(crate) wait: bool,
    #[arg(long, value_enum, requires = "wait", help = UNTIL_AFTER)]
    pub(crate) until: Option<UntilArg>,
    #[arg(
        long,
        value_name = "SECONDS",
        requires = "enter",
        value_parser = seconds,
        help = "Give up waiting after this long, with status 1"
    )]
    pub(crate) timeout: Option<f64>,
    #[arg(
        long,
        help = "Send even when the agent shows a dialog, a panel or its shell mode instead of its input box, or its \
                feedback survey"
    )]
    pub(crate) force: bool,
    #[command(flatten)]
    pub(crate) print: Print,
    #[arg(
        value_name = "TEXT",
        required_unless_present = "enter",
        help = "The text, or - to read it from standard input"
    )]
    pub(crate) text: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct KeysArgs {
    #[command(flatten)]
    pub(crate) target: Target,
    #[arg(required = true, value_name = "KEY", value_parser = key, help = KEY_NAMES)]
    pub(crate) keys: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ReadArgs {
    #[arg(
        long = "pane",
        value_name = "ID",
        help = "The pane; with --last-message, repeat it to read several [default: the one this runs in, else the shown one]"
    )]
    pub(crate) panes: Vec<u64>,
    #[arg(
        long = "tab",
        value_name = "ID",
        help = "A tab: its agent's pane, else its active one; with --last-message, repeat it"
    )]
    pub(crate) tabs: Vec<u64>,
    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u64).range(1..),
        help = "The last N lines, scrollback included (a pane keeps its last 10 MB of output)"
    )]
    pub(crate) lines: Option<u64>,
    #[arg(long, conflicts_with = "lines", help = "The last message of the agent in the pane, from its own record")]
    pub(crate) last_message: bool,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum UntilArg {
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
pub(crate) struct WaitArgs {
    #[arg(long = "pane", value_name = "ID", help = WAIT_PANE)]
    pub(crate) panes: Vec<u64>,
    #[arg(long = "tab", value_name = "ID", help = WAIT_TAB)]
    pub(crate) tabs: Vec<u64>,
    #[arg(long, conflicts_with = "all", help = "Return once any of the panes meets the condition")]
    pub(crate) any: bool,
    #[arg(long, help = "Return once every one of the panes has met the condition")]
    pub(crate) all: bool,
    #[arg(long, value_enum, conflicts_with_all = ["text", "quiet"], help = "Until the pane is in this state")]
    pub(crate) until: Option<UntilArg>,
    #[arg(
        long,
        value_name = "REGEX",
        conflicts_with = "quiet",
        value_parser = pattern,
        help = "Until a line on its screen matches, one already there included"
    )]
    pub(crate) text: Option<String>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Until it writes nothing for this long")]
    pub(crate) quiet: Option<f64>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Give up after this long, with status 1")]
    pub(crate) timeout: Option<f64>,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub(crate) struct Which {
    #[arg(long, value_name = "ID", help = "A pane")]
    pub(crate) pane: Option<u64>,
    #[arg(long, value_name = "ID", help = "A tab and its panes")]
    pub(crate) tab: Option<u64>,
    #[arg(long, value_name = "ID", help = "A workspace and its tabs")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "A project and its workspaces")]
    pub(crate) project: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct CloseArgs {
    #[command(flatten)]
    pub(crate) which: Which,
    #[arg(
        long,
        conflicts_with_all = ["pane", "tab", "project"],
        help = "Also remove the workspace's git worktree from disk"
    )]
    pub(crate) remove_worktree: bool,
    #[arg(long, requires = "remove_worktree", help = "Remove it even when git would lose changes")]
    pub(crate) force: bool,
}

#[derive(Debug, Args)]
pub(crate) struct FocusArgs {
    #[command(flatten)]
    pub(crate) which: Which,
    #[command(flatten)]
    pub(crate) view: ViewArg,
}

#[derive(Debug, Args)]
#[group(multiple = false)]
pub(crate) struct Renamed {
    #[arg(long, value_name = "ID", help = "The tab [default: the one this runs in, else the shown one]")]
    pub(crate) tab: Option<u64>,
    #[arg(long, value_name = "ID", help = "Rename this workspace instead")]
    pub(crate) workspace: Option<u64>,
    #[arg(
        long,
        value_name = "BRANCH",
        value_parser = branch,
        help = "Rename the workspace on this branch instead, looked up in the project this runs in, else the shown one"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "Rename this project instead")]
    pub(crate) project: Option<u64>,
    #[arg(long, value_name = "ID", help = "Rename this group instead")]
    pub(crate) group: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct RenameArgs {
    #[command(flatten)]
    pub(crate) renamed: Renamed,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[arg(value_name = "NAME", help = "The new name; an empty one goes back to the default")]
    pub(crate) name: String,
}

#[derive(Debug, Args)]
pub(crate) struct MarkArgs {
    #[command(flatten)]
    pub(crate) target: Target,
    #[arg(long, help = "Take the mark off: the agent's work has been reviewed")]
    pub(crate) reviewed: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SurfaceArgs {
    #[arg(
        long,
        value_name = "ID",
        required_unless_present = "view",
        help = "The tab to stream; `cornercase status` lists them"
    )]
    pub(crate) tab: Option<u64>,
    #[arg(
        long,
        value_name = "N",
        required_unless_present = "view",
        help = "Columns of the surface the tab is laid out in (1 to 1000)"
    )]
    pub(crate) cols: Option<u16>,
    #[arg(
        long,
        value_name = "N",
        required_unless_present = "view",
        help = "Rows of the surface the tab is laid out in (1 to 500)"
    )]
    pub(crate) rows: Option<u16>,
    #[arg(
        long,
        value_name = "ID",
        conflicts_with_all = ["tab", "cols", "rows"],
        help = "Stream the tab this view shows, at the view's size, and follow it (see `cornercase view`)"
    )]
    pub(crate) view: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct ContractArgs {
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SearchArgs {
    #[arg(required = true, value_name = "TEXT", help = "The text to look for, ignoring case")]
    pub(crate) query: Vec<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct AwayArgs {
    #[arg(long, help = "Stop: agents no longer message you on Telegram or Discord")]
    pub(crate) off: bool,
}

#[derive(Debug, Args)]
pub(crate) struct NotifyArgs {
    #[arg(required = true, value_name = "TEXT", help = "The message")]
    pub(crate) text: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct TodoArgs {
    #[command(subcommand)]
    pub(crate) action: TodoAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TodoAction {
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
    #[command(about = "Send an item's text to an agent, like `send --enter`, and check it off", after_help = TODO_SEND_HELP)]
    Send {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
        #[arg(long, value_name = "ID", help = HERE_PANE)]
        pane: Option<u64>,
        #[arg(long, value_name = "ID", conflicts_with = "pane", help = "A tab: its agent's pane, else its active one")]
        tab: Option<u64>,
        #[arg(
            long,
            help = "Send even when the agent shows a dialog, a panel or its shell mode instead of its input box, or \
                    its feedback survey"
        )]
        force: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Remove an item")]
    Rm {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
    },
}

#[derive(Debug, Args)]
pub(crate) struct FlowArgs {
    #[command(subcommand)]
    pub(crate) action: FlowAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum FlowAction {
    #[command(about = "List the flows, built in, yours and the project's, with their agents and their problems")]
    List(FlowListArgs),
    #[command(
        about = "Start a flow in a worktree, and print its workspace's id and each role's pane id",
        after_help = FLOW_RUN_HELP
    )]
    Run(FlowRunArgs),
    #[command(
        about = "Print a flow as text to share it: a file to send to a teammate, or to commit",
        after_help = FLOW_EXPORT_HELP
    )]
    Export(FlowExportArgs),
    #[command(
        about = "Add a flow someone shared, from a file or from standard input with -",
        after_help = FLOW_IMPORT_HELP
    )]
    Import(FlowImportArgs),
}

#[derive(Debug, Args)]
pub(crate) struct FlowExportArgs {
    #[arg(value_name = "NAME", help = "The flow, as `cornercase flow list` names it")]
    pub(crate) name: String,
    #[arg(long, value_name = "ID", help = "The project whose flows to look in [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ImportTo {
    #[value(help = "Your flows, in flows/ next to config.json")]
    Yours,
    #[value(help = "The project's, in .cornercase/flows/ in its repository")]
    Project,
}

impl ImportTo {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Project => "project",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct FlowImportArgs {
    #[arg(
        value_name = "SOURCE",
        help = "A file `flow export` made, - for standard input, an https:// address, or OWNER/REPO on GitHub (usecornercase.dev/flows)"
    )]
    pub(crate) source: String,
    #[arg(long, value_enum, default_value = "yours", help = "Where the flow goes")]
    pub(crate) to: ImportTo,
    #[arg(long, value_name = "ID", help = "The project, for --to project [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[arg(long, value_name = "NAME", help = "Import it under another name")]
    pub(crate) name: Option<String>,
    #[arg(long, help = "Overwrite the flow that has its name")]
    pub(crate) replace: bool,
    #[arg(
        short = 'y',
        long,
        help = "Import without asking, though the flow comes from the web or sets variables or arguments"
    )]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowListArgs {
    #[arg(
        long,
        value_name = "ID",
        help = "The project whose flows to list too [default: the one this runs in, else the shown one]"
    )]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowRunArgs {
    #[arg(value_name = "NAME", help = "The flow, as `cornercase flow list` names it")]
    pub(crate) name: String,
    #[arg(
        long,
        value_name = "TEXT",
        conflicts_with = "prompt_file",
        required_unless_present = "prompt_file",
        help = "The task: what the team should do"
    )]
    pub(crate) prompt: Option<String>,
    #[arg(long, value_name = "FILE", help = "Read the task from a file, or from standard input with -")]
    pub(crate) prompt_file: Option<PathBuf>,
    #[arg(
        long,
        value_name = "BRANCH",
        value_parser = branch,
        help = "The worktree's branch, made when missing [default: from the task's first words, made unique]"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct EventsArgs {
    #[arg(
        long = "pane",
        value_name = "ID",
        num_args = 1..,
        help = "Only what happens to these panes, and stop once they all closed"
    )]
    pub(crate) panes: Vec<u64>,
    #[arg(long, help = "Print one JSON object per line instead of text")]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RemoteArgs {
    #[arg(value_name = "DESTINATION", help = "Where ssh connects: a host from ~/.ssh/config, or user@host")]
    pub(crate) destination: String,
    #[arg(
        long,
        value_name = "COMMAND",
        help = "How to run cornercase there, as sh reads it [default: cornercase, also looked for in \
                ~/.local/bin, ~/.cargo/bin and Homebrew's folders]"
    )]
    pub(crate) command: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct LogsArgs {
    #[arg(short = 'n', long, value_name = "N", default_value_t = 200, help = "How many of the last lines to print")]
    pub(crate) lines: usize,
    #[arg(short, long, help = "Keep printing what the server writes, until interrupted")]
    pub(crate) follow: bool,
    #[arg(long, conflicts_with_all = ["lines", "follow"], help = "Print only where the log is")]
    pub(crate) path: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginArgs {
    #[command(subcommand)]
    pub(crate) action: PluginAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PluginAction {
    #[command(about = "Add the plugin in a folder of yours, turned off; for writing one")]
    Link {
        #[arg(value_name = "FOLDER", help = "The folder holding cornercase-plugin.toml")]
        folder: PathBuf,
    },
    #[command(
        about = "Install a plugin from GitHub, a git address or a folder, turned off, after showing what it runs"
    )]
    Install {
        #[arg(
            value_name = "SOURCE",
            help = "owner/repo[/folder], an https:// git address or a git folder (with //folder for a plugin in a \
                    folder of it), then @branch, @tag or @commit to pin one"
        )]
        source: String,
        #[arg(short, long, help = "Install without asking; needed when there is no terminal to ask in")]
        yes: bool,
    },
    #[command(about = "Fetch an installed plugin again and, if it changed, show what it runs and ask before updating")]
    Update {
        #[arg(value_name = "ID", help = "The plugin to update; every installed one when left out")]
        id: Option<String>,
        #[arg(short, long, help = "Update without asking; needed when there is no terminal to ask in")]
        yes: bool,
    },
    #[command(about = "Stop a plugin and delete its installed copy; a linked plugin's folder stays")]
    Remove {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "Forget a linked plugin; its folder stays as it is")]
    Unlink {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "List the plugins, whether they are on, and what they run")]
    List(Print),
    #[command(about = "Turn a plugin on: it runs as you, with access to every terminal")]
    Enable {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "Turn a plugin off, stopping what it runs")]
    Disable {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "Print what a plugin's commands wrote, and when cornercase started and stopped them")]
    Logs {
        #[arg(value_name = "ID")]
        id: String,
        #[command(flatten)]
        logs: LogsArgs,
    },
}

#[derive(Debug, Args)]
pub(crate) struct StartServerArgs {
    #[arg(
        long,
        value_name = "N",
        help = "Columns of the terminals until a window attaches, 90 to 1000 [default: 160]"
    )]
    pub(crate) cols: Option<u16>,
    #[arg(long, value_name = "N", help = "Rows of the terminals until a window attaches, 10 to 500 [default: 48]")]
    pub(crate) rows: Option<u16>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct KillServerArgs {
    #[arg(short, long, help = "Stop the server without asking, even if this terminal is one of its own")]
    pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RestartArgs {
    #[arg(short, long, help = "Restart the server without asking")]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) when: WhenArgs,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
    #[arg(long, conflicts_with = "when_idle", help = "Only say whether a newer version is out")]
    pub(crate) check: bool,
    #[arg(short, long, help = "Restart the server without asking")]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) when: WhenArgs,
}

#[derive(Debug, Args)]
pub(crate) struct WhenArgs {
    #[arg(long, help = WHEN_IDLE)]
    pub(crate) when_idle: bool,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, requires = "when_idle", help = RESTART_TIMEOUT)]
    pub(crate) timeout: Option<f64>,
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
pub(crate) struct StatsArgs {
    #[arg(long, conflicts_with = "share", help = "Print every counter and the heatmap blocks as JSON")]
    pub(super) json: bool,
    #[arg(long, help = "Print a link to a card made locally in the browser")]
    pub(super) share: bool,
}
