use clap::Args;

use super::Print;

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
    #[arg(long, help = "Start hidden: the tab it shows does not count as seen until `view-set --visible`")]
    pub(crate) hidden: bool,
}

#[derive(Debug, Args)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is one switch of the command")]
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
    #[arg(
        long,
        conflicts_with = "hidden",
        help = "Its window is on screen again: agents in the tab it shows count as seen"
    )]
    pub(crate) visible: bool,
    #[arg(long, help = "Its window is hidden or minimised: agents in the tab it shows notify again")]
    pub(crate) hidden: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}
