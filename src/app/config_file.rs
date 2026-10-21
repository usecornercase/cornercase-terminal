use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::App;
use super::toast::Toast;
use crate::config::{self, Config};
use crate::log;
use crate::persist;
use crate::ui;

const CHECK_EVERY: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct ConfigFile {
    path: PathBuf,
    mark: persist::Mark,
    checked: Option<Instant>,
    problem: Option<String>,
    revision: u64,
}

impl ConfigFile {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    pub(super) fn open(path: PathBuf) -> (Config, Self) {
        let mark = persist::mark(&path);
        let (config, problem) = match config::read(&path) {
            Ok(config) => (config.unwrap_or_default(), None),
            Err(why) => {
                log::warning!("app", "config.json cannot be read, using the defaults", why = why.as_str());
                (Config::default(), Some(why))
            }
        };
        warn_unknown_profile(None, &config);
        (config, Self { path, mark, checked: None, problem, revision: 1 })
    }
}

fn warn_unknown_profile(old: Option<&Config>, new: &Config) {
    let Some(name) = new.unknown_profile() else { return };
    if old.is_some_and(|old| old.unknown_profile() == Some(name)) {
        return;
    }
    log::warning!("app", "the active profile does not exist, so agents use the system accounts", profile = name);
}

fn unreadable(why: &str) -> String {
    format!("config.json cannot be read ({why}), so cornercase leaves it as it is: fix it or remove it")
}

impl App {
    pub(super) fn watch_config(&mut self, now: Instant) {
        if self.config_file.checked.is_some_and(|at| now.saturating_duration_since(at) < CHECK_EVERY) {
            return;
        }
        self.config_file.checked = Some(now);
        if persist::mark(&self.config_file.path) != self.config_file.mark {
            let _ = self.reload_config();
        }
    }

    pub(super) fn reload_config(&mut self) -> Result<(), String> {
        let mark = persist::mark(&self.config_file.path);
        if mark != self.config_file.mark {
            self.config_file.mark = mark;
            match config::read(&self.config_file.path) {
                Ok(read) => self.config_read(read),
                Err(why) => self.config_unreadable(why),
            }
        }
        match &self.config_file.problem {
            Some(why) => Err(unreadable(why)),
            None => Ok(()),
        }
    }

    fn config_read(&mut self, read: Option<Config>) {
        self.config_file.problem = None;
        if let Some(read) = read.filter(|read| *read != self.config) {
            let changes = config::Changes::between(&self.config, &read);
            log::info!("app", "config.json changed outside", keys = changes.keys().join(","));
            warn_unknown_profile(Some(&self.config), &read);
            self.set_config(read);
        }
    }

    fn config_unreadable(&mut self, why: String) {
        if self.config_file.problem.as_deref() != Some(why.as_str()) {
            log::warning!("app", "config.json cannot be read", why = why.as_str());
            let message = format!("config.json cannot be read: {why}; the settings in use stay");
            self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
            self.config_file.problem = Some(why);
            self.settings_changed();
        }
    }

    pub(super) fn change_config(&mut self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        self.reload_config().map_err(|e| format!("failed to save the settings: {e}"))?;
        let mut next = self.config.clone();
        change(&mut next);
        if next == self.config {
            return Ok(());
        }
        self.config_file.mark =
            config::save(&self.config_file.path, &next).map_err(|e| format!("failed to save the settings: {e}"))?;
        warn_unknown_profile(Some(&self.config), &next);
        self.set_config(next);
        Ok(())
    }

    pub(super) fn write_config(&mut self, next: &Config) -> Result<(), String> {
        let changes = config::Changes::between(&self.config, next);
        self.change_config(|config| *config = changes.apply(config))
    }

    pub(super) fn settings_changed(&mut self) {
        self.config_file.revision += 1;
        self.clients.events.snapshots.mark_changed();
    }
}
