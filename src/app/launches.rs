use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::event::Start;
use crate::agents;
use crate::agents::launch::{self, Launch, Step, Trust};
use crate::error::Result;
use crate::log::{self};
use crate::project::Hierarchy;

pub(super) const LAUNCH_EVERY: Duration = Duration::from_millis(100);

impl App {
    pub(super) fn drive_launches(&mut self, now: Instant) {
        if self.agents.launches.is_empty() {
            return;
        }
        let regex = self.agents.trust_prompt.regex(&self.config.trust_prompt_pattern);
        let asks = |screen: &str| regex.is_some_and(|re| agents::asks_trust(re, screen));
        let trust = if self.config.accept_trust_prompts { Trust::Accepted } else { Trust::LeftToYou };
        let mut finished = Vec::new();
        let starting: Vec<u64> = self.agents.launches.iter().map(|launch| launch.term).collect();
        for (i, launch) in self.agents.launches.iter_mut().enumerate() {
            if launch.after.iter().any(|pane| starting.contains(pane)) {
                continue;
            }
            let Some(term) = self.projects.pane_by_mut(launch.term) else {
                finished.push((i, false));
                continue;
            };
            let shell_in_foreground = term.shell_in_foreground();
            let bracketed_paste = term.emulator.bracketed_paste();
            let application_cursor = term.emulator.application_cursor();
            let emulator = &mut term.emulator;
            let mut screen = || emulator.active_contents().unwrap_or_default();
            let mut seen = launch::Seen {
                shell_in_foreground,
                bracketed_paste,
                application_cursor,
                screen: &mut screen,
                trust_prompt: &asks,
                trust,
            };
            let before = launch.stage();
            let step = launch.step(now, &mut seen);
            let (pane, kind, stage) = (launch.term, launch.kind(), launch.stage());
            match &step {
                Step::Wait if before != stage => log::info!("launch", "waits", pane = pane, kind = kind, stage = stage),
                Step::Wait => {}
                Step::Write(bytes) | Step::Done(bytes) => {
                    let done = matches!(step, Step::Done(_));
                    let message = if done { "done" } else { "wrote" };
                    let bytes = bytes.len();
                    log::info!("launch", message, pane = pane, kind = kind, from = before, to = stage, bytes = bytes);
                }
                Step::Abandon => log::warning!("launch", "the agent did not start", pane = pane, kind = kind),
            }
            match step {
                Step::Wait => {}
                Step::Write(bytes) => {
                    if !(bytes.is_empty() || term.write(&bytes)) {
                        log::warning!("launch", "the pane is not reading", pane = pane);
                        finished.push((i, false));
                    }
                }
                Step::Done(bytes) if !(bytes.is_empty() || term.write(&bytes)) => {
                    log::warning!("launch", "the pane is not reading", pane = pane);
                    finished.push((i, false));
                }
                Step::Done(_) => {
                    if launch.submits() {
                        term.submitted = Some(now);
                    }
                    finished.push((i, true));
                }
                Step::Abandon => finished.push((i, false)),
            }
        }
        for (i, started) in finished.into_iter().rev() {
            if let Some(key) = self.agents.launches.remove(i).key {
                self.requests.launched(key, started);
            }
        }
    }

    pub(super) fn open_tab_with(&mut self, p: usize, w: Option<usize>, start: Start, area: Rect) -> Result<()> {
        let active = self.ensure_workspace(p);
        let w = w.unwrap_or(active).min(self.projects[p].workspaces.len() - 1);
        self.add_tab(p, w, area)?;
        if let Some(tab) = self.projects[p].workspaces[w].tab_mut() {
            tab.name = Some(start.name);
            if let Some(term) = tab.pane() {
                self.agents.launches.push(Launch::new(term.id, start.spec, Instant::now()));
            }
        }
        Ok(())
    }
}
