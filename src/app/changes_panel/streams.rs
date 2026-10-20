use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::app::events::Streamed;
use crate::changes::{self, Body, Checkout, Mode, Scope};
use crate::control::changes::{Content, Following, State, Summary, WorkspaceChanges};
use crate::control::contract::{Code, Refusal};
use crate::control::{self, ChangesCount};
use crate::log;
use crate::project::Hierarchy;

pub(in crate::app) const FEED_EVERY: Duration = Duration::from_millis(100);

struct Stream {
    id: u64,
    client: u64,
    scope: Scope,
    workspace: Option<u64>,
    mode: Mode,
    revision: u64,
    last: Option<Content>,
}

pub(in crate::app) struct Streams {
    open: Vec<Stream>,
    issued: u64,
    changed: bool,
    built: Option<Instant>,
    every: Duration,
}

impl Default for Streams {
    fn default() -> Self {
        Self { open: Vec::new(), issued: 0, changed: false, built: None, every: FEED_EVERY }
    }
}

impl Streams {
    pub(in crate::app) fn followed(&self) -> HashSet<(u64, Mode)> {
        self.open.iter().filter_map(|s| s.workspace.map(|w| (w, s.mode))).collect()
    }

    pub(in crate::app) fn wants_every(&self) -> bool {
        self.open.iter().any(|s| s.scope == Scope::Every)
    }

    pub(in crate::app) fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    pub(in crate::app) fn opened_by(&self, client: u64) -> bool {
        self.open.iter().any(|s| s.client == client)
    }

    pub(in crate::app) fn mark_changed(&mut self) {
        self.changed = true;
    }

    pub(in crate::app) fn start(
        &mut self,
        client: u64,
        request: &control::Changes,
        workspace: Option<u64>,
        content: Content,
    ) -> Summary {
        self.issued += 1;
        let stream = Stream {
            id: self.issued,
            client,
            scope: request.scope,
            workspace,
            mode: request.mode,
            revision: 1,
            last: Some(content.clone()),
        };
        let summary = summary_of(&stream, content);
        log::info!(
            "control",
            "changes",
            client = client,
            stream = stream.id,
            scope = request.scope.name(),
            workspace = workspace.unwrap_or_default(),
            mode = request.mode.name()
        );
        self.open.push(stream);
        self.changed = true;
        summary
    }

    pub(in crate::app) fn change_mode(&mut self, id: u64, client: u64, mode: Mode) -> Result<Following, Refusal> {
        let Some(stream) = self.open.iter_mut().find(|s| s.id == id && s.client == client) else {
            return Err(Refusal::new(Code::NotFound, format!("there is no changes stream {id} on this connection")));
        };
        if stream.scope == Scope::Every && mode != Mode::Uncommitted {
            return Err(Refusal::new(Code::Malformed, "Every project shows only uncommitted changes"));
        }
        stream.mode = mode;
        self.changed = true;
        Ok(Following { stream: id, scope: stream.scope, workspace: stream.workspace, mode })
    }

    #[cfg(test)]
    pub(in crate::app) fn set_every(&mut self, every: Duration) {
        self.every = every;
    }

    #[cfg(test)]
    pub(in crate::app) fn mode_of(&self, stream: u64) -> Option<Mode> {
        self.open.iter().find(|s| s.id == stream).map(|s| s.mode)
    }

    pub(in crate::app) fn wake(&self, now: Instant) -> Option<Duration> {
        if !self.changed || self.open.is_empty() {
            return None;
        }
        let waited = self.built.map_or(self.every, |at| now.saturating_duration_since(at));
        Some(self.every.saturating_sub(waited))
    }
}

fn summary_of(stream: &Stream, content: Content) -> Summary {
    Summary { boot: control::server_token().to_string(), stream: stream.id, revision: stream.revision, content }
}

impl App {
    pub(in crate::app) fn forget_change_streams(&mut self, client: u64) {
        let before = self.clients.change_streams.open.len();
        self.clients.change_streams.open.retain(|s| s.client != client);
        if self.clients.change_streams.open.len() != before {
            self.panels.changes.follow(self.clients.change_streams.followed());
        }
    }

    pub(in crate::app) fn end_gone_streams(&mut self) {
        let gone: Vec<usize> = self
            .clients
            .change_streams
            .open
            .iter()
            .enumerate()
            .filter(|(_, s)| s.workspace.is_some_and(|w| !self.change_workspace_usable(w)))
            .map(|(i, _)| i)
            .collect();
        if gone.is_empty() {
            return;
        }
        for i in gone.into_iter().rev() {
            let stream = self.clients.change_streams.open.remove(i);
            log::info!(
                "control",
                "changes ended",
                client = stream.client,
                stream = stream.id,
                reason = "workspace gone"
            );
            self.clients.events.push(stream.client, Streamed::End);
        }
        self.panels.changes.follow(self.clients.change_streams.followed());
    }

    fn change_workspace_usable(&self, workspace: u64) -> bool {
        self.projects
            .workspace_index(workspace)
            .is_some_and(|(p, w)| !self.projects[p].closing && self.projects[p].workspaces[w].open())
    }

    pub(in crate::app) fn change_content(
        &self,
        scope: Scope,
        workspace: Option<u64>,
        mode: Mode,
        targets: &[(Checkout, bool)],
    ) -> Content {
        let mut content = Content { scope, mode, ..Content::default() };
        match scope {
            Scope::Project => self.project_content(&mut content, workspace, mode),
            Scope::Every => self.every_content(&mut content, targets),
        }
        content
    }

    fn project_content(&self, content: &mut Content, workspace: Option<u64>, mode: Mode) {
        let Some((p, w)) = workspace.and_then(|id| self.projects.workspace_index(id)) else { return };
        let ws = &self.projects[p].workspaces[w];
        match self.panels.changes.model_in(ws.id, mode, ws.base.as_deref()).map(|m| &m.result) {
            None => {}
            Some(Err(e)) => {
                content.state = State::Failed;
                content.error = Some(e.clone());
            }
            Some(Ok(diff)) => {
                content.state = State::Ready;
                let base = (mode != Mode::Uncommitted)
                    .then(|| self.panels.changes.label_in(ws.id, mode).or_else(|| ws.base.clone()))
                    .flatten();
                let viewed = |f: &changes::diff::File| self.panels.changes.viewed(ws.id, f);
                content.workspaces.push(WorkspaceChanges::of(self.projects[p].id, ws.id, base, diff, viewed));
            }
        }
    }

    fn every_content(&self, content: &mut Content, targets: &[(Checkout, bool)]) {
        let (body, owners, _) = self.every_body(targets);
        match body {
            Body::Loading => {}
            Body::Failed(e) => {
                content.state = State::Failed;
                content.error = Some(e);
            }
            Body::Ready(diff) => {
                content.state = State::Ready;
                self.owner_runs(content, &diff, &owners);
            }
        }
    }

    fn owner_runs(&self, content: &mut Content, diff: &changes::diff::Diff, owners: &[u64]) {
        let mut start = 0;
        while start < owners.len() {
            let owner = owners[start];
            let len = owners[start..].iter().take_while(|o| **o == owner).count();
            let part = changes::diff::Diff { files: diff.files[start..start + len].to_vec() };
            if let Some((p, _)) = self.projects.workspace_index(owner) {
                let viewed = |f: &changes::diff::File| self.panels.changes.viewed(owner, f);
                content.workspaces.push(WorkspaceChanges::of(self.projects[p].id, owner, None, &part, viewed));
            }
            start += len;
        }
    }

    pub(in crate::app) fn publish_change_streams(&mut self, targets: &[(Checkout, bool)], now: Instant) {
        let streams = &mut self.clients.change_streams;
        if streams.open.is_empty() {
            streams.changed = false;
            return;
        }
        if !streams.changed || streams.built.is_some_and(|at| now.saturating_duration_since(at) < streams.every) {
            return;
        }
        streams.built = Some(now);
        streams.changed = false;
        let mut list = std::mem::take(&mut self.clients.change_streams.open);
        for stream in &mut list {
            let content = self.change_content(stream.scope, stream.workspace, stream.mode, targets);
            if stream.last.as_ref() == Some(&content) {
                continue;
            }
            stream.revision += 1;
            stream.last = Some(content.clone());
            let summary = summary_of(stream, content);
            let Some(text) = summary.streamed() else { continue };
            log::debug!(
                "control",
                "changes",
                stream = stream.id,
                revision = stream.revision,
                files = summary.content.workspaces.iter().map(|w| w.files.len()).sum::<usize>(),
                bytes = text.len()
            );
            self.clients.events.push(stream.client, Streamed::Snapshot(text));
        }
        list.append(&mut self.clients.change_streams.open);
        self.clients.change_streams.open = list;
    }

    pub(in crate::app) fn changes_count(
        &self,
        workspace: u64,
        target: Option<u64>,
        sweeping: bool,
    ) -> Option<ChangesCount> {
        let counted = |diff: &changes::diff::Diff, mode| ChangesCount {
            files: diff.files.len() as u64,
            added: diff.added() as u64,
            removed: diff.removed() as u64,
            mode,
        };
        let (_, w) = self.projects.workspace_index(workspace).map(|(p, w)| (p, &self.projects[p].workspaces[w]))?;
        if target == Some(workspace) {
            let every = self.panels.changes.scope == Scope::Every && self.changes_shown();
            let model = if every {
                self.panels.changes.summary(workspace)
            } else {
                self.panels.changes.model(workspace, w.base.as_deref())
            };
            let mode = if every { Mode::Uncommitted } else { self.panels.changes.mode };
            return model.and_then(changes::Model::diff).map(|d| counted(d, mode));
        }
        let open = &self.clients.change_streams.open;
        let mut modes: Vec<Mode> = open.iter().filter(|s| s.workspace == Some(workspace)).map(|s| s.mode).collect();
        modes.sort_by_key(|m| *m != Mode::Uncommitted);
        if let Some(mode) = modes.first() {
            return self
                .panels
                .changes
                .model_in(workspace, *mode, w.base.as_deref())
                .and_then(changes::Model::diff)
                .map(|d| counted(d, *mode));
        }
        if sweeping {
            return self
                .panels
                .changes
                .summary(workspace)
                .and_then(changes::Model::diff)
                .map(|d| counted(d, Mode::Uncommitted));
        }
        None
    }
}
