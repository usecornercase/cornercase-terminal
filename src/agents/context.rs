use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agents;
use crate::agents::claude::Claude;
use crate::agents::claude::transcript::{Limits, Subagents, Transcript, transcript_path};
use crate::agents::codex::rollout as codex;
use crate::agents::gemini::session as gemini;
use crate::agents::opencode::database as opencode;

pub(in crate::agents) mod counting;
pub(in crate::agents) mod discovery;
pub(in crate::agents) mod jsonl;
pub(in crate::agents) mod message;

pub(crate) use message::{Record, Said};

use counting::Ledger;
use discovery::Job;

const TAIL: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) model: String,
    pub(crate) percent: Option<u16>,
}

#[derive(Debug, Default)]
pub(crate) struct Pane {
    transcript: Option<Transcript>,
    shown: Option<Context>,
    codex: Option<codex::Rollout>,
    agent: Option<(i32, SystemTime)>,
    kind: Option<&'static str>,
    finding: Job<Option<PathBuf>>,
    opencode: Option<opencode::Session>,
    looking: Job<(Option<opencode::Session>, opencode::Models)>,
    models: opencode::Models,
    gemini: Option<gemini::Session>,
    reading: Job<Option<gemini::Session>>,
    gemini_record: Option<(PathBuf, Option<String>)>,
    prompted: Option<SystemTime>,
    ledger: Ledger,
    subagents: Subagents,
    following: Job<Subagents>,
}

impl Pane {
    fn follow(&mut self, pid: i32, kind: &'static str) {
        if self.agent.is_none_or(|(seen, _)| seen != pid) || self.kind != Some(kind) {
            *self = Self { agent: Some((pid, SystemTime::now())), kind: Some(kind), ..Self::default() };
        }
    }

    pub(crate) fn kind(&self) -> Option<&'static str> {
        self.kind
    }

    pub(crate) fn agent(&self) -> Option<(i32, SystemTime)> {
        self.agent
    }

    pub(crate) fn take_counted(&mut self) -> agents::stats::Counted {
        self.ledger.take()
    }

    pub(crate) fn update_codex(&mut self, pid: i32) {
        self.follow(pid, agents::CODEX);
        if let Some(found) = self.finding.take()
            && found.as_ref() != self.codex.as_ref().map(|r| &r.path)
        {
            self.codex = found.map(codex::Rollout::new);
            self.shown = None;
        }
        if let Some((pid, since)) = self.agent {
            self.finding.start(move || codex::rollout_path(pid, since));
        }
        if let Some(rollout) = &mut self.codex {
            rollout.update();
            self.shown = rollout.context();
            self.prompted = self.prompted.max(rollout.prompted());
            if let Some((_, since)) = self.agent {
                self.ledger.apply(&rollout.path.to_string_lossy(), since, std::mem::take(&mut rollout.counted));
            }
        }
    }

    pub(crate) fn update_opencode(&mut self, pid: i32) {
        self.follow(pid, agents::OPENCODE);
        if let Some((mut found, models)) = self.looking.take() {
            if let Some(session) = &mut found
                && let Some((_, since)) = self.agent
                && let Some(place) = &session.place
            {
                self.ledger.apply(&place.id, since, std::mem::take(&mut session.counted));
            }
            self.models = models;
            self.shown = found.as_ref().and_then(|session| session.context.clone());
            self.prompted = self.prompted.max(found.as_ref().and_then(|session| session.prompted));
            self.opencode = found;
        }
        if !self.looking.running()
            && let Some((pid, since)) = self.agent
        {
            let mut models = std::mem::take(&mut self.models);
            self.looking.start(move || (opencode::look(pid, since, &mut models), models));
        }
    }

    pub(crate) fn update_gemini(&mut self, pid: i32, screen: &str) {
        self.follow(pid, agents::GEMINI);
        if let Some(mut found) = self.reading.take() {
            if let Some(session) = &mut found
                && let Some((_, since)) = self.agent
            {
                self.ledger.apply(&session.path.to_string_lossy(), since, std::mem::take(&mut session.counted));
            }
            self.shown = found.as_ref().and_then(|session| session.context.clone());
            self.prompted = self.prompted.max(found.as_ref().and_then(|session| session.prompted));
            self.gemini_record = found.as_ref().map(|s| (s.path.clone(), s.id().map(str::to_string)));
            self.gemini = found;
        }
        if !self.reading.running()
            && let Some((pid, since)) = self.agent
        {
            let previous = self.gemini.take();
            let footer = gemini::footer_model(screen);
            self.reading.start(move || gemini::look(pid, since, previous, footer));
        }
    }

    pub(crate) fn update(&mut self, dir: Option<&Path>, claude: Option<&Claude>) {
        if let Some(claude) = claude {
            self.follow(claude.pid, agents::CLAUDE);
        }
        let found = dir.zip(claude).and_then(|(dir, claude)| {
            let session = claude.session.as_ref()?;
            let cwd = session.cwd.as_deref()?;
            Some((dir, claude, cwd, transcript_path(dir, cwd, session.id.as_deref()?)?))
        });
        let Some((dir, claude, cwd, path)) = found else {
            if claude.is_none() {
                *self = Self::default();
            } else {
                self.transcript = None;
                self.shown = None;
            }
            return;
        };
        if self.transcript.as_ref().is_none_or(|t| t.path != path) {
            self.transcript = None;
            self.shown = None;
        }
        let transcript = self.transcript.get_or_insert_with(|| Transcript::new(path));
        let before = transcript.reply.clone();
        transcript.update();
        self.prompted = self.prompted.max(transcript.prompted);
        if let Some((_, since)) = self.agent {
            self.ledger.apply(&transcript.path.to_string_lossy(), since, std::mem::take(&mut transcript.counted));
        }
        if transcript.reply != before {
            self.shown = transcript.reply.as_ref().map(|reply| reply.context(&Limits::read(dir, cwd, claude)));
        }
        let parent = transcript.path.with_extension("").join("subagents");
        if let Some(mut found) = self.following.take() {
            if let Some((_, since)) = self.agent {
                for (path, records) in std::mem::take(&mut found.records) {
                    self.ledger.apply(&path.to_string_lossy(), since, records);
                }
            }
            self.subagents = found;
        }
        if !self.following.running()
            && let Some((_, since)) = self.agent
        {
            let state = std::mem::take(&mut self.subagents);
            self.following.start(move || state.read(&parent, since));
        }
    }

    pub(crate) fn context(&self) -> Option<&Context> {
        self.shown.as_ref()
    }

    pub(crate) fn prompted(&self) -> Option<SystemTime> {
        self.prompted
    }

    pub(crate) fn codex_turn(&self) -> bool {
        self.codex.as_ref().is_some_and(codex::Rollout::turn)
    }

    pub(crate) fn opencode_turn(&self) -> Option<bool> {
        self.opencode.as_ref().map(|session| session.turn)
    }

    pub(crate) fn record_for(&self, agent: &str) -> Option<Record> {
        Some(match agent {
            agents::CLAUDE => Record::Claude(self.transcript.as_ref()?.path.clone()),
            agents::CODEX => Record::Codex(self.codex.as_ref()?.path.clone()),
            agents::GEMINI => Record::Gemini(self.gemini_record.as_ref()?.0.clone()),
            agents::OPENCODE => {
                let place = self.opencode.as_ref()?.place.clone()?;
                Record::Opencode { database: place.database, session: place.id }
            }
            _ => return None,
        })
    }

    pub(crate) fn conversation(&self) -> Option<&str> {
        let claude = self.transcript.as_ref().filter(|t| t.len > 0).and_then(|t| t.path.file_stem()?.to_str());
        claude.or_else(|| self.codex.as_ref()?.id()).or_else(|| self.gemini_record.as_ref()?.1.as_deref())
    }
}

pub(in crate::agents) fn percent(tokens: u64, window: u64) -> u16 {
    let window = window.max(1);
    let rounded = tokens.saturating_mul(100).saturating_add(window / 2) / window;
    u16::try_from(rounded.min(100)).unwrap_or(100)
}

#[cfg(test)]
pub(in crate::agents) mod tests;
