use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime};

use super::registry::Plugin;
use crate::control;
use crate::issues::Source;
use crate::log::{self, Level};
use crate::os::fs::{DirBuilderExt, OpenOptionsExt, set_file_mode, set_mode};
use crate::os::process::{Grouped, Signal};
use crate::paths;
use crate::script::Shell;

pub(crate) const BIN_ENV: &str = "CORNERCASE_BIN";
pub(crate) const ID_ENV: &str = "CORNERCASE_PLUGIN_ID";
pub(crate) const ROOT_ENV: &str = "CORNERCASE_PLUGIN_ROOT";
pub(crate) const CONFIG_ENV: &str = "CORNERCASE_PLUGIN_CONFIG";
pub(crate) const STATE_ENV: &str = "CORNERCASE_PLUGIN_STATE";
pub(crate) const EVENT_ENV: &str = "CORNERCASE_EVENT";
const LOG_FILE: &str = "plugin.log";
const MAX_LOG: u64 = 1024 * 1024;
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Places {
    pub(crate) registry: PathBuf,
    config: PathBuf,
    state: PathBuf,
    pub(crate) installs: PathBuf,
}

impl Places {
    pub(crate) fn new(config: &Path, session: &Path) -> Self {
        Self {
            registry: paths::plugins(config),
            config: config.to_path_buf(),
            state: session.to_path_buf(),
            installs: paths::plugin_installs(session),
        }
    }

    pub(crate) fn config(&self, id: &str) -> PathBuf {
        paths::plugin_config(&self.config, id)
    }

    pub(crate) fn state(&self, id: &str) -> PathBuf {
        paths::plugin_state(&self.state, id)
    }

    pub(crate) fn log(&self, id: &str) -> PathBuf {
        self.state(id).join(LOG_FILE)
    }

    pub(crate) fn note(&self, id: &str, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        let line = log::line(SystemTime::now(), Level::Info, "cornercase", message, fields);
        if let Ok(mut file) = self.open_log(id) {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn open_log(&self, id: &str) -> io::Result<File> {
        let path = self.log(id);
        private_dir(&self.state(id))?;
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG) {
            std::fs::rename(&path, log::rotated(&path))?;
        }
        let file = OpenOptions::new().create(true).append(true).mode(0o600).open(path)?;
        set_file_mode(&file, 0o600)?;
        Ok(file)
    }

    pub(crate) fn launch(
        &self,
        plugin: &Plugin,
        command: &[String],
        env: &[(String, String)],
        cwd: Option<&Path>,
    ) -> io::Result<Launch> {
        self.launched(plugin, Invocation { argv: command, env, cwd }, None)
    }

    fn launched(&self, plugin: &Plugin, invocation: Invocation<'_>, log: Option<&Path>) -> io::Result<Launch> {
        let Invocation { argv: command, env, cwd } = invocation;
        let id = &plugin.entry.id;
        let (config, state) = (self.config(id), self.state(id));
        private_dir(&config)?;
        private_dir(&state)?;
        let root = plugin.root();
        let (program, args) =
            super::manifest::program(root, command).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let mut vars: Vec<(OsString, OsString)> = vec![
            (BIN_ENV.into(), std::env::current_exe().unwrap_or_default().into()),
            (ID_ENV.into(), id.into()),
            (ROOT_ENV.into(), root.into()),
            (CONFIG_ENV.into(), config.into()),
            (STATE_ENV.into(), state.into()),
        ];
        vars.extend(env.iter().map(|(key, value)| (key.into(), value.into())));
        let cwd = cwd.unwrap_or(root).to_path_buf();
        Ok(match Shell::here().interpreted(&program, &args, log) {
            Some(line) => {
                vars.extend(line.os_env());
                Launch { program: line.program.into(), args: line.args, cwd, env: vars }
            }
            None => Launch { program, args, cwd, env: vars },
        })
    }

    pub(crate) fn command(
        &self,
        plugin: &Plugin,
        command: &[String],
        env: &[(String, String)],
        cwd: Option<&Path>,
    ) -> io::Result<Command> {
        let launch =
            self.launched(plugin, Invocation { argv: command, env, cwd }, Some(&self.log(&plugin.entry.id)))?;
        let log = self.open_log(&plugin.entry.id)?;
        let mut built = Command::new(launch.program);
        built.args(launch.args).current_dir(launch.cwd).stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
        for key in removed() {
            built.env_remove(key);
        }
        built.envs(launch.env);
        Ok(built)
    }

    pub(crate) fn spawn(
        &self,
        plugin: &Plugin,
        command: &[String],
        env: &[(String, String)],
        cwd: Option<&Path>,
    ) -> io::Result<Grouped> {
        Grouped::spawn(&mut self.command(plugin, command, env, cwd)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Launch {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) env: Vec<(OsString, OsString)>,
}

pub(crate) fn removed() -> impl Iterator<Item = &'static str> {
    let own = [control::PANE_ENV, control::SERVER_ENV, crate::protocol::RESTARTED_ENV];
    let agents = crate::agents::claude::CLAUDE_SESSION_ENV.into_iter().chain(crate::agents::copilot::SESSION_ENV);
    own.into_iter().chain(agents).chain(Source::REMOTE.into_iter().filter_map(Source::token_env))
}

pub(crate) fn private_dir(path: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    set_mode(path, 0o700)
}

pub(crate) fn finish(mut child: Grouped, within: Duration) -> io::Result<Option<ExitStatus>> {
    let until = Instant::now() + within;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= until {
            child.signal(Signal::KILL);
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(POLL);
    }
}

#[derive(Debug)]
pub(crate) enum Ended {
    Status(ExitStatus),
    Timeout(Duration),
    Failed(String),
}

#[derive(Debug)]
pub(crate) struct Ran {
    pub(crate) ended: Ended,
    pub(crate) took: Duration,
}

impl Ran {
    pub(crate) fn failure(&self) -> Option<String> {
        match &self.ended {
            Ended::Status(status) if status.success() => None,
            Ended::Status(status) => Some(status.to_string()),
            Ended::Timeout(within) => Some(format!("stopped after {} s", within.as_secs())),
            Ended::Failed(why) => Some(why.clone()),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Invocation<'a> {
    pub(crate) argv: &'a [String],
    pub(crate) env: &'a [(String, String)],
    pub(crate) cwd: Option<&'a Path>,
}

pub(crate) fn run_to_end(places: &Places, plugin: &Plugin, invocation: Invocation<'_>, within: Duration) -> Ran {
    let Invocation { argv, env, cwd } = invocation;
    let started = Instant::now();
    let child = places.spawn(plugin, argv, env, cwd);
    let ended = match child {
        Err(e) => Ended::Failed(format!("cannot start {}: {e}", argv.first().map_or("", String::as_str))),
        Ok(child) => match finish(child, within) {
            Ok(Some(status)) => Ended::Status(status),
            Ok(None) => Ended::Timeout(within),
            Err(e) => Ended::Failed(e.to_string()),
        },
    };
    Ran { ended, took: started.elapsed() }
}

#[cfg(test)]
mod tests;
