use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::process;

const ENV: [(&str, &str); 4] =
    [("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1"), ("NO_COLOR", "1"), ("GH_PAGER", "cat")];

const TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn run<T: DeserializeOwned>(gh: &Path, dir: &Path, args: &[&str]) -> Result<T> {
    serde_json::from_slice(&run_raw(gh, dir, args)?).map_err(Error::GhOutput)
}

pub(crate) fn run_raw(gh: &Path, dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    run_raw_env(gh, dir, args, &[])
}

pub(crate) fn run_raw_env(gh: &Path, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Vec<u8>> {
    let out = output(gh, dir, args, env)?;
    if !out.status.success() {
        return Err(Error::Gh(failure(&out, "gh")));
    }
    Ok(out.stdout)
}

pub(crate) fn run_output(gh: &Path, dir: &Path, args: &[&str]) -> Result<Output> {
    output(gh, dir, args, &[])
}

fn output(gh: &Path, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Output> {
    let mut command = Command::new(gh);
    command.current_dir(dir).args(args).envs(ENV);
    if !env.is_empty() {
        command.env_remove("GITHUB_TOKEN").env_remove("GITHUB_ENTERPRISE_TOKEN").envs(env.iter().copied());
    }
    process::output_within(&mut command, TIMEOUT)
        .map_err(Error::RunGh)?
        .ok_or_else(|| Error::RunGh(io::Error::new(io::ErrorKind::TimedOut, "no answer within a minute")))
}

pub(crate) fn failure(out: &Output, tool: &str) -> String {
    let text = [&out.stderr, &out.stdout].map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    text.iter()
        .flat_map(|t| t.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(|| format!("{tool} failed with {}", out.status), str::to_string)
}
