use std::time::{Duration, Instant};

use super::MachineEvent;
use super::card::counting;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::layout::Side;
use crate::app::toast::Toast;
use crate::changes::Scope;
use crate::control::Command;
use crate::log;
use crate::machines::State;
use crate::machines::follow::{Event, Follow, Followed, Wanted};
use crate::remote;
use crate::ui;

mod changes;
mod files;

pub(in crate::app) use files::FileRead;

impl From<Followed> for AppEvent {
    fn from(followed: Followed) -> Self {
        Self::Machine(MachineEvent::Followed(followed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct RemoteAim {
    pub(in crate::app) link: u64,
    pub(in crate::app) boot: String,
    pub(in crate::app) workspace: u64,
}

struct Streamed {
    follow: Follow,
    command: Command,
}

impl Streamed {
    fn owns(slot: Option<&Self>, id: u64) -> bool {
        slot.is_some_and(|streamed| streamed.follow.id() == id)
    }
}

struct Lost {
    failed: u32,
    at: Option<Instant>,
}

impl Lost {
    fn again(lost: &mut Option<Self>) {
        if lost.as_ref().is_some_and(|lost| lost.at.is_none()) {
            return;
        }
        let failed = lost.as_ref().map_or(0, |lost| lost.failed) + 1;
        *lost = Some(Self { failed, at: None });
    }

    fn due(&self) -> Option<Instant> {
        Some(self.at? + remote::backoff(self.failed))
    }

    fn held_back(&mut self, now: Instant) -> bool {
        self.at.get_or_insert(now);
        self.due().is_none_or(|due| now < due)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum Feed {
    Changes,
    Listing,
    Reading,
    Searching,
}

impl Feed {
    fn of(side: Side) -> &'static [Self] {
        match side {
            Side::Changes => &[Self::Changes],
            Side::Files => &[Self::Listing, Self::Reading, Self::Searching],
            Side::Todo | Side::Flow | Side::Recap => &[],
        }
    }
}

#[derive(Default)]
pub(in crate::app) struct RemotePanels {
    aim: Option<RemoteAim>,
    pub(in crate::app) changes: changes::RemoteChanges,
    pub(in crate::app) files: files::RemoteFiles,
    next: u64,
}

impl RemotePanels {
    pub(super) fn reset(&mut self) {
        self.aim = None;
        self.changes = changes::RemoteChanges::default();
        self.files = files::RemoteFiles::default();
    }

    pub(in crate::app) fn hang_up(&mut self) {
        self.changes.hang_up();
        self.files.hang_up();
    }

    fn close_others(&mut self, side: Option<Side>) {
        if side != Some(Side::Changes) {
            self.changes = changes::RemoteChanges::default();
        }
        if side != Some(Side::Files) {
            self.files = files::RemoteFiles::default();
        }
    }

    fn lost(&mut self, feed: Feed) -> Option<&mut Option<Lost>> {
        match feed {
            Feed::Changes => Some(&mut self.changes.lost),
            Feed::Listing | Feed::Reading | Feed::Searching => self.files.lost(feed),
        }
    }

    fn wanted(&mut self, feed: Feed, workspace: u64, now: Instant) -> Option<Command> {
        if self.lost(feed).and_then(Option::as_mut).is_some_and(|lost| lost.held_back(now)) {
            return None;
        }
        match feed {
            Feed::Changes => self.changes.wanted(workspace),
            Feed::Listing => Some(self.files.wanted(workspace)),
            Feed::Reading => self.files.wanted_read(workspace),
            Feed::Searching => self.files.wanted_search(workspace),
        }
    }

    fn slot(&mut self, feed: Feed) -> &mut Option<Streamed> {
        match feed {
            Feed::Changes => &mut self.changes.stream,
            Feed::Listing | Feed::Reading | Feed::Searching => self.files.slot(feed),
        }
    }
}

impl App {
    pub(in crate::app) fn remote_aim(&self) -> Option<RemoteAim> {
        let shown = self.shown_remote()?;
        Some(RemoteAim { link: shown.link, boot: shown.boot, workspace: shown.workspace })
    }

    pub(in crate::app) fn sync_remote_panels(&mut self, now: Instant) {
        let aim = self.remote_aim();
        if self.machines.panels.aim != aim {
            self.machines.panels.reset();
            self.machines.panels.aim.clone_from(&aim);
        }
        let Some(aim) = aim else { return };
        let side = self.screen.side;
        self.machines.panels.close_others(side);
        let Some(side) = side else { return };
        let linked = self.linked(aim.link);
        if !linked {
            self.machines.panels.changes.lost = None;
            self.machines.panels.files.found_again();
        }
        let reachable = self.has_window() && linked && self.remote_side_shown(side);
        for &feed in Feed::of(side) {
            let wanted = self.machines.panels.wanted(feed, aim.workspace, now).filter(|_| reachable);
            self.follow_wanted(&aim, feed, wanted);
        }
        if side == Side::Changes {
            self.ask_remote_changes(&aim, reachable, now);
        }
    }

    fn remote_side_shown(&self, side: Side) -> bool {
        side != Side::Changes || (self.changes_shown() && self.changes_state().scope() == Scope::Project)
    }

    fn follow_wanted(&mut self, aim: &RemoteAim, feed: Feed, wanted: Option<Command>) {
        let running = self.machines.panels.slot(feed).as_ref().map(|streamed| streamed.command.clone());
        if running == wanted {
            return;
        }
        let started = wanted.and_then(|command| self.start_follow(aim, command, feed == Feed::Searching));
        *self.machines.panels.slot(feed) = started;
    }

    fn start_follow(&mut self, aim: &RemoteAim, command: Command, once: bool) -> Option<Streamed> {
        let follow = self.follow_machine(aim, command.clone(), once)?;
        Some(Streamed { follow, command })
    }

    fn follow_machine(&mut self, aim: &RemoteAim, command: Command, once: bool) -> Option<Follow> {
        let held = self.machines.held.iter().find(|held| held.link.id() == aim.link)?;
        let id = self.machines.panels.next;
        self.machines.panels.next += 1;
        let connector = (self.machines.connect)(&held.destination);
        log::info!("machines", "panel stream", name = held.name, request = command.name(), stream = id);
        let wanted = Wanted { command, boot: aim.boot.clone(), once };
        Some(Follow::start(id, connector, &held.name, wanted, &self.tx))
    }

    pub(super) fn drop_unheld_panels(&mut self) {
        let link = self.machines.panels.aim.as_ref().map(|aim| aim.link);
        if link.is_some_and(|link| !self.machines.held.iter().any(|held| held.link.id() == link)) {
            self.machines.panels.reset();
        }
    }

    pub(super) fn remote_followed(&mut self, followed: Followed) {
        let panels = &mut self.machines.panels;
        let Some(workspace) = panels.aim.as_ref().map(|aim| aim.workspace) else { return };
        if let Some(slot) = panels.changes.slot(followed.id) {
            match followed.event {
                Event::Line(text) => panels.changes.heard(slot, workspace, &text),
                Event::Ended(_) => {
                    log::info!("machines", "panel stream ended", stream = followed.id);
                    panels.changes.ended(slot);
                }
            }
            return;
        }
        let Some(feed) = panels.files.feed(followed.id) else { return };
        match followed.event {
            Event::Line(text) => self.remote_files_heard(feed, followed.id, text),
            Event::Ended(_) => {
                log::info!("machines", "panel stream ended", stream = followed.id);
                self.remote_files_ended(feed, workspace);
            }
        }
    }

    fn machine_toast(&mut self, link: u64, what: &str) {
        let Some(name) = self.held_by(link).map(|held| held.name.clone()) else { return };
        self.messages.toast = Some(Toast::new(format!("{name} {what}"), ui::ToastIcon::Bug));
    }

    fn remote_down(&self, link: u64, lost: Option<&Lost>, now: Instant) -> Option<String> {
        let held = self.held_by(link)?;
        if held.status.state != State::Linked {
            return Some(ui::retrying(&self.retry(held, now)));
        }
        let left = lost?.due().map(|due| due.saturating_duration_since(now));
        Some(ui::retrying(&left.map_or(ui::Retry::Now, counting)))
    }

    pub(in crate::app) fn panels_wake(&self, now: Instant) -> Option<Duration> {
        let panels = &self.machines.panels;
        let lost = [&panels.changes.lost, &panels.files.list_lost, &panels.files.read_lost];
        lost.into_iter().flatten().filter_map(Lost::due).map(|due| due.saturating_duration_since(now)).min()
    }

    pub(in crate::app) fn cut_off(&mut self, feed: Feed) -> bool {
        let Some(link) = self.machines.panels.aim.as_ref().map(|aim| aim.link) else { return false };
        let lost = self.machines.panels.lost(feed).is_some_and(|lost| lost.is_some());
        let Some(held) = self.held_by(link) else { return false };
        if held.status.state == State::Linked && !lost {
            return false;
        }
        let why = self.unlinked(held);
        self.not_sent(&why);
        true
    }
}
