use std::process::ExitCode;

use anyhow::Result;
use cornercase::cli;
use cornercase::error::Error;

fn main() -> Result<ExitCode> {
    if std::env::var_os("CORNERCASE_HIGHLIGHT_WORKER").is_some() {
        std::process::exit(cornercase::highlight_worker());
    }
    let (parsed, json) = cli::parse();
    match cli::run(parsed) {
        Ok(done) => Ok(if done { ExitCode::SUCCESS } else { ExitCode::FAILURE }),
        Err(Error::Partial) => Ok(ExitCode::from(3)),
        Err(Error::UpdateByHand) => Ok(ExitCode::from(6)),
        Err(Error::Exited(code)) => Ok(ExitCode::from(code)),
        Err(Error::WrongUsage(message)) => {
            eprintln!("error: {message}");
            Ok(ExitCode::from(2))
        }
        Err(Error::StaleBoot(message)) => {
            eprintln!("error: {message}");
            Ok(ExitCode::from(4))
        }
        Err(error) if cli::older(&error) => {
            cli::print_older(&error, json);
            Ok(ExitCode::from(5))
        }
        Err(e) => Err(e.into()),
    }
}
