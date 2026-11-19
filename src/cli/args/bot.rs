use clap::{Args, Subcommand};

use super::Print;
use crate::cli::help::{
    BOT_ABOUT_HELP, BOT_AGENTS_HELP, BOT_EDIT_HELP, BOT_HISTORY_HELP, BOT_ID_HELP, BOT_NEW_HELP, BOT_SAY_HELP,
    BOT_SECRET_ASK_HELP, BOT_SECRET_DECLINE_HELP, BOT_SECRET_GET_HELP, BOT_SECRET_GIVE_HELP, BOT_SECRET_HELP,
    BOT_SECRET_LIST_HELP, BOT_SECRET_REMOVE_HELP, BOT_SECRET_SET_HELP, BOT_SETUP_HELP,
};

const ACCESS: [&str; 2] = ["cornercase", "full"];

#[derive(Debug, Args)]
pub(crate) struct BotArgs {
    #[command(subcommand)]
    pub(crate) action: BotAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BotAction {
    #[command(about = "List the agents, which can be a bot and why not", after_help = BOT_AGENTS_HELP)]
    Agents(Print),
    #[command(about = "Make a bot: a name, a folder, a memory and a chat; with --about it sets itself up at once", after_help = BOT_NEW_HELP)]
    New(NewArgs),
    #[command(about = "Run the bot's setup turn again: it reads bot.md and schedules what it asks for", after_help = BOT_SETUP_HELP)]
    Setup(BotNameArgs),
    #[command(about = "List the bots with their state and unread answers")]
    List(Print),
    #[command(about = "Change a bot's name, model, effort, account or access, from its next message", after_help = BOT_EDIT_HELP)]
    Edit(EditArgs),
    #[command(about = "Print what a bot is for: the text of its bot.md", after_help = BOT_ABOUT_HELP)]
    About(BotNameArgs),
    #[command(about = "Delete a bot, its folder and its queue entries; refused while it answers")]
    Remove(BotNameArgs),
    #[command(about = "Write to a bot; it answers in its chat", after_help = BOT_SAY_HELP)]
    Say(SayArgs),
    #[command(about = "Print a bot's chat", after_help = BOT_HISTORY_HELP)]
    History(HistoryArgs),
    #[command(
        about = "Forget the bot's conversation; the next message starts a new one that reads bot.md and memory.md"
    )]
    Reset(Target),
    #[command(about = "End the answer the bot is writing, and drop what was held for it")]
    Stop(Target),
    #[command(about = "Mark the bot's answers as read")]
    Seen(Target),
    #[command(about = "The passwords and tokens a bot keeps, encrypted, for itself", after_help = BOT_SECRET_HELP)]
    Secret(SecretArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SecretArgs {
    #[command(subcommand)]
    pub(crate) action: SecretAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SecretAction {
    #[command(about = "Save a secret for a bot, reading its value from standard input", after_help = BOT_SECRET_SET_HELP)]
    Set(SecretSetArgs),
    #[command(about = "List the names of a bot's secrets and when each was saved", after_help = BOT_SECRET_LIST_HELP)]
    List(SecretListArgs),
    #[command(about = "Remove a secret of a bot, or all of them", after_help = BOT_SECRET_REMOVE_HELP)]
    Remove(SecretRemoveArgs),
    #[command(about = "Print one of the secrets of the bot that runs this; only that bot may", after_help = BOT_SECRET_GET_HELP)]
    Get(SecretGetArgs),
    #[command(about = "Ask the user for secrets the bot that runs this lacks; only that bot may", after_help = BOT_SECRET_ASK_HELP)]
    Ask(SecretAskArgs),
    #[command(about = "Give the values a bot asked for, reading them from standard input", after_help = BOT_SECRET_GIVE_HELP)]
    Give(SecretAnswerArgs),
    #[command(about = "Turn down what a bot asked for", after_help = BOT_SECRET_DECLINE_HELP)]
    Decline(SecretAnswerArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SecretAskArgs {
    #[arg(value_name = "NAME", required = true, num_args = 1..=4, help = "1 to 4 names of a-z, 0-9, - and _, such as campus-password")]
    pub(crate) names: Vec<String>,
    #[arg(long, value_name = "TEXT", help = "Why the bot needs them, in one line of at most 200 characters")]
    pub(crate) about: String,
    #[arg(long, help = "Ask again for names already saved, to replace them")]
    pub(crate) replace: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SecretAnswerArgs {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
    pub(crate) bot: String,
    #[arg(value_name = "ASK", help = "The ask's number, from its `ask` in cornercase status --app or bot history")]
    pub(crate) ask: u64,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SecretSetArgs {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
    pub(crate) bot: String,
    #[arg(value_name = "NAME", help = "1 to 40 characters of a-z, 0-9, - and _, such as campus-password")]
    pub(crate) name: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SecretListArgs {
    #[arg(value_name = "BOT", help = "The bot's id or its name [default: the bot that runs this]")]
    pub(crate) bot: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SecretRemoveArgs {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
    pub(crate) bot: String,
    #[arg(value_name = "NAME", required_unless_present = "all", conflicts_with = "all", help = "The secret to remove")]
    pub(crate) name: Option<String>,
    #[arg(long, help = "Remove every secret, even a file that cannot be read")]
    pub(crate) all: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SecretGetArgs {
    #[arg(value_name = "NAME", help = "The secret to read")]
    pub(crate) name: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct Target {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
    pub(crate) bot: String,
}

#[derive(Debug, Args)]
pub(crate) struct BotNameArgs {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
    pub(crate) bot: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct NewArgs {
    #[arg(value_name = "NAME", help = "1 to 40 characters, not used by another bot")]
    pub(crate) name: String,
    #[arg(
        long,
        value_name = "AGENT",
        help = "An agent that can be a bot (`cornercase bot agents` lists them) [default: the agent chosen in Settings, when it can be a bot]"
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
    #[arg(
        long,
        value_name = "ON|OFF",
        value_parser = ["on", "off"],
        help = "on (the default): the bot is told when an agent it starts finishes, and answers in its chat; off: it is not"
    )]
    pub(crate) follow: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "What the bot is for; it goes into bot.md, and the bot sets itself up for it at once"
    )]
    pub(crate) about: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct EditArgs {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
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
    #[arg(
        long,
        value_name = "ON|OFF",
        value_parser = ["on", "off"],
        help = "on: the bot is told when an agent it starts finishes, and answers in its chat; off: it is not; from its next turn"
    )]
    pub(crate) follow: Option<String>,
    #[arg(long, value_name = "TEXT", help = "What the bot is for, written to bot.md; an empty one empties it")]
    pub(crate) about: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct SayArgs {
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
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
    #[arg(value_name = "BOT", help = BOT_ID_HELP)]
    pub(crate) bot: String,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(usize), help = "Only the last N lines")]
    pub(crate) last: Option<usize>,
    #[arg(long, value_name = "LINE", help = "Only the lines after this id")]
    pub(crate) after: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}
