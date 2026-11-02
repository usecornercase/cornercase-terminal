use std::time::{Duration, Instant, SystemTime};

use super::{Asker, Handled, Pending, Reply, Stage, conflict, json};
use crate::app::App;
use crate::control::{ContextInfo, ContextRead, Done};
use crate::project::Hierarchy;

const NOT_YET: &str = "the agent has not said yet which conversation it is in";

impl App {
    pub(super) fn context_request(&mut self, client: u64, asker: Asker, request: &ContextRead) -> Handled {
        let pane = self.target(asker, request.pane, request.tab)?;
        let term = self.projects.pane_by(pane).filter(|term| term.context.kind().is_some());
        let Some(kind) = term.and_then(|term| term.context.kind()) else {
            return Err(conflict(format!("pane {pane} runs no agent cornercase reads")));
        };
        if request.cached || kind.breakdown().is_none() {
            return Ok(Some(json(&self.context_info(pane))));
        }
        if term.is_some_and(|term| term.context.agent().is_none()) {
            return Ok(Some(json(&ContextInfo { error: Some(NOT_YET.into()), ..self.context_info(pane) })));
        }
        self.read_context(pane);
        let probe = self.agents.contexts.reads.get(&pane).map(|read| &read.probe);
        let Some(answers) = probe.filter(|probe| probe.loading()).map(crate::agents::probe::Probe::answers) else {
            return Ok(Some(json(&self.context_info(pane))));
        };
        let key = self.clients.requests.key();
        let stage = Stage::Context { pane, answers };
        let seconds = self.agents.contexts.timeout + Duration::from_secs(5);
        let timeout = Some((Instant::now() + seconds, seconds.as_secs_f64()));
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout,
            done: Done::default(),
            stage,
        });
        Ok(None)
    }

    pub(super) fn context_reply(&self, pane: u64, before: u64) -> Option<Reply> {
        if self.projects.pane_by(pane).is_none() {
            return Some(Err(format!("pane {pane} closed before it said what fills its context")));
        }
        let answers = self.agents.contexts.reads.get(&pane).map_or(0, |read| read.probe.answers());
        (answers > before).then(|| Ok(json(&self.context_info(pane))))
    }

    fn context_info(&self, pane: u64) -> ContextInfo {
        let Some(term) = self.projects.pane_by(pane) else { return ContextInfo { pane, ..ContextInfo::default() } };
        let kind = term.context.kind();
        let shown = term.context.context();
        let read = self.agents.contexts.reads.get(&pane).map(|read| &read.probe);
        let notice = kind
            .filter(|kind| kind.breakdown().is_none())
            .map(|kind| format!("{} does not say what fills its context", kind.title()));
        let info = ContextInfo {
            pane,
            agent: kind.map(|kind| kind.name().to_string()),
            title: kind.map(|kind| kind.title().to_string()),
            model: shown.map(|shown| shown.model.clone()),
            percent: shown.and_then(|shown| shown.percent),
            compacted: term.context.compaction().hides,
            loading: read.is_some_and(crate::agents::probe::Probe::loading),
            breakdown: read.and_then(|probe| probe.last()).map(|(breakdown, _)| breakdown.clone()),
            notice,
            error: read.and_then(|probe| probe.error()).map(str::to_string),
            ..ContextInfo::default()
        };
        let age = read.and_then(|probe| probe.last()).map(|(_, at)| Instant::now().saturating_duration_since(*at));
        info.read(age, SystemTime::now())
    }
}
