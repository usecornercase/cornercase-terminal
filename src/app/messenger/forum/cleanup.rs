use crate::app::App;
use crate::config::ForumClose;
use crate::log;
use crate::messenger::{Order, Via};
use crate::project::Hierarchy;

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
        let gone: Vec<(u64, i64)> =
            posted.into_iter().filter(|&(workspace, _)| self.projects.workspace_by(workspace).is_none()).collect();
        for (workspace, thread) in gone {
            log::info!(
                via.target(),
                "the post of a closed workspace is cleaned",
                workspace = workspace,
                delete = delete
            );
            self.messenger_mut(via).forum.posts.forget_workspace(workspace);
            if delete {
                self.post(via, Order::Remove { thread });
                continue;
            }
            self.post(via, Order::Reopen { thread });
            self.post(via, Order::Retag { thread, tag_id: done });
            self.post(via, Order::Close { thread, lock: true });
        }
    }

    pub(in crate::app) fn archive_post(&mut self, workspace: u64) {
        for via in Via::ALL {
            let Some(done) = self.forum_on(via).map(|forum| forum.tags.done) else { continue };
            let forum = &mut self.messenger_mut(via).forum;
            let Some(thread) = forum.posts.thread(workspace) else { continue };
            forum.posts.archive(workspace, true);
            forum.labels.shown(workspace, done);
            self.post(via, Order::Retag { thread, tag_id: done });
            self.post(via, Order::Close { thread, lock: false });
        }
    }

    pub(in crate::app) fn unarchive_post(&mut self, workspace: u64) {
        for via in Via::ALL {
            if self.forum_on(via).is_none() {
                continue;
            }
            let posts = &mut self.messenger_mut(via).forum.posts;
            let Some(thread) = posts.thread(workspace).filter(|_| posts.archived(workspace)) else { continue };
            posts.archive(workspace, false);
            self.post(via, Order::Reopen { thread });
        }
    }
}
