use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::event::AppEvent;
use super::overlay::Overlay;
use super::toast::Toast;
use super::view::label;
use crate::accounts::{self, Account};
use crate::agents;
use crate::agents::usage;
use crate::config::Config;
use crate::control::usage::PlanInfo;
use crate::log::{Job, Level};
use crate::time;
use crate::ui;

mod limits;
mod machines;

pub(super) struct Usages {
    pub(super) state: usage::State,
    page: usage::Page,
    machine: machines::Pick,
    timeout: Duration,
}

impl Default for Usages {
    fn default() -> Self {
        Self {
            state: usage::State::default(),
            page: usage::Page::default(),
            machine: machines::Pick::default(),
            timeout: usage::TIMEOUT,
        }
    }
}

impl Usages {
    pub(super) fn timeout(&self) -> Duration {
        self.timeout
    }

    #[cfg(test)]
    pub(super) fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

impl App {
    pub(super) fn open_usage(&mut self) {
        self.overlay = Some(Overlay::Usage);
        self.ask_usage();
    }

    fn ask_usage(&mut self) {
        if !matches!(self.usage_pick(), machines::Pick::Machine(_)) {
            if self.usages.page == usage::Page::Limits {
                self.probe_usage();
            } else {
                self.ask_merged_stale(Instant::now());
            }
        }
        self.ask_machines();
    }

    pub(super) fn usage_agents(&self) -> Vec<agents::Known> {
        let supported: Vec<agents::Known> =
            agents::Known::ALL.into_iter().filter(|agent| agent.usage().is_some()).collect();
        let installed: Vec<agents::Known> = supported
            .iter()
            .copied()
            .filter(|agent| agents::installed(&agents::command(&self.config, agent.name())))
            .collect();
        if installed.is_empty() { supported } else { installed }
    }

    pub(super) fn usage_probes(&self) -> Vec<(agents::Known, Option<Account>)> {
        let listed = accounts::list(self.accounts.dir());
        let mut probes = Vec::new();
        for agent in self.usage_agents() {
            probes.push((agent, None));
            let names = listed.iter().find(|(kind, _)| *kind == agent).map(|(_, names)| names.as_slice());
            for name in names.unwrap_or_default() {
                let home = accounts::path(self.accounts.dir(), agent, name);
                let home = home.canonicalize().unwrap_or(home);
                probes.push((agent, Some(Account { kind: agent, name: name.clone(), home })));
            }
        }
        probes
    }

    fn probe_usage(&mut self) {
        let probes = self.usage_probes();
        self.start_probes(probes);
    }

    pub(super) fn start_probes(&mut self, probes: Vec<(agents::Known, Option<Account>)>) {
        for (agent, account) in self.usages.state.start(probes) {
            let command = agents::command(&self.config, agent.name());
            let timeout = self.usages.timeout;
            let key = usage::key(agent, account.as_ref());
            let mut job = Job::new(Level::Info, "usage", "probe").with("agent", agent.name());
            if let Some(account) = &account {
                job = job.with("account", &account.name);
            }
            let home = account.map(|account| account.home);
            self.spawn_job(
                job,
                move || usage::probe(agent, &command, home.as_deref(), timeout),
                move |result, _| Some(AppEvent::Usage(key, result)),
            );
        }
    }

    pub(super) fn usage_view(&self) -> ui::Usage {
        let (stats, notes) = self.picked_stats(SystemTime::now());
        let (machines, picked) = self.usage_picks();
        let (sections, by_machine) = self.picked_limits(Instant::now(), time::now());
        ui::Usage { page: self.usages.page, stats, sections, by_machine, machines, picked, notes }
    }

    pub(super) fn switch_usage(&mut self, page: usage::Page) {
        if self.usages.page == page {
            return;
        }
        self.usages.page = page;
        self.ask_usage();
    }

    pub(super) fn usage_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                self.switch_usage(self.usages.page.switched());
            }
            KeyCode::Enter | KeyCode::Esc => self.overlay = None,
            _ => {}
        }
    }

    pub(super) fn usage_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let view = self.usage_view();
        if ui::usage_done(area, &view).contains(pos) {
            self.overlay = None;
        } else if let Some(i) = ui::usage_tabs(area, &view).iter().position(|r| r.contains(pos)) {
            self.switch_usage(if i == 0 { usage::Page::Limits } else { usage::Page::Stats });
        } else if let Some(i) = ui::usage_picks(area, &view).iter().position(|r| r.contains(pos)) {
            self.pick_usage(i);
        } else if view.page == usage::Page::Stats && !view.stats.empty && ui::usage_share(area, &view).contains(pos) {
            self.outbox.copy(&view.stats.share_link());
            self.messages.toast = Some(Toast::new("copied a link to your stats", ui::ToastIcon::Check));
        }
    }

    pub(crate) fn stats_view(&self, now: SystemTime) -> agents::stats::View {
        let mut view = self.agents.stats.view(now);
        if self.config.merged_pull_requests {
            view.merged = self.git.merged.row(self.agents.stats.since(), now);
        }
        view
    }
}

fn active(
    keys: &[(agents::Known, Option<&str>)],
    chosen: &BTreeMap<String, String>,
    (agent, account): (agents::Known, Option<&str>),
) -> bool {
    let names = names(keys, agent);
    let picked = chosen.get(agent.name()).map(String::as_str);
    !names.is_empty() && picked.filter(|name| names.contains(name)) == account
}

fn labelled(
    config: &Config,
    keys: &[(agents::Known, Option<&str>)],
    (agent, account): (agents::Known, Option<&str>),
) -> Option<String> {
    label(config, agent, account, !names(keys, agent).is_empty())
}

fn names<'a>(keys: &[(agents::Known, Option<&'a str>)], agent: agents::Known) -> Vec<&'a str> {
    keys.iter().filter(|(kind, _)| *kind == agent).filter_map(|(_, name)| *name).collect()
}

pub(super) fn infos(
    state: &usage::State,
    config: &Config,
    listed: &[(agents::Known, Option<&str>)],
    now: Instant,
    clock: SystemTime,
) -> Vec<PlanInfo> {
    let chosen = config.selected_accounts();
    listed
        .iter()
        .map(|&(agent, account)| {
            let key = (agent, account);
            let label = labelled(config, listed, key);
            PlanInfo::of(agent, state.probe_of(agent, account), now, clock).for_account(
                account,
                label,
                active(listed, chosen, key),
            )
        })
        .collect()
}

fn here(state: &usage::State, config: &Config, now: Instant, clock: i64) -> Vec<PlanInfo> {
    let listed: Vec<(agents::Known, Option<&str>)> =
        state.shown().map(|(agent, account, _)| (agent, account)).collect();
    infos(state, config, &listed, now, at(clock))
}

fn at(clock: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::try_from(clock).unwrap_or_default())
}

fn sections(state: &usage::State, config: &Config, now: Instant, clock: i64) -> Vec<ui::UsageSection> {
    here(state, config, now, clock).iter().map(|info| limits::section(info, clock)).collect()
}

#[cfg(test)]
mod tests;
