use clap::Args;
use clap::builder::NonEmptyStringValueParser;

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct UsageArgs {
    #[arg(long, help = "Print the last answers with their age, without asking the agents again")]
    pub(crate) cached: bool,
    #[arg(
        long,
        value_name = "NAME",
        value_parser = NonEmptyStringValueParser::new(),
        help = "Ask a machine for its limits, or with `all` this computer and every machine"
    )]
    pub(crate) machine: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}
