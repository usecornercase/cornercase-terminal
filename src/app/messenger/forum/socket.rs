use super::super::Messenger;
use crate::app::App;
use crate::control::{ForumInfo, PostInfo};
use crate::messenger::Via;

const CHANNELS: &str = "https://discord.com/channels";

impl Messenger {
    fn pending_step(&self) -> Option<String> {
        if self.content_refused() {
            return Some(super::content_step(false).1);
        }
        let checklist = self.forum.setup.checklist.as_ref().filter(|_| !self.forum_ready())?;
        self.forum_steps(checklist).into_iter().find(|(done, _)| !done).map(|(_, step)| step)
    }
}

impl App {
    pub(in crate::app) fn forum_info(&self, via: Via) -> Option<ForumInfo> {
        let messenger = self.messenger(via);
        let on = via == Via::Discord && self.config.discord_forum;
        let saved = messenger.saved.forum.as_ref();
        if !on && saved.is_none() {
            return None;
        }
        Some(ForumInfo {
            on,
            ready: messenger.forum_ready(),
            guild: saved.map(|forum| forum.guild.to_string()),
            channel: saved.map(|forum| forum.channel.to_string()),
            pending: messenger.pending_step(),
        })
    }

    pub(in crate::app) fn post_info(&self, workspace: u64) -> Option<PostInfo> {
        let messenger = self.messenger(Via::Discord);
        let guild = messenger.saved.forum.as_ref()?.guild;
        let posts = &messenger.forum.posts;
        let thread = posts.thread(workspace)?;
        Some(PostInfo { url: format!("{CHANNELS}/{guild}/{thread}"), archived: posts.archived(workspace) })
    }
}
