use clap::Args;
use clap::builder::NonEmptyStringValueParser;

const MACHINE: &str = "Do it on this machine, as `cornercase machines` names it: ids are then that machine's, as \
                       `status --json` lists them under machines, and so are its accounts and trackers";

#[derive(Debug, Args)]
pub(crate) struct MachineArg {
    #[arg(long, value_name = "NAME", value_parser = NonEmptyStringValueParser::new(), help = MACHINE)]
    pub(crate) machine: Option<String>,
}
