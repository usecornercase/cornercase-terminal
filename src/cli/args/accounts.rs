use clap::{Args, Subcommand};

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct ProfileArg {
    #[arg(
        long,
        value_name = "NAME",
        help = "Start it with the accounts of this profile; an empty name is each agent's own login [default: the \
                default accounts]"
    )]
    pub(crate) profile: Option<String>,
    #[arg(
        long = "account",
        value_name = "AGENT=ACCOUNT",
        value_parser = choice,
        help = "Start that agent with this account, over the default ones; AGENT= is its own login. Give it once \
                per agent"
    )]
    pub(crate) accounts: Vec<(String, String)>,
}

#[derive(Debug, Args)]
pub(crate) struct AccountArgs {
    #[command(subcommand)]
    pub(crate) action: AccountAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AccountAction {
    #[command(about = "List the accounts of each agent, with their folders")]
    List(AccountListArgs),
    #[command(about = "Add an account: a folder of its own the agent keeps its login in")]
    Add(AccountAddArgs),
    #[command(about = "Remove an account and its folder; an agent that used it goes back to its own login")]
    Remove(AccountRemoveArgs),
    #[command(about = "Give an account a name the window shows; its folder keeps its name")]
    Rename(AccountRenameArgs),
    #[command(about = "Make an account the default for an agent's new workspaces; open panes keep theirs")]
    Use(AccountUseArgs),
    #[command(about = "Restart an agent's open tabs on an account, keeping their conversations")]
    Restart(AccountRestartArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AccountRenameArgs {
    #[arg(value_name = "AGENT", help = "The agent: claude, codex or gemini")]
    pub(crate) provider: String,
    #[arg(
        value_name = "ACCOUNT LABEL",
        num_args = 0..=2,
        help = "The account's folder name and the name to show; with --system only the name, with --clear only the \
                account"
    )]
    pub(crate) names: Vec<String>,
    #[arg(long, help = "Name the agent's own login, the one outside any account")]
    pub(crate) system: bool,
    #[arg(long, help = "Drop the name, so the folder's name (or System) shows again")]
    pub(crate) clear: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct AccountUseArgs {
    #[arg(value_name = "AGENT", help = "The agent: claude, codex or gemini")]
    pub(crate) provider: String,
    #[arg(value_name = "ACCOUNT", required_unless_present = "system", help = "The account's folder name")]
    pub(crate) account: Option<String>,
    #[arg(long, conflicts_with = "account", help = "Use the agent's own login, outside any account")]
    pub(crate) system: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct AccountRestartArgs {
    #[arg(value_name = "AGENT", help = "The agent: claude, codex or gemini")]
    pub(crate) provider: String,
    #[arg(value_name = "ACCOUNT", help = "The account's folder name [default: the agent's default]")]
    pub(crate) account: Option<String>,
    #[arg(long, conflicts_with = "account", help = "Restart them on the agent's own login, outside any account")]
    pub(crate) system: bool,
    #[arg(
        long = "pane",
        value_name = "ID",
        help = "Restart only this pane, also a shell at its prompt; give it once per pane [default: every pane of \
                the agent on another account]"
    )]
    pub(crate) panes: Vec<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct AccountListArgs {
    #[arg(long, help = "Also ask each agent whether its account is signed in, and as whom")]
    pub(crate) check: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct AccountAddArgs {
    #[arg(value_name = "AGENT", help = "The agent: claude, codex or gemini")]
    pub(crate) provider: String,
    #[arg(value_name = "NAME", help = "The account's name: letters, digits, `.`, `_` and `-`")]
    pub(crate) name: String,
    #[arg(long, help = "Then open a tab that runs the agent's login with this account")]
    pub(crate) login: bool,
    #[arg(long, value_name = "ID", requires = "login", help = "The workspace of the login tab [default: here]")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, requires = "login", help = "Also show the login tab in the window")]
    pub(crate) focus: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct AccountRemoveArgs {
    #[arg(value_name = "AGENT", help = "The agent: claude, codex or gemini")]
    pub(crate) provider: String,
    #[arg(value_name = "NAME", help = "The account's name")]
    pub(crate) name: String,
    #[arg(short, long, help = "Remove it without asking")]
    pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ProfileArgs {
    #[command(subcommand)]
    pub(crate) action: ProfileAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProfileAction {
    #[command(about = "List the profiles, the active one marked with *")]
    List(Print),
    #[command(about = "Make a profile: which account each agent uses")]
    New {
        #[arg(value_name = "NAME", help = "The profile's name")]
        name: String,
        #[arg(value_name = "AGENT=ACCOUNT", value_parser = pair, help = "An agent and its account, such as claude=work")]
        accounts: Vec<(String, String)>,
    },
    #[command(about = "Change which account an agent uses in a profile, or rename it")]
    Set {
        #[arg(value_name = "NAME", help = "The profile's name")]
        name: String,
        #[arg(
            long,
            value_name = "AGENT",
            required_unless_present = "rename",
            help = "The agent to change; without --account it goes back to the system's login"
        )]
        provider: Option<String>,
        #[arg(long, value_name = "ACCOUNT", requires = "provider", help = "The account it uses")]
        account: Option<String>,
        #[arg(long, value_name = "NEW", help = "A new name for the profile")]
        rename: Option<String>,
    },
    #[command(about = "Make a profile the active one: new workspaces and agents use its accounts")]
    Use {
        #[arg(value_name = "NAME", required_unless_present = "system", help = "The profile's name")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name", help = "Use the system's own logins, with no profile")]
        system: bool,
    },
    #[command(about = "Delete a profile; its accounts stay")]
    Delete {
        #[arg(value_name = "NAME", help = "The profile's name")]
        name: String,
    },
}

fn pair(text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((agent, account)) if !agent.is_empty() && !account.is_empty() => {
            Ok((agent.to_string(), account.to_string()))
        }
        _ => Err(format!("`{text}` is not AGENT=ACCOUNT, such as claude=work")),
    }
}

pub(super) fn choice(text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((agent, account)) if !agent.is_empty() => Ok((agent.to_string(), account.to_string())),
        _ => Err(format!("`{text}` is not AGENT=ACCOUNT, such as claude=work, or claude= for its system login")),
    }
}
