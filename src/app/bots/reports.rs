use std::time::{Duration, Instant};

use super::{News, Report, Starting};
use crate::app::App;
use crate::bots::chat::Line;
use crate::bots::follow;
use crate::bots::turn::Session;
use crate::queue::State;

impl App {
    pub(in crate::app) fn drive_bot_reports(&mut self, now: Instant) {
        for at in 0..self.panels.bots.live.len() {
            self.drive_reports_of(at, now);
        }
    }

    fn held_by_its_run(&self, at: usize, report: &Report) -> bool {
        let live = &self.panels.bots.live[at];
        report.run.is_some_and(|run| live.runs.iter().any(|running| running.entry == run))
    }

    fn report_due(&self, at: usize, report: &Report, now: Instant) -> bool {
        report.at + self.panels.bots.batch <= now && !self.held_by_its_run(at, report)
    }

    fn run_session(&self, run: u64) -> Option<String> {
        match &self.panels.queued.queue().entry(run)?.state {
            State::Answered { session } => session.clone(),
            _ => None,
        }
    }

    fn drive_reports_of(&mut self, at: usize, now: Instant) {
        if self.panels.bots.live[at].turn.is_some() {
            return;
        }
        let Some(first) = self.panels.bots.live[at].reports.iter().position(|report| self.report_due(at, report, now))
        else {
            return;
        };
        let run = self.panels.bots.live[at].reports[first].run;
        let session = run.and_then(|run| self.run_session(run).map(|session| (run, session)));
        let same = |report: &Report| report.run == run;
        let reports = &mut self.panels.bots.live[at].reports;
        let texts: Vec<String> =
            reports.iter().filter(|report| same(report)).map(|report| report.text.clone()).collect();
        let count = follow::batch(&texts);
        let mut taken = Vec::new();
        let mut kept = Vec::new();
        for report in std::mem::take(reports) {
            if same(&report) && taken.len() < count {
                taken.push(report);
            } else {
                kept.push(report);
            }
        }
        *reports = kept;
        self.start_report_turn(at, taken, session);
    }

    fn start_report_turn(&mut self, at: usize, taken: Vec<Report>, session: Option<(u64, String)>) {
        let id = self.panels.bots.live[at].bot.id.clone();
        self.publish_bot(&id, News::State("thinking"));
        let thread_run = session.as_ref().map(|(run, _)| *run);
        let mut asked = Vec::new();
        for report in taken {
            let Report { agent, header, body, text, .. } = report;
            let line = self.bot_append(at, |id, now| Line::finished(id, now, &header, (agent, body), thread_run));
            asked.push((line.id, text));
        }
        let session = match session {
            Some((run, session)) => Session::Resume { run, session },
            None => Session::Chat,
        };
        self.bot_start(at, &asked, Starting { session, cornercase: true });
    }

    pub(in crate::app) fn bot_reports_wake(&self, now: Instant) -> Option<Duration> {
        let batch = self.panels.bots.batch;
        let free = (0..self.panels.bots.live.len()).filter(|at| self.panels.bots.live[*at].turn.is_none());
        free.flat_map(|at| self.panels.bots.live[at].reports.iter().map(move |report| (at, report)))
            .filter(|(at, report)| !self.held_by_its_run(*at, report))
            .map(|(_, report)| (report.at + batch).saturating_duration_since(now))
            .min()
    }
}
