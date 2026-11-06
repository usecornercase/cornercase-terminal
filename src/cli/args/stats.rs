use clap::Args;
use clap::builder::NonEmptyStringValueParser;

#[derive(Debug, Args)]
pub(crate) struct StatsArgs {
    #[arg(long, conflicts_with = "share", help = "Print every counter and the heatmap blocks as JSON")]
    pub(in crate::cli) json: bool,
    #[arg(long, help = "Print a link to a card made locally in the browser")]
    pub(in crate::cli) share: bool,
    #[arg(
        long,
        value_name = "NAME",
        value_parser = NonEmptyStringValueParser::new(),
        help = "Read a machine's stats through the server, or with `all` this computer's and every machine's summed"
    )]
    pub(in crate::cli) machine: Option<String>,
    #[arg(
        long,
        help = "With --machine, print the server's last copy of the machines' stats, with its age, without asking them"
    )]
    pub(in crate::cli) cached: bool,
}
