use clap::{Args, Subcommand};

use super::Print;
use crate::cli::help::bot::{ABOUT_HELP, AGENTS_HELP, EDIT_HELP, HISTORY_HELP, NEW_HELP, SAY_HELP};

const ACCESS: [&str; 2] = ["cornercase", "full"];

#[derive(Debug, Args)]
pub(crate) struct BotArgs {
    #[command(subcommand)]
    pub(crate) action: BotAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BotAction {
    #[command(about = "List the agents, which can be a bot and why not", after_help = AGENTS_HELP)]
    Agents(Print),
    #[command(about = "Make a bot: a name, a folder, a memory and a chat; it starts nothing", after_help = NEW_HELP)]
    New(NewArgs),
    #[command(about = "List the bots with their state and unread answers")]
    List(Print),
    #[command(about = "Change a bot's name, model, effort, account or access, from its next message", after_help = EDIT_HELP)]
    Edit(EditArgs),
    #[command(about = "Delete a bot and its folder; refused while it answers")]
    Remove(Target),
    #[command(about = "Write to a bot; it answers in its chat", after_help = SAY_HELP)]
    Say(SayArgs),
    #[command(about = "Print a bot's chat", after_help = HISTORY_HELP)]
    History(HistoryArgs),
    #[command(
        about = "Forget the bot's conversation; the next message starts a new one that reads bot.md and memory.md"
    )]
    Reset(Target),
    #[command(about = "End the answer the bot is writing, and drop what was held for it")]
    Stop(Target),
    #[command(about = "Mark the bot's answers as read")]
    Seen(Target),
}

#[derive(Debug, Args)]
pub(crate) struct Target {
    #[arg(value_name = "BOT", help = ABOUT_HELP)]
    pub(crate) bot: String,
}

#[derive(Debug, Args)]
pub(crate) struct NewArgs {
    #[arg(value_name = "NAME", help = "1 to 40 characters, not used by another bot")]
    pub(crate) name: String,
    #[arg(
        long,
        value_name = "AGENT",
        help = "claude or codex [default: the agent chosen in Settings, when it can be a bot]"
    )]
    pub(crate) agent: Option<String>,
    #[arg(long, value_name = "MODEL", help = "The model, as `start --model` takes it")]
    pub(crate) model: Option<String>,
    #[arg(long, value_name = "LEVEL", help = "The effort, as `start --effort` takes it")]
    pub(crate) effort: Option<String>,
    #[arg(long, value_name = "NAME", help = "An account of that agent [default: the agent's own login]")]
    pub(crate) account: Option<String>,
    #[arg(long, value_name = "ACCESS", value_parser = ACCESS, help = "cornercase (the default) or full: it can change any file and run anything, unattended")]
    pub(crate) access: Option<String>,
    #[arg(long, value_name = "TEXT", help = "What the bot is for; it goes into bot.md")]
    pub(crate) about: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct EditArgs {
    #[arg(value_name = "BOT", help = ABOUT_HELP)]
    pub(crate) bot: String,
    #[arg(long, value_name = "NAME", help = "The new name; the id stays")]
    pub(crate) name: Option<String>,
    #[arg(long, value_name = "AGENT", help = "Only the bot's own agent: another one cannot continue its conversation")]
    pub(crate) agent: Option<String>,
    #[arg(long, value_name = "MODEL", help = "The model; an empty one goes back to the agent's own")]
    pub(crate) model: Option<String>,
    #[arg(long, value_name = "LEVEL", help = "The effort; an empty one goes back to the agent's own")]
    pub(crate) effort: Option<String>,
    #[arg(long, value_name = "NAME", help = "The account; an empty one goes back to the agent's own login")]
    pub(crate) account: Option<String>,
    #[arg(long, value_name = "ACCESS", value_parser = ACCESS, help = "cornercase or full")]
    pub(crate) access: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SayArgs {
    #[arg(value_name = "BOT", help = ABOUT_HELP)]
    pub(crate) bot: String,
    #[arg(value_name = "TEXT", help = "The message, at most 20,000 characters; - reads it from standard input")]
    pub(crate) text: Option<String>,
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "text",
        help = "Read the message from a file; - is standard input"
    )]
    pub(crate) file: Option<String>,
    #[arg(long, help = "Wait for the answer and print it; exit 3 when the turn fails or is stopped")]
    pub(crate) wait: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct HistoryArgs {
    #[arg(value_name = "BOT", help = ABOUT_HELP)]
    pub(crate) bot: String,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(usize), help = "Only the last N lines")]
    pub(crate) last: Option<usize>,
    #[arg(long, value_name = "LINE", help = "Only the lines after this id")]
    pub(crate) after: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}
