use clap::{ArgGroup, Args, Subcommand, ValueEnum};

use super::{Print, branch};
use crate::cli::help::BY_BRANCH;
use crate::control::arrange::Place;

#[derive(Debug, Args)]
#[command(
    group(ArgGroup::new("row").required(true).multiple(false).args(["group", "project", "workspace", "worktree", "tab"])),
    group(ArgGroup::new("place").required(true).multiple(false).args(["before", "last", "into", "ungroup"]))
)]
pub(crate) struct MoveArgs {
    #[arg(long, value_name = "ID", help = "Move this group")]
    pub(crate) group: Option<u64>,
    #[arg(long, value_name = "ID", help = "Move this project")]
    pub(crate) project: Option<u64>,
    #[arg(long, value_name = "ID", help = "Move this workspace")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "Move this tab")]
    pub(crate) tab: Option<u64>,
    #[arg(
        long,
        value_name = "ID",
        help = "Before this one, of the same kind; a project joins the group of the project it goes before"
    )]
    pub(crate) before: Option<u64>,
    #[arg(long, help = "At the end of its list")]
    pub(crate) last: bool,
    #[arg(
        long,
        value_name = "GROUP",
        conflicts_with_all = ["group", "workspace", "worktree", "tab"],
        help = "Into this group, after its projects"
    )]
    pub(crate) into: Option<u64>,
    #[arg(
        long,
        conflicts_with_all = ["group", "workspace", "worktree", "tab"],
        help = "Out of its group, after the projects that have none"
    )]
    pub(crate) ungroup: bool,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("row").required(true).multiple(false).args(["group", "project", "workspace", "worktree"])))]
pub(crate) struct FoldArgs {
    #[arg(long, value_name = "ID", help = "A group and its projects")]
    pub(crate) group: Option<u64>,
    #[arg(long, value_name = "ID", help = "A project and its workspaces")]
    pub(crate) project: Option<u64>,
    #[arg(long, value_name = "ID", help = "A workspace and its tabs")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum PlaceArg {
    Swap,
    Left,
    Right,
    Above,
    Below,
}

impl From<PlaceArg> for Place {
    fn from(place: PlaceArg) -> Self {
        match place {
            PlaceArg::Swap => Self::Swap,
            PlaceArg::Left => Self::Left,
            PlaceArg::Right => Self::Right,
            PlaceArg::Above => Self::Above,
            PlaceArg::Below => Self::Below,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct MovePaneArgs {
    #[arg(long, value_name = "ID", required = true, help = "The pane to move")]
    pub(crate) pane: u64,
    #[arg(long, value_name = "ID", required = true, help = "The pane of the same tab to move it onto")]
    pub(crate) target: u64,
    #[arg(long, value_enum, required = true, help = "Swap the two, or take that side of the target")]
    pub(crate) place: PlaceArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct GroupArgs {
    #[command(subcommand)]
    pub(crate) action: GroupAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GroupAction {
    #[command(about = "Create a group, with the next icon and colour, and print its id")]
    New {
        #[arg(required = true, value_name = "NAME", help = "The group's name")]
        name: Vec<String>,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Change a group's icon and colour, or print them and the ones to choose from")]
    Style {
        #[arg(value_name = "ID", help = "The group")]
        group: u64,
        #[arg(long, value_name = "CHAR", help = "One of ● ◉ ◐ ◆ ■ ▲ ▼ ★ ✦ ♥ ♣ ♠")]
        icon: Option<String>,
        #[arg(long, value_name = "N", help = "A palette index: 1 9 208 214 3 11 2 10 6 14 4 12 99 5 13 205")]
        colour: Option<u8>,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Delete a group; its projects stay, outside any group, unless --close-projects")]
    Delete {
        #[arg(value_name = "ID", help = "The group")]
        group: u64,
        #[arg(long, help = "Close its projects too, then delete the group once their shells have stopped")]
        close_projects: bool,
    },
}
