use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::{Asker, Handled, Pending, Stage, conflict, json, malformed};
use crate::app::changes_panel::{Gapped, Seen};
use crate::app::{App, AppEvent, ChangesEvent};
use crate::changes::diff::File;
use crate::changes::{self, Mode, Scope, discard};
use crate::control::changes::{self as wire, Base, Gap, Patch, Viewed};
use crate::control::contract::{Code, Refusal};
use crate::control::{Changes, ChangesBase, ChangesDiscard, ChangesFile, ChangesGap, ChangesMode, ChangesViewed};
use crate::error::Result;
use crate::git;
use crate::log::{Job, Level};
use crate::project::{Hierarchy, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BaseChange {
    Read,
    Set(String),
    Clear,
}

#[derive(Clone, Copy)]
pub(in crate::app) struct Replied<'a> {
    pub(in crate::app) key: u64,
    pub(in crate::app) workspace: u64,
    pub(in crate::app) path: &'a str,
}

type Answer = std::result::Result<Value, Refusal>;

struct Place {
    id: u64,
    dir: std::path::PathBuf,
    base: Option<String>,
}

type Listed = (Vec<String>, Option<String>);

fn missing(path: &str, workspace: u64) -> Refusal {
    Refusal::new(
        Code::NotFound,
        format!("there is no change to {path} in workspace {workspace}; `cornercase changes list` lists them"),
    )
}

fn read_file(place: &Place, mode: Mode, path: &str) -> Result<Option<Arc<File>>> {
    changes::git::read_file(&place.dir, mode, place.base.clone(), path)
}

impl App {
    fn reading(&mut self, client: u64, key: u64) {
        let done = crate::control::Done::default();
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done,
            stage: Stage::Reading,
        });
    }

    fn changes_place(&self, asker: Asker, id: Option<u64>) -> std::result::Result<Place, Refusal> {
        let id = if let Some(id) = id {
            id
        } else {
            let here = self.here(asker).workspace;
            let at = here.ok_or_else(|| Refusal::from("no workspace is open here; pass --workspace"))?;
            self.projects.workspace(at).id
        };
        let ws = self.git_workspace(id)?;
        Ok(Place { id, dir: ws.path.clone(), base: ws.base.clone() })
    }

    pub(in crate::app) fn git_workspace(&self, id: u64) -> std::result::Result<&Workspace, Refusal> {
        let at = self.usable_workspace(id)?;
        let ws = self.projects.workspace(at);
        if self.projects[at.project()].closing {
            return Err(format!("workspace {id} is closing").into());
        }
        if git::branch(&ws.path).is_none() {
            return Err(conflict(format!("workspace {id} is not in a git repository")));
        }
        Ok(ws)
    }

    fn live_file(
        &self,
        place: &Place,
        mode: Mode,
        path: &str,
        now: Instant,
    ) -> std::result::Result<Option<Arc<File>>, Refusal> {
        let live = self.panels.changes.model_in(place.id, mode, place.base.as_deref()).filter(|m| m.live(now));
        let Some(diff) = live.and_then(changes::Model::diff) else { return Ok(None) };
        diff.files.iter().find(|f| f.path == path).cloned().map(Some).ok_or_else(|| missing(path, place.id))
    }

    pub(super) fn changes_request(&mut self, client: u64, asker: Asker, request: &Changes) -> Handled {
        if self.clients.change_streams.opened_by(client) {
            return Err(conflict("this connection already follows changes; open another connection for another scope"));
        }
        let workspace = match request.scope {
            Scope::Project => Some(self.changes_place(asker, request.workspace)?.id),
            Scope::Every => {
                if request.workspace.is_some() || request.mode != Mode::Uncommitted {
                    return Err(malformed("Every project shows only uncommitted changes, and takes no workspace"));
                }
                None
            }
        };
        let targets = if workspace.is_none() { self.every_targets() } else { Vec::new() };
        let content = self.change_content(request.scope, workspace, request.mode, &targets);
        let summary = self.clients.change_streams.start(client, request, workspace, content);
        self.panels.changes.follow(self.clients.change_streams.followed());
        Ok(Some(json(&summary)))
    }

    pub(super) fn changes_mode_request(&mut self, client: u64, request: &ChangesMode) -> Handled {
        let id = request.stream.ok_or_else(|| malformed("`changes-mode` needs the stream to change: pass --stream"))?;
        let following = self.clients.change_streams.change_mode(id, client, request.mode)?;
        self.panels.changes.follow(self.clients.change_streams.followed());
        Ok(Some(json(&following)))
    }

    pub(super) fn changes_file_request(
        &mut self,
        client: u64,
        asker: Asker,
        request: &ChangesFile,
        now: Instant,
    ) -> Handled {
        let place = self.changes_place(asker, request.workspace)?;
        let mode = request.mode;
        if let Some(file) = self.live_file(&place, mode, &request.path, now)? {
            let viewed = self.panels.changes.viewed(place.id, &file);
            return Ok(Some(json(&Patch::of(place.id, mode, &file, viewed))));
        }
        let key = self.clients.requests.key();
        let (workspace, path) = (place.id, request.path.clone());
        let reading = path.clone();
        let job = Job::new(Level::Debug, "changes", "file").with("workspace", workspace);
        self.spawn_job(
            job,
            move || read_file(&place, mode, &reading),
            move |result, _| {
                Some(AppEvent::Changes(ChangesEvent::File { request: key, workspace, mode, path, result }))
            },
        );
        self.reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn changes_file_replied(
        &mut self,
        at: Replied<'_>,
        mode: Mode,
        result: Result<Option<Arc<File>>>,
    ) {
        self.answer_read(at.key, |app| match result {
            Err(e) => Err(Refusal::from(format!("cannot read the changes: {e}"))),
            Ok(None) => Err(missing(at.path, at.workspace)),
            Ok(Some(file)) => app
                .usable_workspace(at.workspace)
                .map(|_| json(&Patch::of(at.workspace, mode, &file, app.panels.changes.viewed(at.workspace, &file)))),
        });
    }

    fn answer_read(&mut self, key: u64, reply: impl FnOnce(&mut Self) -> Answer) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let reply = reply(self);
        self.refuse_or_answer(pending.client, reply);
    }

    pub(super) fn changes_gap_request(
        &mut self,
        client: u64,
        asker: Asker,
        request: &ChangesGap,
        now: Instant,
    ) -> Handled {
        let place = self.changes_place(asker, request.workspace)?;
        let (mode, hunk) = (request.mode, request.hunk);
        let live = self.live_file(&place, mode, &request.path, now)?;
        let key = self.clients.requests.key();
        let (workspace, path) = (place.id, request.path.clone());
        let job = Job::new(Level::Debug, "changes", "gap").with("workspace", workspace);
        let read = path.clone();
        self.spawn_job(
            job,
            move || {
                let file = match live {
                    Some(file) => Some(file),
                    None => read_file(&place, mode, &read)?,
                };
                Ok(file.map(|file| {
                    let lines = (hunk >= 1 && hunk < file.hunks.len())
                        .then(|| changes::gap(&place.dir, mode, &file, hunk))
                        .flatten();
                    (file, lines)
                }))
            },
            move |result, _| {
                Some(AppEvent::Changes(ChangesEvent::GapRead { request: key, workspace, path, hunk, result }))
            },
        );
        self.reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn changes_gap_replied(&mut self, at: Replied<'_>, hunk: usize, result: Result<Gapped>) {
        self.answer_read(at.key, |_| match result {
            Err(e) => Err(Refusal::from(format!("cannot read the changes: {e}"))),
            Ok(None) => Err(missing(at.path, at.workspace)),
            Ok(Some((file, lines))) => gap_reply(at, hunk, file.hunks.len(), lines),
        });
    }

    pub(super) fn changes_base_request(&mut self, client: u64, asker: Asker, request: &ChangesBase) -> Handled {
        let change = match (&request.base, request.clear) {
            (Some(_), true) => return Err(malformed("`changes-base` takes either a branch or `clear`, not both")),
            (Some(base), false) => BaseChange::Set(base.clone()),
            (None, true) => BaseChange::Clear,
            (None, false) => BaseChange::Read,
        };
        let place = self.changes_place(asker, request.workspace)?;
        let key = self.clients.requests.key();
        let workspace = place.id;
        let job = Job::new(Level::Debug, "changes", "branches").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok((changes::git::branches(&place.dir), changes::git::default_base(&place.dir))),
            move |listed, _| Some(AppEvent::Changes(ChangesEvent::Base { request: key, workspace, change, listed })),
        );
        self.reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn changes_base_replied(
        &mut self,
        key: u64,
        workspace: u64,
        change: &BaseChange,
        listed: Result<Listed>,
    ) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let reply = match listed {
            Err(e) => Err(Refusal::from(format!("cannot list the branches: {e}"))),
            Ok((branches, default)) => self
                .usable_workspace(workspace)
                .and_then(|_| match change {
                    BaseChange::Set(base) if !branches.contains(base) => Err(Refusal::new(
                        Code::NotFound,
                        format!("workspace {workspace} has no branch {base}; `cornercase changes base` lists them"),
                    )),
                    BaseChange::Set(base) => {
                        self.set_changes_base(workspace, Some(base.clone()), default.as_deref());
                        Ok(())
                    }
                    BaseChange::Clear => {
                        self.set_changes_base(workspace, None, default.as_deref());
                        Ok(())
                    }
                    BaseChange::Read => Ok(()),
                })
                .map(|()| {
                    let base = self.projects.workspace_by(workspace).and_then(|w| w.base.clone());
                    json(&Base { workspace, base, default, branches })
                }),
        };
        self.refuse_or_answer(pending.client, reply);
    }

    pub(super) fn changes_discard_request(&mut self, client: u64, asker: Asker, request: &ChangesDiscard) -> Handled {
        let place = self.changes_place(asker, request.workspace)?;
        if request.path.is_empty() {
            return Err(malformed("`changes-discard` needs the file's path"));
        }
        let digest = seen_digest(request.digest.as_deref(), "discarding a file")?;
        let key = self.clients.requests.key();
        self.spawn_discard(Some(key), place.dir, Seen { workspace: place.id, path: request.path.clone(), digest });
        self.reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn changes_discard_replied(&mut self, at: Replied<'_>, result: Result<discard::Outcome>) {
        self.answer_read(at.key, |_| match result {
            Err(e) => Err(Refusal::from(format!("cannot discard {}: {e}", at.path))),
            Ok(discard::Outcome::Gone) => Err(missing(at.path, at.workspace)),
            Ok(discard::Outcome::Changed) => Err(conflict(format!(
                "{} changed since that digest, so nothing was discarded; read it again with `cornercase changes file`",
                at.path
            ))),
            Ok(discard::Outcome::Discarded(file)) => Ok(json(&wire::Discarded {
                workspace: at.workspace,
                path: file.path.clone(),
                old_path: file.old_path.clone(),
                status: file.status.into(),
            })),
        });
    }

    pub(super) fn changes_viewed_request(&mut self, asker: Asker, request: &ChangesViewed) -> Handled {
        let place = self.changes_place(asker, request.workspace)?;
        if request.path.is_empty() {
            return Err(malformed("`changes-viewed` needs the file's path"));
        }
        let digest = if request.clear {
            None
        } else {
            Some(seen_digest(request.digest.as_deref(), "marking a file as viewed")?)
        };
        self.panels.changes.set_viewed(place.id, &request.path, digest);
        self.clients.change_streams.mark_changed();
        Ok(Some(json(&Viewed { workspace: place.id, path: request.path.clone(), viewed: !request.clear })))
    }
}

fn seen_digest(text: Option<&str>, doing: &str) -> std::result::Result<u64, Refusal> {
    let text = text.ok_or_else(|| malformed(format!("{doing} needs the digest of the version seen: pass --digest")))?;
    wire::parse_digest(text).ok_or_else(|| malformed("a digest is 16 hexadecimal digits"))
}

fn gap_reply(at: Replied<'_>, hunk: usize, hunks: usize, lines: Option<Vec<changes::GapLine>>) -> Answer {
    let Replied { workspace, path, .. } = at;
    if hunk < 1 || hunk >= hunks {
        return Err(Refusal::new(
            Code::NotFound,
            format!("{path} has no gap before hunk {hunk}: it has {hunks} hunks"),
        ));
    }
    lines.map(|lines| json(&Gap::of(workspace, path, hunk, &lines))).ok_or_else(|| {
        Refusal::from(format!("cannot read the unchanged lines of {path}: the file is gone or too large"))
    })
}
