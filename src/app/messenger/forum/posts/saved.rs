use std::collections::BTreeMap;

use super::{Owner, Posts};
use crate::app::App;
use crate::log;
use crate::messenger::Via;
use crate::state::{DiscordPost, ProjectState};

impl Posts {
    fn saved(&self, workspace: u64, channel: i64) -> Option<DiscordPost> {
        let owner = Owner::Workspace(workspace);
        let archived = self.archived(owner);
        self.threads.get(&owner).map(|&thread| DiscordPost { channel, thread, archived })
    }

    fn bot_threads(&self) -> impl Iterator<Item = (u64, i64)> {
        self.threads.iter().filter_map(|(owner, thread)| match owner {
            Owner::Bot(key) => Some((*key, *thread)),
            Owner::Workspace(_) => None,
        })
    }
}

impl App {
    pub(in crate::app) fn save_posts(&self, projects: &mut [ProjectState]) {
        let messenger = self.messenger(Via::Discord);
        let Some(channel) = messenger.saved.forum.as_ref().map(|forum| forum.channel) else { return };
        for (saved, project) in projects.iter_mut().zip(self.projects.iter()) {
            for (saved, workspace) in saved.workspaces.iter_mut().zip(&project.workspaces) {
                saved.discord_post = messenger.forum.posts.saved(workspace.id, channel);
            }
        }
    }

    pub(in crate::app) fn restore_post(&mut self, workspace: u64, saved: Option<DiscordPost>) {
        let Some(post) = saved else { return };
        let messenger = self.messenger_mut(Via::Discord);
        if messenger.saved.forum.as_ref().is_some_and(|forum| forum.channel == post.channel) {
            let owner = Owner::Workspace(workspace);
            messenger.forum.posts.threads.insert(owner, post.thread);
            messenger.forum.posts.archive(owner, post.archived);
        } else {
            log::info!(Via::Discord.target(), "a saved post is of another forum", workspace = workspace);
        }
    }

    pub(in crate::app::messenger) fn sync_bot_posts(&mut self, via: Via) {
        if self.panels.bots.dir.is_none() {
            return;
        }
        let bots = &self.panels.bots;
        let posts: BTreeMap<String, i64> = self
            .messenger(via)
            .forum
            .posts
            .bot_threads()
            .filter_map(|(key, thread)| bots.by_key(key).map(|at| (bots.live[at].bot.id.clone(), thread)))
            .collect();
        let messenger = self.messenger_mut(via);
        if messenger.saved.bot_posts != posts {
            messenger.saved.bot_posts = posts;
            messenger.save();
        }
    }

    pub(in crate::app) fn restore_bot_posts(&mut self) {
        for via in Via::ALL {
            let bots = &self.panels.bots;
            let saved = &self.messenger(via).saved.bot_posts;
            let found: Vec<(u64, i64)> =
                saved.iter().filter_map(|(id, thread)| bots.by_id(id).map(|at| (bots.live[at].key, *thread))).collect();
            let posts = &mut self.messenger_mut(via).forum.posts;
            posts.threads.retain(|owner, _| matches!(owner, Owner::Workspace(_)));
            posts.threads.extend(found.into_iter().map(|(key, thread)| (Owner::Bot(key), thread)));
        }
    }
}
