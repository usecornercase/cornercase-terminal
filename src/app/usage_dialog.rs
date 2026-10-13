use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::event::AppEvent;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents;
use crate::agents::usage;
use crate::host::clipboard;
use crate::log::{Job, Level};
use crate::time;
use crate::ui;

impl App {
    pub(super) fn open_usage(&mut self) {
        self.overlay = Some(Overlay::Usage);
        if self.usage_page == usage::Page::Limits {
            self.probe_usage();
        }
    }

    fn probe_usage(&mut self) {
        let supported: Vec<agents::Known> =
            agents::Known::ALL.into_iter().filter(|agent| agent.usage().is_some()).collect();
        let installed: Vec<agents::Known> = supported
            .iter()
            .copied()
            .filter(|agent| agents::installed(&agents::command(&self.config, agent.name())))
            .collect();
        let shown = if installed.is_empty() { supported } else { installed };
        for agent in self.usage.start(shown) {
            let command = agents::command(&self.config, agent.name());
            let timeout = self.usage_timeout;
            let job = Job::new(Level::Info, "usage", "probe").with("agent", agent.name());
            self.spawn_job(
                job,
                move || usage::probe(agent, &command, timeout),
                move |result, _| Some(AppEvent::Usage(agent, result)),
            );
        }
    }

    pub(super) fn usage_view(&self) -> ui::Usage {
        ui::Usage {
            page: self.usage_page,
            stats: self.stats_view(SystemTime::now()),
            sections: sections(&self.usage, Instant::now(), time::now()),
        }
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
            self.outbox
                .host_writes
                .push(clipboard::osc52(&agents::stats::share_link(&self.agents.stats, SystemTime::now())));
            self.toast = Some(Toast::new("copied a link to your stats", ui::ToastIcon::Check));
        }
    }

    pub(crate) fn stats_view(&self, now: SystemTime) -> agents::stats::View {
        self.agents.stats.view(now)
    }
}

fn section(agent: agents::Known, probe: &usage::Probe, now: Instant, clock: i64) -> ui::UsageSection {
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
        empty: report
            .and_then(|r| r.notice.or_else(|| agent.usage().filter(|_| !r.limited).map(|spec| spec.no_limits))),
        extra: report.and_then(|r| r.extra.clone()),
    }
}

fn sections(state: &usage::State, now: Instant, clock: i64) -> Vec<ui::UsageSection> {
    state.shown().map(|(agent, probe)| section(agent, probe, now, clock)).collect()
}

#[cfg(test)]
mod tests;
