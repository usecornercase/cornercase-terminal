use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::agents::activity::{NOTIFY_AFTER, Status};
use crate::app::App;
use crate::messenger::{Order, Tags, Via};
use crate::project::{Hierarchy, Workspace};

#[derive(Debug, Default)]
pub(super) struct Labels {
    shown: HashMap<u64, i64>,
    coming: HashMap<u64, (i64, Instant)>,
}

impl Labels {
    pub(super) fn shown(&mut self, workspace: u64, tag_id: i64) {
        self.shown.insert(workspace, tag_id);
        self.coming.remove(&workspace);
    }

    pub(super) fn unshown(&mut self, workspace: u64) {
        self.shown.remove(&workspace);
        self.coming.remove(&workspace);
    }

    fn settled(&mut self, workspace: u64, tag_id: i64, now: Instant) -> bool {
        if self.shown.get(&workspace) == Some(&tag_id) {
            self.coming.remove(&workspace);
            return false;
        }
        let since = match self.coming.get(&workspace) {
            Some(&(coming, since)) if coming == tag_id => since,
            _ => {
                self.coming.insert(workspace, (tag_id, now));
                now
            }
        };
        if now.saturating_duration_since(since) < NOTIFY_AFTER {
            return false;
        }
        self.shown(workspace, tag_id);
        true
    }

    fn keep(&mut self, alive: &HashSet<u64>) {
        self.shown.retain(|workspace, _| alive.contains(workspace));
        self.coming.retain(|workspace, _| alive.contains(workspace));
    }
}

pub(super) fn tag_of(workspace: &Workspace, tags: Tags) -> i64 {
    let statuses: Vec<Status> = workspace.terms().filter_map(|term| term.agent.status()).collect();
    if statuses.iter().any(|status| matches!(status, Status::Waiting | Status::Failed)) {
        tags.waiting
    } else if statuses.contains(&Status::Working) {
        tags.working
    } else {
        tags.done
    }
}

impl App {
    pub(in crate::app) fn retag_posts(&mut self, now: Instant) {
        for via in Via::ALL {
            self.retag(via, now);
        }
    }

    fn retag(&mut self, via: Via, now: Instant) {
        let Some(tags) = self.forum_on(via).map(|forum| forum.tags) else { return };
        let posts = &self.messenger(via).forum.posts;
        let posted: Vec<(u64, i64)> = posts.threads().into_iter().filter(|&(id, _)| !posts.archived(id)).collect();
        let wanted: Vec<(u64, i64, i64)> = posted
            .iter()
            .filter_map(|&(id, thread)| Some((id, thread, tag_of(self.projects.workspace_by(id)?, tags))))
            .collect();
        let alive: HashSet<u64> = self.projects.workspaces().map(|workspace| workspace.id).collect();
        let labels = &mut self.messenger_mut(via).forum.labels;
        labels.keep(&alive);
        let retags: Vec<Order> = wanted
            .into_iter()
            .filter(|&(id, _, tag_id)| labels.settled(id, tag_id, now))
            .map(|(_, thread, tag_id)| Order::Retag { thread, tag_id })
            .collect();
        for order in retags {
            self.post(via, order);
        }
    }
}
