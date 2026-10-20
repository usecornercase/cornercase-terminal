use std::path::Path;
use std::time::Instant;

use crate::accounts::{self, Accounts};
use crate::agents::Known;
use crate::app::App;
use crate::git::worktree::setup;
use crate::log;
use crate::process;
use crate::project::Hierarchy;
use crate::term::Term;

impl App {
    pub(in crate::app) fn shell_blocker(&self, pane: u64, kind: Known) -> Option<String> {
        let Some(term) = self.projects.pane_by(pane) else { return Some(format!("pane {pane} is gone")) };
        if let Some((_, role)) = self.seat(pane) {
            return Some(format!("it is the {role} of a flow run"));
        }
        if term.setup.as_ref().is_some_and(setup::Pane::running) {
            return Some("wait for its setup".to_string());
        }
        if self.resuming_shell(term) {
            return Some("wait for it to resume".to_string());
        }
        if self.person_holds(pane, Instant::now()) {
            return Some("wait for what is typed in it to be sent or cleared".to_string());
        }
        if !term.shell_in_foreground() {
            let program = term.program(&self.config).unwrap_or_else(|| "its program".to_string());
            return Some(format!("stop {program} first"));
        }
        self.custom_shell(term, kind).then(|| "its account was not set by cornercase".to_string())
    }

    fn resuming_shell(&self, term: &Term) -> bool {
        term.resuming.is_some()
            || self.agents.launches.of_pane(term.id).next().is_some()
            || self.agents.restarts.holds(term.id)
    }

    fn custom_shell(&self, term: &Term, kind: Known) -> bool {
        let (Some(home), None) = (kind.account(), term.accounts().get(kind)) else { return false };
        let Some(pid) = term.shell_pid() else { return false };
        let Some((_, dir)) = process::env(pid).into_iter().find(|(name, _)| name == home.env) else { return false };
        let dir = Path::new(&dir);
        let default = self.accounts.default_home(kind).and_then(|home| home.canonicalize().ok());
        dir.is_absolute()
            && default.is_none_or(|default| dir.canonicalize().ok() != Some(default))
            && accounts::named(self.accounts.dir(), kind, dir) == accounts::CUSTOM
    }

    pub(in crate::app) fn respawn_shell(&mut self, pane: u64, kind: Known, to: &str) -> Result<(), String> {
        if let Some(why) = self.shell_blocker(pane, kind) {
            return Err(why);
        }
        let term = self.projects.pane_by(pane).ok_or_else(|| format!("pane {pane} is gone"))?;
        if term.accounts().name(kind).unwrap_or_default() == to {
            return Err(format!("pane {pane} already uses that account"));
        }
        let accounts = Accounts::of(self.accounts.dir(), &super::with_account(term.accounts().chosen(), kind, to));
        if !to.is_empty() && accounts.get(kind).is_none() {
            return Err(format!("the {} account `{to}` is gone", kind.name()));
        }
        let cwd = self.pane_folder(pane, term.cwd())?;
        let label = self.config.account_label(kind.name(), to).to_string();
        self.respawn_restarted(pane, cwd, accounts)?;
        self.fail_pane_requests(pane, &format!("pane {pane} restarted its shell on {label}, so this request ended"));
        log::info!("accounts", "shell restarted", pane = pane, provider = kind.name(), to = super::shown(to));
        Ok(())
    }
}
