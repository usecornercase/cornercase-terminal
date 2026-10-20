use std::collections::BTreeMap;
use std::time::{Instant, SystemTime};

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
use crate::host::clipboard;
use crate::log::{Job, Level};
use crate::time;
use crate::ui;

impl App {
    pub(super) fn open_usage(&mut self) {
        self.overlay = Some(Overlay::Usage);
        if self.usages.page == usage::Page::Limits {
            self.probe_usage();
        } else {
            self.ask_merged_stale(Instant::now());
        }
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
        let listed = accounts::list(&self.paths.accounts_dir);
        let mut probes = Vec::new();
        for agent in self.usage_agents() {
            probes.push((agent, None));
            let names = listed.iter().find(|(kind, _)| *kind == agent).map(|(_, names)| names.as_slice());
            for name in names.unwrap_or_default() {
                let home = accounts::path(&self.paths.accounts_dir, agent, name);
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
        ui::Usage {
            page: self.usages.page,
            stats: self.stats_view(SystemTime::now()),
            sections: sections(&self.usages.state, &self.config, Instant::now(), time::now()),
        }
    }

    pub(super) fn switch_usage(&mut self, page: usage::Page) {
        if self.usages.page == page {
            return;
        }
        self.usages.page = page;
        if page == usage::Page::Limits {
            self.probe_usage();
        } else {
            self.ask_merged_stale(Instant::now());
        }
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
        } else if view.page == usage::Page::Stats && !view.stats.empty && ui::usage_share(area, &view).contains(pos) {
            self.outbox.host_writes.push(clipboard::osc52(&view.stats.share_link()));
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

fn section(agent: agents::Known, title: &str, probe: &usage::Probe, now: Instant, clock: i64) -> ui::UsageSection {
    let report = probe.last().map(|(r, _)| r);
    let plan = report.and_then(|r| r.plan.as_deref()).map_or_else(String::new, |p| format!(" · {p} plan"));
    let windows = report.map(|r| r.windows.as_slice()).unwrap_or_default();
    let status = if probe.loading() {
        "loading…".into()
    } else {
        probe.last().map(|(_, at)| usage::updated(now.duration_since(*at).as_secs())).unwrap_or_default()
    };
    ui::UsageSection {
        title: format!("{title}{plan}"),
        status,
        error: probe.error().map(|e| format!("usage unavailable: {e}")),
        windows: windows
            .iter()
            .map(|w| ui::UsageWindow {
                label: w.label.clone(),
                percent: w.percent,
                severity: w.severity,
                resets: w.resets_at.map(|at| usage::until(at - clock)).unwrap_or_default(),
            })
            .collect(),
        empty: report.and_then(|r| r.notice_for(agent)),
        extra: report.and_then(|r| r.extra.clone()),
    }
}

pub(super) fn active(
    keys: &[(agents::Known, Option<&str>)],
    chosen: &BTreeMap<String, String>,
    (agent, account): (agents::Known, Option<&str>),
) -> bool {
    let names = names(keys, agent);
    let picked = chosen.get(agent.name()).map(String::as_str);
    !names.is_empty() && picked.filter(|name| names.contains(name)) == account
}

pub(super) fn labelled(
    config: &Config,
    keys: &[(agents::Known, Option<&str>)],
    (agent, account): (agents::Known, Option<&str>),
) -> Option<String> {
    label(config, agent, account, !names(keys, agent).is_empty())
}

fn names<'a>(keys: &[(agents::Known, Option<&'a str>)], agent: agents::Known) -> Vec<&'a str> {
    keys.iter().filter(|(kind, _)| *kind == agent).filter_map(|(_, name)| *name).collect()
}

fn sections(state: &usage::State, config: &Config, now: Instant, clock: i64) -> Vec<ui::UsageSection> {
    let keys: Vec<(agents::Known, Option<&str>)> = state.shown().map(|(agent, account, _)| (agent, account)).collect();
    let chosen = config.selected_accounts();
    state
        .shown()
        .map(|(agent, account, probe)| {
            let key = (agent, account);
            let title =
                usage::heading(agent.title(), labelled(config, &keys, key).as_deref(), active(&keys, chosen, key));
            section(agent, &title, probe, now, clock)
        })
        .collect()
}

#[cfg(test)]
mod tests;
