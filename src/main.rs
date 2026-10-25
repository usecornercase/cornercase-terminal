use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cornercase::cli::{self, Cli};
use cornercase::error::Error;

fn main() -> Result<ExitCode> {
    if std::env::var_os("CORNERCASE_HIGHLIGHT_WORKER").is_some() {
        std::process::exit(cornercase::highlight_worker());
    }
    match cli::run(Cli::parse()) {
        Ok(done) => Ok(if done { ExitCode::SUCCESS } else { ExitCode::FAILURE }),
        Err(Error::WrongUsage(message)) => {
            eprintln!("error: {message}");
            Ok(ExitCode::from(2))
        }
        Err(e) => Err(e.into()),
    }
}
