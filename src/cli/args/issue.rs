use clap::{Args, Subcommand};

use super::Print;

pub(crate) const SOURCES: [&str; 7] = ["github", "gitlab", "shortcut", "linear", "jira", "plane", "asana"];

const PROJECT: &str =
    "The project whose repository and places count [default: the one this runs in, else the shown one]";

#[derive(Debug, Args)]
pub(crate) struct IssueArgs {
    #[command(subcommand)]
    pub(crate) action: IssueAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum IssueAction {
    #[command(about = "Say which trackers are connected for a project, what is missing and where work can start")]
    Sources(SourcesArgs),
    #[command(about = "List issues with the Issues dialog's filters, one line each")]
    List(ListArgs),
    #[command(about = "Print an issue's details, its description and its comments")]
    Read(ReadArgs),
    #[command(about = "List the people a tracker's assignee and author filters can name")]
    People(PeopleArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SourcesArgs {
    #[arg(long, value_name = "ID", help = PROJECT)]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct ListArgs {
    #[arg(long, value_parser = SOURCES, help = "Only this tracker [default: every connected one]")]
    pub(crate) source: Option<String>,
    #[arg(long, value_name = "ID", help = PROJECT)]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "Closed issues too")]
    pub(crate) closed: bool,
    #[arg(
        long,
        value_name = "WHO",
        visible_alias = "owner",
        help = "me, anyone, a handle or, for Jira, Plane and Asana, an id from `issue people` (a handle needs --source)"
    )]
    pub(crate) assignee: Option<String>,
    #[arg(
        long,
        value_name = "WHO",
        visible_aliases = ["requester", "creator", "reporter"],
        help = "Who opened it: me, anyone, a handle or an id from `issue people` (a handle needs --source)"
    )]
    pub(crate) author: Option<String>,
    #[arg(long, help = "Answer from what the server already holds, without asking the trackers")]
    pub(crate) cached: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct ReadArgs {
    #[arg(value_name = "ISSUE", help = "A key such as #482, 482, sc-48 or ENG-42, or the issue's URL")]
    pub(crate) issue: String,
    #[arg(long, value_parser = SOURCES, help = "The tracker, for a key that more than one could have")]
    pub(crate) source: Option<String>,
    #[arg(long, value_name = "ID", help = PROJECT)]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct PeopleArgs {
    #[arg(value_name = "SOURCE", value_parser = SOURCES, help = "The tracker")]
    pub(crate) source: String,
    #[arg(long, value_name = "ID", help = PROJECT)]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "Answer from what the server already holds, without asking the tracker")]
    pub(crate) cached: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct IssueArg {
    #[arg(
        id = "issue",
        long = "issue",
        value_name = "KEY",
        conflicts_with_all = ["prompt", "prompt_file"],
        help = "Work on this issue instead: a key such as #482, sc-48 or ENG-42, or its URL, with the branch and prompt the Issues dialog's start uses"
    )]
    pub(crate) key: Option<String>,
    #[arg(
        long,
        requires = "issue",
        value_parser = SOURCES,
        help = "The tracker, for a key that more than one could have"
    )]
    pub(crate) source: Option<String>,
}
