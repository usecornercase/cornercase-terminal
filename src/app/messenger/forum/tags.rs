use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::Owner;
use crate::agents::activity::{NOTIFY_AFTER, Status};
use crate::app::App;
use crate::app::bots::Live;
use crate::messenger::{Order, Tags, Via};
use crate::project::{Hierarchy, Workspace};

#[derive(Debug, Default)]
pub(super) struct Labels {
    shown: HashMap<Owner, i64>,
    coming: HashMap<Owner, (i64, Instant)>,
}

impl Labels {
    pub(super) fn shown(&mut self, owner: Owner, tag_id: i64) {
        self.shown.insert(owner, tag_id);
        self.coming.remove(&owner);
    }

    pub(super) fn unshown(&mut self, owner: Owner) {
        self.shown.remove(&owner);
        self.coming.remove(&owner);
    }

    fn settled(&mut self, owner: Owner, tag_id: i64, now: Instant) -> bool {
        if self.shown.get(&owner) == Some(&tag_id) {
            self.coming.remove(&owner);
            return false;
        }
        let since = match self.coming.get(&owner) {
            Some(&(coming, since)) if coming == tag_id => since,
            _ => {
                self.coming.insert(owner, (tag_id, now));
                now
            }
        };
        if now.saturating_duration_since(since) < NOTIFY_AFTER {
            return false;
        }
        self.shown(owner, tag_id);
        true
    }

    fn keep(&mut self, alive: &HashSet<Owner>) {
        self.shown.retain(|owner, _| alive.contains(owner));
        self.coming.retain(|owner, _| alive.contains(owner));
    }
}

fn tag_of(workspace: &Workspace, tags: Tags) -> i64 {
    let statuses: Vec<Status> = workspace.terms().filter_map(|term| term.agent.status()).collect();
    if statuses.iter().any(|status| matches!(status, Status::Waiting | Status::Failed)) {
        tags.waiting
    } else if statuses.contains(&Status::Working) {
        tags.working
    } else {
        tags.done
    }
}

fn bot_tag(live: &Live, tags: Tags) -> i64 {
    if !live.asks.is_empty() {
        tags.waiting
    } else if live.answering() {
        tags.working
    } else {
        tags.done
    }
}

impl App {
    pub(in crate::app::messenger) fn owner_tag(&self, owner: Owner, tags: Tags) -> Option<i64> {
        match owner {
            Owner::Workspace(id) => Some(tag_of(self.projects.workspace_by(id)?, tags)),
            Owner::Bot(key) => self.panels.bots.by_key(key).map(|at| bot_tag(&self.panels.bots.live[at], tags)),
        }
    }

    pub(in crate::app::messenger) fn owners(&self) -> HashSet<Owner> {
        let workspaces = self.projects.workspaces().map(|workspace| Owner::Workspace(workspace.id));
        workspaces.chain(self.panels.bots.live.iter().map(|live| Owner::Bot(live.key))).collect()
    }

    pub(in crate::app) fn retag_posts(&mut self, now: Instant) {
        for via in Via::ALL {
            self.retag(via, now);
        }
    }

    fn retag(&mut self, via: Via, now: Instant) {
        let Some(tags) = self.forum_on(via).map(|forum| forum.tags) else { return };
        let posts = &self.messenger(via).forum.posts;
        let posted: Vec<(Owner, i64)> = posts.threads().into_iter().filter(|&(id, _)| !posts.archived(id)).collect();
        let wanted: Vec<(Owner, i64, i64)> =
            posted.iter().filter_map(|&(owner, thread)| Some((owner, thread, self.owner_tag(owner, tags)?))).collect();
        let alive = self.owners();
        let labels = &mut self.messenger_mut(via).forum.labels;
        labels.keep(&alive);
        let retags: Vec<Order> = wanted
            .into_iter()
            .filter(|&(owner, _, tag_id)| labels.settled(owner, tag_id, now))
            .map(|(_, thread, tag_id)| Order::Retag { thread, tag_id })
            .collect();
        for order in retags {
            self.post(via, order);
        }
    }
}
