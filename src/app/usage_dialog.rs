use std::time::{Instant, SystemTime};

use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use super::event::AppEvent;
use super::overlay::Overlay;
use crate::agents;
use crate::issues::{self};
use crate::log::{Job, Level};
use crate::panics;
use crate::ui::{self};
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
        let mut view = self.usage.view(Instant::now(), issues::now());
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
