use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct RequestsArgs {
    #[arg(
        long,
        required = true,
        help = "Keep running and run each line of standard input as soon as it arrives, until it ends (give - as the only LINES)"
    )]
    pub(crate) follow: bool,
    #[arg(required = true, value_name = "LINES", help = "- to read one command per line from standard input")]
    pub(crate) input: String,
}
