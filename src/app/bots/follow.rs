use std::time::Instant;

use super::{BotEvent, Followed, Report};
use crate::agents::Known;
use crate::agents::activity::Status;
use crate::agents::context::Said as Last;
use crate::app::App;
use crate::app::control::Starter;
use crate::app::event::AppEvent;
use crate::app::settle::{EXCERPT, Notice, Watch, excerpt, settle};
use crate::bots::chat::{AgentRef, Line};
use crate::bots::follow::{self, MAX_FOLLOWED, Said};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::term::Term;

#[derive(Debug, Clone)]
pub(in crate::app) struct Started {
    pub(in crate::app) agent: Option<String>,
    pub(in crate::app) arm: bool,
}

#[derive(Debug)]
pub(crate) struct Read {
    pane: u64,
    run: Option<u64>,
    failed: bool,
    agent: AgentRef,
    place: String,
    screen: String,
    said: Result<Option<Last>>,
}

enum Seen {
    Waiting { at: usize, pane: u64 },
    Over { at: usize, pane: u64 },
}

impl App {
    pub(in crate::app) fn starter(&self, client: u64, bot: Option<&str>, run: Option<u64>) -> Option<Starter> {
        let id = self.clients.bots.bot_of(client).or(bot)?;
        let at = self.panels.bots.by_id(id)?;
        let own = |entry: &crate::queue::Entry| entry.bot() == Some(id);
        let run = run.filter(|run| self.panels.queued.queue().entry(*run).is_some_and(own));
        Some(Starter { bot: self.panels.bots.live[at].key, run })
    }

    pub(in crate::app) fn tag_started(&mut self, pane: u64, starter: Option<Starter>, what: Started) {
        let Some(starter) = starter else { return };
        let Some(at) = self.panels.bots.by_key(starter.bot) else { return };
        let id = self.panels.bots.live[at].bot.id.clone();
        if let Some(term) = self.projects.pane_by_mut(pane) {
            term.bot = Some(id);
        }
        let Some(kind) = what.agent.filter(|_| self.panels.bots.live[at].bot.follow) else { return };
        self.follow_agent(at, pane, (&kind, starter.run), what.arm);
        let (Some(agent), Some(place)) = (self.agent_ref(pane, &kind), self.pane_place(pane)) else { return };
        let text = follow::started(&kind, &place);
        self.bot_append(at, |id, now| Line::started(id, now, &text, agent, starter.run));
    }

    fn agent_ref(&self, pane: u64, kind: &str) -> Option<AgentRef> {
        let at = self.projects.tab_of_pane(pane)?;
        Some(AgentRef {
            kind: kind.to_string(),
            pane,
            workspace: self.projects.workspace(at.workspace()).id,
            project: self.projects[at.project()].id,
            boot: crate::control::server_token().to_string(),
        })
    }

    fn follow_agent(&mut self, at: usize, pane: u64, (kind, run): (&str, Option<u64>), armed: bool) {
        let live = &mut self.panels.bots.live[at];
        live.following.retain(|followed| followed.pane != pane);
        live.following.push(Followed { pane, kind: kind.to_string(), run, armed, watch: None });
        if live.following.len() <= MAX_FOLLOWED {
            return;
        }
        let dropped = live.following.remove(0);
        let place = self.pane_place(dropped.pane).unwrap_or_else(|| format!("pane {}", dropped.pane));
        let text = follow::dropped(&dropped.kind, &place);
        self.bot_append(at, |id, now| Line::notice(id, now, &text));
    }

    pub(in crate::app) fn bot_prompted(&mut self, starter: Option<Starter>, pane: u64) {
        let Some(starter) = starter else { return };
        let Some(at) = self.panels.bots.by_key(starter.bot) else { return };
        let live = &mut self.panels.bots.live[at];
        if let Some(followed) = live.following.iter_mut().filter(|_| live.bot.follow).find(|f| f.pane == pane) {
            followed.armed = true;
            followed.run = starter.run;
        }
    }

    pub(in crate::app) fn follow_restored(&mut self) {
        for at in 0..self.panels.bots.live.len() {
            self.rebuild_following(at);
        }
    }

    pub(in crate::app) fn rebuild_following(&mut self, at: usize) {
        let live = &self.panels.bots.live[at];
        if !live.bot.follow {
            return;
        }
        let tagged = |term: &&Term| term.bot.as_deref() == Some(live.bot.id.as_str());
        let panes: Vec<(u64, String)> =
            self.projects.terms().filter(tagged).map(|term| (term.id, saved_kind(term))).collect();
        let skip = panes.len().saturating_sub(MAX_FOLLOWED);
        let live = &mut self.panels.bots.live[at];
        live.following.clear();
        for (pane, kind) in panes.into_iter().skip(skip) {
            live.following.push(Followed { pane, kind, run: None, armed: false, watch: None });
        }
    }

    pub(in crate::app) fn follow_changed(&mut self, at: usize, before: bool) {
        match (before, self.panels.bots.live[at].bot.follow) {
            (true, false) => self.stop_following(at),
            (false, true) => self.rebuild_following(at),
            _ => {}
        }
    }

    fn stop_following(&mut self, at: usize) {
        let live = &mut self.panels.bots.live[at];
        live.following.clear();
        live.reports.clear();
    }

    pub(in crate::app) fn forget_tags(&mut self, id: &str) {
        for term in self.projects.terms_mut() {
            if term.bot.as_deref() == Some(id) {
                term.bot = None;
            }
        }
    }

    pub(in crate::app) fn started_by(&self, term: &Term) -> Option<String> {
        self.bot_name_of(term.bot.as_deref()?)
    }

    pub(in crate::app) fn bot_name_of(&self, id: &str) -> Option<String> {
        let at = self.panels.bots.by_id(id)?;
        Some(self.panels.bots.live[at].bot.name.clone())
    }

    pub(in crate::app) fn following_panes(&self, at: usize) -> Vec<u64> {
        self.panels.bots.live[at].following.iter().map(|followed| followed.pane).collect()
    }

    pub(in crate::app) fn follow_bot_agents(&mut self, now: Instant) {
        let mut seen = Vec::new();
        for at in 0..self.panels.bots.live.len() {
            self.watch_followed(at, now, &mut seen);
        }
        for news in seen {
            match news {
                Seen::Waiting { at, pane } => self.write_waiting(at, pane),
                Seen::Over { at, pane } => self.start_read(at, pane),
            }
        }
    }

    fn watch_followed(&mut self, at: usize, now: Instant, seen: &mut Vec<Seen>) {
        let Self { panels, projects, runs, .. } = self;
        let live = &mut panels.bots.live[at];
        if !live.bot.follow {
            return;
        }
        live.following.retain(|followed| projects.pane_by(followed.pane).is_some());
        for followed in &mut live.following {
            let Some(term) = projects.pane_by(followed.pane) else { continue };
            let Some(state) = term.agent.state() else { continue };
            let state = if term.agent.reported_done() { "done" } else { state };
            let watch = followed.watch.get_or_insert_with(|| Watch::started(state, now));
            watch.see(state, now);
            let notice = settle(watch, term, now, false);
            if !followed.armed {
                continue;
            }
            let finished = matches!(notice, Some(Notice::Finished(..)));
            let over = match runs.standing(followed.pane) {
                Some(standing) => follow::orchestrator_over(finished, standing),
                None => finished,
            };
            if matches!(notice, Some(Notice::Waiting(_))) {
                seen.push(Seen::Waiting { at, pane: followed.pane });
            } else if over {
                followed.armed = false;
                seen.push(Seen::Over { at, pane: followed.pane });
            }
        }
    }

    fn write_waiting(&mut self, at: usize, pane: u64) {
        let Some(term) = self.projects.pane_by(pane) else { return };
        let screen = excerpt(&term.emulator.screen_text().unwrap_or_default(), EXCERPT);
        let kind = self.followed_kind(at, term);
        let run = self.panels.bots.live[at].following.iter().find(|f| f.pane == pane).and_then(|f| f.run);
        let (Some(agent), Some(place)) = (self.agent_ref(pane, &kind), self.pane_place(pane)) else { return };
        let text = follow::waiting(&kind, &place);
        self.bot_append(at, |id, now| Line::waiting(id, now, &text, (agent, screen), run));
        if !self.bot_on_screen(at) {
            self.panels.bots.live[at].unread += 1;
        }
    }

    fn followed_kind(&self, at: usize, term: &Term) -> String {
        let stored = self.panels.bots.live[at].following.iter().find(|followed| followed.pane == term.id);
        term.agent
            .agent()
            .map(str::to_string)
            .or_else(|| stored.map(|followed| followed.kind.clone()))
            .unwrap_or_default()
    }

    fn start_read(&mut self, at: usize, pane: u64) {
        let Some(term) = self.projects.pane_by(pane) else { return };
        let kind = self.followed_kind(at, term);
        let record = Known::of(&kind).and_then(|known| term.context.record_for(known));
        let (failed, screen) = (
            term.agent.status() == Some(Status::Failed),
            excerpt(&term.emulator.screen_text().unwrap_or_default(), EXCERPT),
        );
        let run = self.panels.bots.live[at].following.iter().find(|f| f.pane == pane).and_then(|f| f.run);
        let (Some(agent), Some(place)) = (self.agent_ref(pane, &kind), self.pane_place(pane)) else { return };
        let read = Read { pane, run, failed, agent, place, screen, said: Ok(None) };
        let bot = self.panels.bots.live[at].bot.id.clone();
        let Some(record) = record else {
            self.bot_report_read(&bot, read);
            return;
        };
        let job = Job::new(Level::Debug, "bots", "last message").with("bot", &bot).with("pane", pane);
        self.spawn_job(
            job,
            move || record.last_message(),
            move |said, _| Some(AppEvent::Bot(Box::new(BotEvent::Read { bot, read: Box::new(Read { said, ..read }) }))),
        );
    }

    pub(in crate::app) fn bot_report_read(&mut self, bot: &str, read: Read) {
        let Some(at) = self.panels.bots.by_id(bot).filter(|at| self.panels.bots.live[*at].bot.follow) else { return };
        let Read { pane, run, failed, agent, place, screen, said } = read;
        let message = said.ok().flatten().map(|said| said.text).filter(|text| !text.trim().is_empty());
        let (body, text) = match &message {
            Some(message) => (follow::body(message), follow::report(&place, pane, failed, Said::Message(message))),
            None => (follow::body(&screen), follow::report(&place, pane, failed, Said::Screen(&screen))),
        };
        let header = follow::finished(&agent.kind, &place, failed);
        let report = Report { run, agent, header, body, text, at: Instant::now() };
        self.panels.bots.live[at].reports.push(report);
    }
}

fn saved_kind(term: &Term) -> String {
    let saved = term.resume.as_ref().map(|state| state.kind.as_str());
    saved.or_else(|| term.agent.agent()).unwrap_or("agent").to_string()
}
