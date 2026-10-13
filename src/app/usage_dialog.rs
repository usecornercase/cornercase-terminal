use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use super::event::AppEvent;
use super::overlay::Overlay;
use crate::agents;
use crate::log::{Job, Level};
use crate::panics;
use crate::time;
use crate::ui;
use crate::usage;

impl App {
    pub(super) fn open_usage(&mut self) {
        self.overlay = Some(Overlay::Usage);
        if self.usage_page == usage::Page::Limits {
            self.probe_usage();
        }
    }

    fn probe_usage(&mut self) {
        let installed: Vec<usage::Agent> = usage::Agent::ALL
            .into_iter()
            .filter(|agent| agents::installed(&agents::command(&self.config, agent.kind())))
            .collect();
        let shown = if installed.is_empty() { usage::Agent::ALL.to_vec() } else { installed };
        for agent in self.usage.start(shown) {
            let command = agents::command(&self.config, agent.kind());
            let (timeout, tx) = (self.usage_timeout, self.tx.clone());
            let job = Job::new(Level::Info, "usage", "probe").with("agent", agent.kind()).begin();
            std::thread::spawn(move || {
                let result = panics::job(|| usage::probe(agent, &command, timeout));
                job.finish(&result);
                let _ = tx.send(AppEvent::Usage(agent, result));
            });
        }
    }

    pub(super) fn usage_view(&self) -> ui::Usage {
        let mut view = view(&self.usage, Instant::now(), time::now());
        view.page = self.usage_page;
        view.stats = self.stats_view(SystemTime::now());
        view
    }

    pub(super) fn switch_usage(&mut self, page: usage::Page) {
        if self.usage_page == page {
            return;
        }
        self.usage_page = page;
        if page == usage::Page::Limits {
            self.probe_usage();
        }
    }

    pub(super) fn usage_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                self.switch_usage(self.usage_page.switched());
            }
            KeyCode::Enter | KeyCode::Esc => self.overlay = None,
            _ => {}
        }
    }

    pub fn stats_view(&self, now: SystemTime) -> crate::stats::View {
        self.stats.view(now)
    }
}

fn section(agent: usage::Agent, probe: &usage::Probe, now: Instant, clock: i64) -> ui::UsageSection {
    let report = probe.last().map(|(r, _)| r);
    let plan = report.and_then(|r| r.plan.as_deref()).map_or_else(String::new, |p| format!(" · {p} plan"));
    let windows = report.map(|r| r.windows.as_slice()).unwrap_or_default();
    let status = if probe.loading() {
        "loading…".into()
    } else {
        probe.last().map(|(_, at)| usage::updated(now.duration_since(*at).as_secs())).unwrap_or_default()
    };
    ui::UsageSection {
        title: format!("{}{plan}", agent.title()),
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
        empty: report.and_then(|r| r.notice.or_else(|| (!r.limited).then(|| agent.no_limits()))),
        extra: report.and_then(|r| r.extra.clone()),
    }
}

fn view(state: &usage::State, now: Instant, clock: i64) -> ui::Usage {
    ui::Usage {
        page: usage::Page::Limits,
        stats: crate::stats::View::default(),
        sections: state.shown().map(|(agent, probe)| section(agent, probe, now, clock)).collect(),
    }
}

#[cfg(test)]
mod tests;
