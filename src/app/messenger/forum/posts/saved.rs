use super::Posts;
use crate::app::App;
use crate::log;
use crate::messenger::Via;
use crate::state::{DiscordPost, ProjectState};

impl Posts {
    fn saved(&self, workspace: u64, channel: i64) -> Option<DiscordPost> {
        let archived = self.archived(workspace);
        self.threads.get(&workspace).map(|&thread| DiscordPost { channel, thread, archived })
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
            messenger.forum.posts.threads.insert(workspace, post.thread);
            messenger.forum.posts.archive(workspace, post.archived);
        } else {
            log::info!(Via::Discord.target(), "a saved post is of another forum", workspace = workspace);
        }
    }
}
