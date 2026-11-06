pub(crate) mod accounts;
pub(crate) mod agents;
pub(crate) mod arrange;
pub(crate) mod changes;
pub(crate) mod close;
pub(crate) mod config;
pub(crate) mod context;
pub(crate) mod create;
pub(crate) mod files;
pub(crate) mod flow;
pub(crate) mod held;
pub(crate) mod issue;
pub(crate) mod machine;
pub(crate) mod machines;
pub(crate) mod plugin;
pub(crate) mod ports;
pub(crate) mod processes;
pub(crate) mod pull_request;
pub(crate) mod queue;
pub(crate) mod recap;
pub(crate) mod requests;
pub(crate) mod server;
pub(crate) mod start;
pub(crate) mod stats;
pub(crate) mod terminal;
pub(crate) mod theme;
pub(crate) mod todo;
pub(crate) mod tour;
pub(crate) mod usage;
pub(crate) mod view;
pub(crate) mod worktrees;

use clap::builder::NonEmptyStringValueParser;
use clap::{Args, Parser, Subcommand, ValueEnum};

use super::help::{
    ABOUT, ACCOUNT_HELP, AFTER_HELP, AWAY_HELP, CLOSE_HELP, COMMAND_HELP, COMMAND_THERE, CONFIG_HELP, CONTRACT_HELP,
    EVENTS_HELP, FLOW_HELP, FOCUS_HELP, FOLD_HELP, GO_HELP, GROUP_HELP, HERE_PANE, INPUT_HELP, ISSUE_HELP, KEY_NAMES,
    KEYS_HELP, KILL_SERVER_HELP, LINK_HELP, LOGS_HELP, LONG_ABOUT, MARK_HELP, MOVE_HELP, MOVE_PANE_HELP, NEW_TAB_HELP,
    NEW_WORKSPACE_HELP, NOTIFY_HELP, OPEN_HELP, PLUGIN_HELP, PROFILE_HELP, RATIO_HELP, READ_HELP, REDRAW_HELP,
    REMOTE_ARG, REMOTE_HELP, RENAME_HELP, RESIZE_HELP, RESTART_HELP, SCROLL_HELP, SEARCH_HELP, SELECTION_HELP,
    SEND_HELP, SKILL_HELP, SPLIT_HELP, START_HELP, START_SERVER_HELP, STATUS_HELP, SURFACE_HELP, THEME_HELP, TODO_HELP,
    TOUR_HELP, UNTIL_AFTER, UPDATE_HELP, USAGE_HELP, VIEW_ARG, VIEW_HELP, VIEW_SET_HELP, WAIT_HELP, WAIT_PANE,
    WAIT_TAB,
};
use super::help::{
    AGENTS_HELP, CHANGES_HELP, CONTEXT_HELP, FILES_HELP, PASTE_FILE_HELP, PORTS_HELP, PROCESSES_HELP,
    PULL_REQUEST_HELP, RECAP_HELP, REQUESTS_HELP,
};
use super::help::{CLONE_HELP, HELD_HELP, MACHINES_HELP, WORKTREES_HELP};
use crate::control::Until;
use crate::term::keys;

pub(crate) use close::{CloseArgs, RenameArgs, Renamed, Which};
pub(crate) use create::{CloneArgs, Create, NewTabArgs, NewWorkspaceArgs, OpenArgs, SplitArgs};
pub(crate) use flow::*;
pub(crate) use plugin::{PluginAction, PluginArgs};
pub(crate) use server::{KillServerArgs, LogsArgs, RemoteArgs, RestartArgs, StartServerArgs, UpdateArgs, WhenArgs};
pub(crate) use start::StartArgs;
pub(crate) use stats::StatsArgs;

#[derive(Debug, Parser)]
#[command(name = "cornercase", version, about = ABOUT, long_about = LONG_ABOUT, after_help = AFTER_HELP)]
pub struct Cli {
    #[arg(long, global = true, value_name = "DESTINATION", value_parser = NonEmptyStringValueParser::new(), help = REMOTE_ARG)]
    pub(crate) remote: Option<String>,
    #[arg(long, global = true, value_name = "COMMAND", value_parser = NonEmptyStringValueParser::new(), help = COMMAND_THERE)]
    pub(crate) remote_command: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
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
        after_help = "Counts only agents in cornercase, from when counting started. With no server, reads stats.json (here; with --remote the server there must run). --machine needs the server, which asks the machine over ssh; with `all` it sums the machines it could read and lists each. Merged pull requests need a running server and gh or glab.

Examples:
  cornercase stats
  cornercase stats --json
  cornercase stats --share
  cornercase stats --machine vps
  cornercase stats --machine all --json"
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
        about = "Clone a git repository into a folder, open it as a project, and print its path",
        after_help = CLONE_HELP
    )]
    Clone(CloneArgs),
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
    Start(Box<StartArgs>),
    #[command(about = "Paste text into a pane, and press Enter with --enter", after_help = SEND_HELP)]
    Send(SendArgs),
    #[command(about = "Press keys in a pane", after_help = KEYS_HELP)]
    Keys(KeysArgs),
    #[command(
        about = "Send now a message that `send` holds for a person typing in a pane, or say it can wait its turn",
        after_help = HELD_HELP
    )]
    Held(held::HeldArgs),
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
    #[command(
        about = "Go to the agent a notice is about, as a click on its desktop notification does",
        after_help = GO_HELP
    )]
    Go(GoArgs),
    #[command(about = "Show a message in the window, with a desktop notification", after_help = NOTIFY_HELP)]
    Notify(NotifyArgs),
    #[command(about = "Have agents message you on Telegram or Discord until you turn it off", after_help = AWAY_HELP)]
    Away(AwayArgs),
    #[command(about = "Mark an agent's pane as pending review, or as reviewed", after_help = MARK_HELP)]
    Mark(MarkArgs),
    #[command(about = "Find the panes where some text was said: agent transcripts and every pane's history", after_help = SEARCH_HELP)]
    Search(SearchArgs),
    #[command(about = "Add, list, check off and remove items of the TODO list", after_help = TODO_HELP)]
    Todo(todo::TodoArgs),
    #[command(about = "Schedule issues or prompts for later; queued agents accept folder trust prompts")]
    Queue(queue::QueueArgs),
    #[command(about = "Follow agents, programs, panes and tabs as they change, one line per event", after_help = EVENTS_HELP)]
    Events(EventsArgs),
    #[command(about = "List, start, follow, share and manage flows: teams of agents in a worktree, run by an orchestrator agent", after_help = FLOW_HELP)]
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
    View(view::ViewArgs),
    #[command(about = "Change a view: its size, whether it is active, or take back the tab it shows", after_help = VIEW_SET_HELP)]
    ViewSet(view::ViewSetArgs),
    #[command(
        about = "Type keys, text, pastes and mouse events into a pane, for programs that draw its terminal",
        after_help = INPUT_HELP
    )]
    Input(terminal::InputArgs),
    #[command(
        about = "Keep one connection and run each command of standard input as it arrives, for programs that send many",
        after_help = REQUESTS_HELP
    )]
    Requests(requests::RequestsArgs),
    #[command(about = "Scroll a pane's view through its history", after_help = SCROLL_HELP)]
    Scroll(terminal::ScrollArgs),
    #[command(about = "Select text in a pane, print or copy the selection, or clear it", after_help = SELECTION_HELP)]
    Selection(terminal::SelectionArgs),
    #[command(about = "Lay a running `cornercase surface` out at another size", after_help = RESIZE_HELP)]
    Resize(terminal::ResizeArgs),
    #[command(about = "Send a running `cornercase surface` a complete line now", after_help = REDRAW_HELP)]
    Redraw(terminal::RedrawArgs),
    #[command(about = "Move a divider of a tab's split", after_help = RATIO_HELP)]
    Ratio(terminal::RatioArgs),
    #[command(
        about = "Say which file a click on a cell of a pane opens, for programs that draw its terminal",
        after_help = LINK_HELP
    )]
    Link(terminal::LinkArgs),
    #[command(
        about = "Paste a file of this machine into a pane: kept on the server's machine, its path pasted, so an agent there can open it",
        after_help = PASTE_FILE_HELP
    )]
    PasteFile(terminal::PasteFileArgs),
    #[command(
        about = "Give programs in the terminals the colours you draw them with, or say where theirs come from",
        after_help = THEME_HELP
    )]
    Theme(theme::ThemeArgs),
    #[command(about = "Read and change settings, tokens and project commands", after_help = CONFIG_HELP)]
    Config(config::ConfigArgs),
    #[command(
        about = "Move a group, project, workspace or tab before another of its kind, or a project into or out of a group",
        after_help = MOVE_HELP
    )]
    Move(arrange::MoveArgs),
    #[command(
        about = "Swap two panes of a tab, or move one to an edge of another, as the right-button drag does",
        after_help = MOVE_PANE_HELP
    )]
    MovePane(arrange::MovePaneArgs),
    #[command(about = "Collapse a group, project, workspace or agent in the sidebar", after_help = FOLD_HELP)]
    Collapse(arrange::FoldArgs),
    #[command(about = "Expand a group, project, workspace or agent in the sidebar", after_help = FOLD_HELP)]
    Expand(arrange::FoldArgs),
    #[command(about = "Create a group, change its icon and colour, or delete it", after_help = GROUP_HELP)]
    Group(arrange::GroupArgs),
    #[command(
        about = "Read issues from the trackers: which are connected, their lists, an issue and the people to filter by",
        after_help = ISSUE_HELP
    )]
    Issue(issue::IssueArgs),
    #[command(about = "List, read, mark, stage and discard the changes of a workspace, as the changes panel shows them", after_help = CHANGES_HELP)]
    Changes(changes::ChangesArgs),
    #[command(
        about = "List folders, read files and search the names or text of a workspace, as the files panel does",
        after_help = FILES_HELP
    )]
    Files(files::FilesArgs),
    #[command(
        about = "Print your Claude Code and Codex plan limits, as the Usage dialog's Limits tab shows them",
        after_help = USAGE_HELP
    )]
    Usage(usage::UsageArgs),
    #[command(
        about = "List the TCP ports a workspace's programs listen on, with the URL to open each",
        after_help = PORTS_HELP
    )]
    Ports(ports::PortsArgs),
    #[command(
        about = "Print a workspace's pull request and every check with its link, as its dialog shows it, asking gh at once",
        after_help = PULL_REQUEST_HELP
    )]
    PullRequest(pull_request::PullRequestArgs),
    #[command(
        about = "Print what fills an agent's context, as its Context dialog shows it",
        after_help = CONTEXT_HELP
    )]
    Context(context::ContextArgs),
    #[command(
        about = "Print the Recap panel's summary of what your agents did: now, yesterday or the last 7 days",
        after_help = RECAP_HELP
    )]
    Recap(recap::RecapArgs),
    #[command(
        about = "List what every pane runs with its CPU and memory, as the Processes dialog does, or stop a program",
        after_help = PROCESSES_HELP
    )]
    Processes(processes::ProcessesArgs),
    #[command(
        about = "List the worktrees on disk and what makes each safe to remove, measure one, and free its build folders",
        after_help = WORKTREES_HELP
    )]
    Worktrees(worktrees::WorktreesArgs),
    #[command(
        about = "List the machines work can start on with the state of each link, and add, change or remove them",
        after_help = MACHINES_HELP
    )]
    Machines(machines::MachinesArgs),
    #[command(about = "Show the guided tour in the window, or move, end or reset it", after_help = TOUR_HELP)]
    Tour(tour::TourArgs),
    #[command(
        about = "Close a workspace's agents and reopen them later, each in its conversation",
        after_help = AGENTS_HELP
    )]
    Agents(agents::AgentsArgs),
    #[command(about = "Add, list and remove the accounts agents log in with", after_help = ACCOUNT_HELP)]
    Account(accounts::AccountArgs),
    #[command(about = "Make, change and switch profiles: which account each agent uses", after_help = PROFILE_HELP)]
    Profile(accounts::ProfileArgs),
}

impl Control {
    pub(crate) fn view(&self) -> Option<u64> {
        match self {
            Self::Status(a) => a.view.view,
            Self::Open(a) => a.create.view.view,
            Self::Clone(a) => a.create.view.view,
            Self::NewWorkspace(a) => a.create.view.view,
            Self::NewTab(a) => a.create.view.view,
            Self::Split(a) => a.create.view.view,
            Self::Start(a) => a.create.view.view,
            Self::Send(a) => a.target.view.view,
            Self::Keys(a) => a.target.view.view,
            Self::Held(a) => a.target.view.view,
            Self::Mark(a) => a.target.view.view,
            Self::Context(a) => a.target.view.view,
            Self::Read(a) => a.view.view,
            Self::Wait(a) => a.view.view,
            Self::Focus(a) => a.view.view,
            Self::Go(a) => a.view.view,
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
    #[arg(
        long,
        conflicts_with_all = ["panes", "view"],
        help = "Print the TODO list, the queue, away, the update found and keep awake as JSON; with --follow, again \
                each time they change"
    )]
    pub(crate) app: bool,
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
pub(crate) struct GoArgs {
    #[arg(value_name = "TOKEN", help = "The notice's token, from `cornercase events --notices`")]
    pub(crate) token: String,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FocusArgs {
    #[command(flatten)]
    pub(crate) which: Which,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[arg(
        long,
        value_name = "NAME",
        conflicts_with_all = ["pane", "worktree", "project", "view", "reveal"],
        help = "Pick one of this machine's workspaces or tabs as the one shown, whose tab the window shows from there: --workspace and --tab are its ids"
    )]
    pub(crate) machine: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        conflicts_with_all = ["tab", "workspace", "worktree", "project"],
        help = "Also scroll the pane to this text and select it, as Enter on a result of the search does: the text `cornercase search` looked for"
    )]
    pub(crate) reveal: Option<String>,
    #[arg(
        long,
        value_name = "LINE",
        requires = "reveal",
        help = "The match's line as `cornercase search` printed it (screen matches); without it, the newest place the pane shows the text"
    )]
    pub(crate) line: Option<usize>,
    #[command(flatten)]
    pub(crate) print: Print,
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
        required_unless_present_any = ["view", "popup"],
        help = "The tab to stream; `cornercase status` lists them"
    )]
    pub(crate) tab: Option<u64>,
    #[arg(
        long,
        value_name = "N",
        required_unless_present_any = ["view", "popup"],
        help = "Columns of the surface the tab is laid out in (1 to 1000)"
    )]
    pub(crate) cols: Option<u16>,
    #[arg(
        long,
        value_name = "N",
        required_unless_present_any = ["view", "popup"],
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
    #[arg(
        long,
        conflicts_with_all = ["tab", "cols", "rows", "view"],
        help = "Stream the plugin popup open in the window, at its own size, until it closes"
    )]
    pub(crate) popup: bool,
    #[arg(long, help = "After the first line, send only what changed (see below)")]
    pub(crate) patches: bool,
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
    #[arg(long, help = "Also list the settings that match, as the global search does")]
    pub(crate) settings: bool,
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
pub(crate) struct EventsArgs {
    #[arg(
        long = "pane",
        value_name = "ID",
        num_args = 1..,
        help = "Only what happens to these panes, and stop once they all closed"
    )]
    pub(crate) panes: Vec<u64>,
    #[arg(
        long,
        help = "Also print what programs ask of the terminal: their clipboard writes (with the text in --json) and bells"
    )]
    pub(crate) host: bool,
    #[arg(
        long,
        help = "Also print each notice about an agent that needs you, failed or finished, with its token for `cornercase go`"
    )]
    pub(crate) notices: bool,
    #[arg(long, help = "Print one JSON object per line instead of text")]
    pub(crate) json: bool,
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
