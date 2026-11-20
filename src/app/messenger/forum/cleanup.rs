use super::Owner;
use crate::app::App;
use crate::config::ForumClose;
use crate::log;
use crate::messenger::{Order, Via};

fn cleaned(via: Via, owner: Owner, delete: bool) {
    match owner {
        Owner::Workspace(workspace) => {
            log::info!(
                via.target(),
                "the post of a closed workspace is cleaned",
                workspace = workspace,
                delete = delete
            );
        }
        Owner::Bot(key) => log::info!(via.target(), "the post of a removed bot is cleaned", bot = key, delete = delete),
    }
}

impl App {
    pub(in crate::app) fn clean_gone_posts(&mut self) {
        for via in Via::ALL {
            self.clean_gone(via);
        }
    }

    fn clean_gone(&mut self, via: Via) {
        let Some(done) = self.forum_on(via).map(|forum| forum.tags.done) else { return };
        let delete = ForumClose::from_setting(&self.config.discord_forum_close) == ForumClose::Delete;
        let posted = self.messenger(via).forum.posts.threads();
        let alive = self.owners();
        let gone: Vec<(Owner, i64)> = posted.into_iter().filter(|(owner, _)| !alive.contains(owner)).collect();
        if gone.is_empty() {
            return;
        }
        for (owner, thread) in gone {
            cleaned(via, owner, delete);
            self.messenger_mut(via).forum.posts.forget_owner(owner);
            if delete {
                self.post(via, Order::Remove { thread });
                continue;
            }
            self.post(via, Order::Reopen { thread });
            self.post(via, Order::Retag { thread, tag_id: done });
            self.post(via, Order::Close { thread, lock: true });
        }
        self.sync_bot_posts(via);
    }

    pub(in crate::app) fn archive_post(&mut self, workspace: u64) {
        let owner = Owner::Workspace(workspace);
        for via in Via::ALL {
            let Some(done) = self.forum_on(via).map(|forum| forum.tags.done) else { continue };
            let forum = &mut self.messenger_mut(via).forum;
            let Some(thread) = forum.posts.thread(owner) else { continue };
            forum.posts.archive(owner, true);
            forum.labels.shown(owner, done);
            self.post(via, Order::Retag { thread, tag_id: done });
            self.post(via, Order::Close { thread, lock: false });
        }
    }

    pub(in crate::app) fn unarchive_post(&mut self, workspace: u64) {
        let owner = Owner::Workspace(workspace);
        for via in Via::ALL {
            if self.forum_on(via).is_none() {
                continue;
            }
            let posts = &mut self.messenger_mut(via).forum.posts;
            let Some(thread) = posts.thread(owner).filter(|_| posts.archived(owner)) else { continue };
            posts.archive(owner, false);
            self.post(via, Order::Reopen { thread });
        }
    }
}
