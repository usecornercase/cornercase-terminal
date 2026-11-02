use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use parking_lot::Mutex;

use super::{Feed, Lost, Streamed};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::files_panel::styled_after;
use crate::app::machines::MachineEvent;
use crate::changes::diff::Status;
use crate::control::contract::{Code, Refusal};
use crate::control::files::{FolderInfo, Found, Listing, SearchMode};
use crate::control::{Command, FilesList, FilesRead, FilesSearch};
use crate::files::{self, Mode, disk};
use crate::machines::heard;
use crate::panics;
use crate::text;
use crate::ui;
use crate::ui::files as panel;

#[derive(Debug)]
struct Said {
    workspace: Option<u64>,
    path: String,
    content: disk::Content,
}

const SILENT: &str = "did not answer the search";

#[derive(Debug)]
pub(crate) struct FileRead {
    stream: u64,
    turn: u64,
    said: Result<Said, Refusal>,
}

struct Queued {
    stream: u64,
    turn: u64,
    text: String,
    first: bool,
}

#[derive(Default)]
struct Reader {
    queued: Option<Queued>,
    busy: bool,
}

#[derive(Default)]
pub(in crate::app) struct RemoteFiles {
    pub(in crate::app) panel: files::Panel,
    pub(in crate::app) marks: HashMap<String, Status>,
    list: Option<Streamed>,
    read: Option<Streamed>,
    search: Option<Streamed>,
    turn: u64,
    reader: Arc<Mutex<Reader>>,
    pub(super) list_lost: Option<Lost>,
    pub(super) read_lost: Option<Lost>,
}

impl RemoteFiles {
    pub(super) fn wanted(&self, workspace: u64) -> Command {
        let folders = self.panel.wanted_folders(workspace);
        Command::FilesList(FilesList { workspace: Some(workspace), folders, follow: true })
    }

    pub(super) fn wanted_read(&self, workspace: u64) -> Option<Command> {
        let path = self.panel.viewer(workspace)?.path.clone();
        Some(Command::FilesRead(FilesRead { workspace: Some(workspace), path, follow: true }))
    }

    pub(super) fn wanted_search(&self, workspace: u64) -> Option<Command> {
        let query = self.panel.searched(workspace).filter(|query| self.panel.answered(workspace) != Some(*query))?;
        let mode = match self.panel.mode(workspace) {
            Mode::Name => SearchMode::Names,
            Mode::Text => SearchMode::Text,
        };
        Some(Command::FilesSearch(FilesSearch { workspace: Some(workspace), query: query.to_string(), mode }))
    }

    pub(super) fn slot(&mut self, feed: Feed) -> &mut Option<Streamed> {
        match feed {
            Feed::Reading => &mut self.read,
            Feed::Searching => &mut self.search,
            Feed::Listing | Feed::Changes => &mut self.list,
        }
    }

    pub(super) fn feed(&self, id: u64) -> Option<Feed> {
        let slots = [(Feed::Listing, &self.list), (Feed::Reading, &self.read), (Feed::Searching, &self.search)];
        slots.into_iter().find(|(_, slot)| Streamed::owns(slot.as_ref(), id)).map(|(feed, _)| feed)
    }

    pub(super) fn hang_up(&mut self) {
        self.list = None;
        self.read = None;
        self.search = None;
    }

    pub(super) fn lost(&mut self, feed: Feed) -> Option<&mut Option<Lost>> {
        match feed {
            Feed::Listing => Some(&mut self.list_lost),
            Feed::Reading => Some(&mut self.read_lost),
            Feed::Searching | Feed::Changes => None,
        }
    }

    pub(super) fn found_again(&mut self) {
        self.list_lost = None;
        self.read_lost = None;
    }

    fn shown_lost(&self, workspace: u64) -> Option<&Lost> {
        if self.panel.viewer(workspace).is_some() { self.read_lost.as_ref() } else { self.list_lost.as_ref() }
    }

    fn ended(&mut self, workspace: u64, feed: Feed) {
        if feed == Feed::Searching {
            if let Some(asked) = self.asked(workspace) {
                self.fed(workspace, &asked);
            }
        } else if let Some(lost) = self.lost(feed) {
            Lost::again(lost);
        }
        *self.slot(feed) = None;
    }

    fn heard(&mut self, workspace: u64, text: &str) {
        match heard::listing(text) {
            Ok(listing) if listing.workspace == Some(workspace) => self.listed(workspace, &listing),
            Err(refusal) => self.refused(&refusal),
            Ok(_) => {}
        }
    }

    fn listed(&mut self, workspace: u64, listing: &Listing) {
        self.marks = listing.folders.iter().flat_map(marks).collect();
        self.panel.fed_listing(workspace, listing.folders.iter().map(FolderInfo::listed).collect());
    }

    fn refused(&mut self, refusal: &Refusal) {
        if refusal.code == Code::StaleBoot {
            self.panel = files::Panel::default();
            self.marks.clear();
        }
    }

    fn found(&mut self, workspace: u64, text: &str) {
        match heard::found(text) {
            Ok(found) if found.workspace == workspace => self.fed(workspace, &found),
            Err(refusal) if refusal.code == Code::StaleBoot => self.refused(&refusal),
            Err(_) => {
                if let Some(asked) = self.asked(workspace) {
                    self.fed(workspace, &asked);
                }
            }
            Ok(_) => {}
        }
    }

    fn asked(&self, workspace: u64) -> Option<Found> {
        let Command::FilesSearch(search) = &self.search.as_ref()?.command else { return None };
        Some(Found { workspace, query: search.query.clone(), mode: search.mode, ..Found::default() })
    }

    fn fed(&mut self, workspace: u64, found: &Found) {
        match found.mode {
            SearchMode::Names => self.panel.fed_names(workspace, &found.query, found.found_names()),
            SearchMode::Text => self.panel.fed_text(workspace, &found.query, found.found_text()),
        }
    }
}

fn marks(folder: &FolderInfo) -> impl Iterator<Item = (String, Status)> + '_ {
    let marked = folder.entries.iter().filter_map(|entry| Some((&entry.name, entry.status?)));
    marked.map(|(name, status)| (files::child(&folder.path, name), status.into()))
}

fn post(reader: &Arc<Mutex<Reader>>, tx: &Sender<AppEvent>, queued: Queued) {
    let mut held = reader.lock();
    held.queued = Some(queued);
    if !held.busy {
        held.busy = true;
        let (reader, tx) = (Arc::clone(reader), tx.clone());
        std::thread::spawn(move || {
            while let Some(queued) = next(&reader) {
                read_and_send(&tx, &queued);
            }
        });
    }
}

fn next(reader: &Mutex<Reader>) -> Option<Queued> {
    let mut held = reader.lock();
    let queued = held.queued.take();
    held.busy = queued.is_some();
    queued
}

fn read_and_send(tx: &Sender<AppEvent>, queued: &Queued) {
    let (stream, turn) = (queued.stream, queued.turn);
    let send = |said| _ = tx.send(AppEvent::Machine(MachineEvent::Read(FileRead { stream, turn, said })));
    let Some(read) = panics::contain(|| heard::read(&queued.text)) else { return };
    let read = match read {
        Ok(read) => read,
        Err(refusal) => {
            send(Err(refusal));
            return;
        }
    };
    let Some(content) = panics::contain(|| read.content()) else { return };
    let said = |content| Said { workspace: read.workspace, path: read.path.clone(), content };
    styled_after(content, queued.first, |content, _| send(Ok(said(content))));
}

impl App {
    pub(super) fn remote_files_heard(&mut self, feed: Feed, stream: u64, text: String) {
        let Some(workspace) = self.machines.panels.aim.as_ref().map(|aim| aim.workspace) else { return };
        let files = &mut self.machines.panels.files;
        if let Some(lost) = files.lost(feed) {
            *lost = None;
        }
        match feed {
            Feed::Reading => {
                files.turn += 1;
                let first = files.panel.viewer(workspace).is_none_or(|viewer| viewer.content.is_none());
                post(&files.reader, &self.tx, Queued { stream, turn: files.turn, text, first });
            }
            Feed::Searching => {
                files.found(workspace, &text);
                self.found_answered(workspace, true);
            }
            Feed::Listing | Feed::Changes => files.heard(workspace, &text),
        }
    }

    pub(super) fn remote_files_ended(&mut self, feed: Feed, workspace: u64) {
        self.machines.panels.files.ended(workspace, feed);
        if feed == Feed::Searching {
            self.found_answered(workspace, true);
            if let Some(link) = self.machines.panels.aim.as_ref().map(|aim| aim.link) {
                self.machine_toast(link, SILENT);
            }
        }
    }

    pub(in crate::app) fn remote_file_read(&mut self, read: FileRead) {
        let Some(workspace) = self.machines.panels.aim.as_ref().map(|aim| aim.workspace) else { return };
        let files = &mut self.machines.panels.files;
        if !Streamed::owns(files.read.as_ref(), read.stream) || read.turn != files.turn {
            return;
        }
        match read.said {
            Ok(said) if said.workspace == Some(workspace) => files.panel.fed_read(workspace, &said.path, said.content),
            Err(refusal) => files.refused(&refusal),
            Ok(_) => {}
        }
    }

    pub(in crate::app) fn remote_files_view(&self) -> Option<panel::View> {
        let shown = self.shown_remote()?;
        let held = self.machines.held.iter().find(|held| held.link.id() == shown.link)?;
        let home = held.seen.as_deref().and_then(|seen| seen.home.as_deref());
        let root = text::display_path(&shown.path, home);
        let lost = self.machines.panels.files.shown_lost(shown.workspace);
        let down = self.remote_down(shown.link, lost, Instant::now());
        let remote = ui::RemoteNote { machine: shown.machine, down };
        Some(self.files_view_of(shown.workspace, root, Some(remote)))
    }
}
