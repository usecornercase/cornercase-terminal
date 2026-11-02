use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Feed, Lost, RemoteAim, Streamed};
use crate::app::App;
use crate::app::changes_panel::{ChangesList, single_body};
use crate::changes::diff::{Diff, File};
use crate::changes::{self, Mode, Scope, filter};
use crate::control::changes::{FileInfo, State, Summary};
use crate::control::contract::{Code, Refusal};
use crate::control::{Changes, ChangesFile, ChangesGap, Command};
use crate::machines::follow::Follow;
use crate::machines::heard;
use crate::ui;
use crate::ui::changes as panel;

const PATIENCE: Duration = Duration::from_secs(45);
const LATE: &str = "did not answer in time";
const UNREAD: &str = "cannot read the changes";

type Key = (Mode, String);

struct Listed {
    mode: Mode,
    label: Option<String>,
    files: Vec<FileInfo>,
}

struct Asked {
    follow: Follow,
    mode: Mode,
    path: String,
    digest: u64,
    hunk: usize,
    at: Instant,
}

impl Asked {
    fn owns(slot: Option<&Self>, id: u64) -> bool {
        slot.is_some_and(|asked| asked.follow.id() == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Stream,
    Patch,
    Gap,
}

pub(in crate::app) struct RemoteChanges {
    pub(in crate::app) panel: changes::Panel,
    pub(super) stream: Option<Streamed>,
    pub(super) lost: Option<Lost>,
    listed: Option<Listed>,
    patches: HashMap<Key, Arc<File>>,
    tried: HashMap<Key, u64>,
    patch: Option<Asked>,
    gap: Option<Asked>,
}

impl Default for RemoteChanges {
    fn default() -> Self {
        Self {
            panel: changes::Panel::lazy(),
            stream: None,
            lost: None,
            listed: None,
            patches: HashMap::new(),
            tried: HashMap::new(),
            patch: None,
            gap: None,
        }
    }
}

impl RemoteChanges {
    pub(super) fn wanted(&self, workspace: u64) -> Option<Command> {
        (self.panel.scope() == Scope::Project).then(|| {
            Command::Changes(Changes { workspace: Some(workspace), scope: Scope::Project, mode: self.panel.mode() })
        })
    }

    pub(super) fn hang_up(&mut self) {
        self.stream = None;
        if let Some(asked) = self.patch.take() {
            self.tried.remove(&(asked.mode, asked.path));
        }
        self.gap = None;
    }

    pub(in crate::app) fn try_again(&mut self, path: &str) {
        let key = (self.panel.mode(), path.to_string());
        if self.patch.as_ref().is_some_and(|asked| asked.mode == key.0 && asked.path == key.1) {
            self.patch = None;
        }
        if !self.patches.contains_key(&key) {
            self.tried.remove(&key);
        }
    }

    pub(super) fn slot(&self, id: u64) -> Option<Slot> {
        if Streamed::owns(self.stream.as_ref(), id) {
            Some(Slot::Stream)
        } else if Asked::owns(self.patch.as_ref(), id) {
            Some(Slot::Patch)
        } else if Asked::owns(self.gap.as_ref(), id) {
            Some(Slot::Gap)
        } else {
            None
        }
    }

    pub(super) fn heard(&mut self, slot: Slot, workspace: u64, text: &str) {
        match slot {
            Slot::Stream => {
                self.lost = None;
                match heard::summary(text) {
                    Ok(summary) => self.summarised(workspace, summary),
                    Err(refusal) => self.failed(workspace, refusal.message),
                }
            }
            Slot::Patch => self.patched(workspace, text),
            Slot::Gap => self.gapped(workspace, text),
        }
    }

    pub(super) fn ended(&mut self, slot: Slot) {
        match slot {
            Slot::Stream => {
                self.stream = None;
                Lost::again(&mut self.lost);
            }
            Slot::Patch => self.patch = None,
            Slot::Gap => self.gap = None,
        }
    }

    fn summarised(&mut self, workspace: u64, summary: Summary) {
        let content = summary.content;
        match content.state {
            State::Loading => {}
            State::Failed => self.failed(workspace, content.error.unwrap_or_else(|| UNREAD.into())),
            State::Ready => {
                let found = content.workspaces.into_iter().find(|w| w.workspace == workspace);
                let (label, files) = found.map_or((None, Vec::new()), |w| (w.base, w.files));
                self.mark_viewed(workspace, &files);
                let kept = |(mode, path): &Key, digest: u64| {
                    *mode == content.mode && files.iter().any(|info| info.path == *path && info.file().digest == digest)
                };
                self.patches.retain(|key, file| kept(key, file.digest));
                self.tried.retain(|key, digest| kept(key, *digest));
                self.listed = Some(Listed { mode: content.mode, label, files });
                self.show(workspace);
            }
        }
    }

    fn mark_viewed(&mut self, workspace: u64, files: &[FileInfo]) {
        for info in files {
            let file = info.file();
            if info.viewed != self.panel.viewed(workspace, &file) {
                self.panel.set_viewed(workspace, &file.path, info.viewed.then_some(file.viewed_digest));
            }
        }
    }

    fn failed(&mut self, workspace: u64, message: String) {
        self.listed = None;
        self.panel.shown(workspace, self.panel.mode(), None, Err(message));
    }

    fn show(&mut self, workspace: u64) {
        let Some(listed) = &self.listed else { return };
        let (mode, label) = (listed.mode, listed.label.clone());
        let files = listed.files.iter().map(|info| self.file_of(mode, info)).collect();
        self.panel.shown(workspace, mode, label, Ok(Arc::new(Diff { files })));
    }

    fn file_of(&self, mode: Mode, info: &FileInfo) -> Arc<File> {
        let file = info.file();
        let patched = self.patches.get(&(mode, info.path.clone())).filter(|patched| patched.digest == file.digest);
        patched.map_or_else(|| Arc::new(file), Arc::clone)
    }

    fn shown_diff(&self, workspace: u64) -> Option<&Arc<Diff>> {
        self.listed.as_ref().filter(|listed| listed.mode == self.panel.mode())?;
        self.panel.model(workspace, None)?.diff()
    }

    fn wanted_patch(&self, workspace: u64) -> Option<(String, u64)> {
        let diff = self.shown_diff(workspace)?;
        let kept = self.panel.filter().map(|f| filter::kept(diff, f.query()));
        let mode = self.panel.mode();
        let shows = |(i, file): &(usize, &Arc<File>)| {
            kept.as_ref().is_none_or(|kept| kept[*i])
                && !self.panel.folded(workspace, diff, file)
                && self.tried.get(&(mode, file.path.clone())) != Some(&file.digest)
        };
        let (_, file) = diff.files.iter().enumerate().find(shows)?;
        Some((file.path.clone(), file.digest))
    }

    fn patched(&mut self, workspace: u64, text: &str) {
        let Some(asked) = self.patch.take() else { return };
        match heard::patch(text) {
            Ok(patch) if patch.workspace == workspace && patch.mode == asked.mode && patch.file.path == asked.path => {
                self.patches.insert((asked.mode, asked.path), Arc::new(patch.file()));
                self.show(workspace);
            }
            Err(refusal) => self.refused(workspace, refusal),
            Ok(_) => {}
        }
    }

    fn gapped(&mut self, workspace: u64, text: &str) {
        let Some(asked) = self.gap.take() else { return };
        match heard::gap(text) {
            Ok(gap) if gap.workspace == workspace && gap.path == asked.path && gap.hunk == asked.hunk => {
                let same = |file: &&Arc<File>| file.path == asked.path && file.digest == asked.digest;
                let shown = self.shown_diff(workspace).filter(|_| self.panel.mode() == asked.mode);
                if let Some(file) = shown.and_then(|diff| diff.files.iter().find(same)).cloned() {
                    self.panel.set_gap(workspace, &file, asked.hunk, gap.lines());
                }
            }
            Err(refusal) => self.refused(workspace, refusal),
            Ok(_) => {}
        }
    }

    fn refused(&mut self, workspace: u64, refusal: Refusal) {
        if refusal.code == Code::StaleBoot {
            self.failed(workspace, refusal.message);
        }
    }

    fn expire(&mut self, now: Instant) -> bool {
        let mode = self.panel.mode();
        let mut late = false;
        for slot in [&mut self.patch, &mut self.gap] {
            if slot.as_ref().is_some_and(|asked| asked.mode != mode) {
                *slot = None;
            }
            if slot.as_ref().is_some_and(|asked| now.saturating_duration_since(asked.at) > PATIENCE) {
                *slot = None;
                late = true;
            }
        }
        late
    }
}

impl App {
    pub(in crate::app) fn remote_changes_offered(&self) -> bool {
        self.shown_place().is_some_and(|(_, workspace)| workspace.branch.is_some())
    }

    pub(in crate::app) fn remote_changes_files(&self) -> Option<usize> {
        let (_, workspace) = self.shown_place()?;
        let aim = self.remote_aim()?;
        let remote = &self.machines.panels.changes;
        let current = self.machines.panels.aim.as_ref() == Some(&aim) && remote.stream.is_some();
        let streamed = remote.shown_diff(aim.workspace).filter(|_| current).map(|diff| diff.files.len());
        let mode = remote.panel.mode();
        let counted = workspace.changes.as_ref().filter(|count| count.mode == mode).map(|count| count.files);
        streamed.or_else(|| counted.and_then(|files| usize::try_from(files).ok()))
    }

    pub(in crate::app) fn remote_changes_view(&self) -> Option<panel::View> {
        let aim = self.remote_aim()?;
        let (project, workspace) = self.shown_place()?;
        let remote = &self.machines.panels.changes;
        let current = self.machines.panels.aim.as_ref() == Some(&aim);
        let model = remote.panel.model(aim.workspace, None).filter(|_| current);
        let machine = self.shown_remote()?.machine;
        Some(self.listed_view(ChangesList {
            project: format!("{} › {} · {machine}", project.name, workspace.name),
            mode: remote.panel.mode(),
            base: remote.panel.label(aim.workspace).filter(|_| current),
            live: current && remote.stream.is_some() && self.linked(aim.link),
            listed: single_body(aim.workspace, model),
            remote: Some(ui::RemoteNote {
                machine,
                down: self.remote_down(aim.link, remote.lost.as_ref(), Instant::now()),
            }),
        }))
    }

    pub(super) fn ask_remote_changes(&mut self, aim: &RemoteAim, reachable: bool, now: Instant) {
        if self.machines.panels.changes.expire(now) {
            self.machine_toast(aim.link, LATE);
        }
        let remote = &self.machines.panels.changes;
        if !reachable || remote.patch.is_some() {
            return;
        }
        let Some((path, digest)) = remote.wanted_patch(aim.workspace) else { return };
        let mode = remote.panel.mode();
        self.machines.panels.changes.tried.insert((mode, path.clone()), digest);
        let command = ChangesFile { workspace: Some(aim.workspace), path: path.clone(), mode };
        let follow = self.follow_machine(aim, Command::ChangesFile(command), true);
        self.machines.panels.changes.patch =
            follow.map(|follow| Asked { follow, mode, path, digest, hunk: 0, at: now });
    }

    pub(in crate::app) fn ask_remote_gap(&mut self, file: &File, hunk: usize) {
        if self.cut_off(Feed::Changes) {
            return;
        }
        let Some(aim) = self.machines.panels.aim.clone().filter(|_| self.has_window()) else { return };
        let mode = self.machines.panels.changes.panel.mode();
        let path = file.path.clone();
        let command = ChangesGap { workspace: Some(aim.workspace), path: path.clone(), hunk, mode };
        let follow = self.follow_machine(&aim, Command::ChangesGap(command), true);
        let at = Instant::now();
        self.machines.panels.changes.gap =
            follow.map(|follow| Asked { follow, mode, path, digest: file.digest, hunk, at });
    }
}
