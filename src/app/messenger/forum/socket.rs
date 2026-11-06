use super::super::Messenger;
use super::PICK_A_SERVER;
use crate::app::App;
use crate::app::control::{Handled, conflict, json};
use crate::control::contract::{Code, Refusal};
use crate::control::{ForumInfo, ForumServer, ForumServerInfo, PostInfo};
use crate::messenger::Via;

const CHANNELS: &str = "https://discord.com/channels";
const NO_BOT: &str = "there is no Discord bot yet: `cornercase config secret discord` takes its token";
const NOT_PAIRED: &str = "the Discord bot is not paired yet: pair it in Settings › Remote › Discord";
const MODE_OFF: &str = "forum mode is off: `cornercase config set discord_forum true` turns it on";
const NOT_READ: &str = "Discord was not read yet; `cornercase status --app` lists the servers once it is";

impl Messenger {
    fn pending_step(&self) -> Option<String> {
        if self.content_refused() {
            return Some(super::content_step(false).1);
        }
        let checklist = self.forum.setup.checklist.as_ref().filter(|_| self.paired() && !self.forum_ready())?;
        self.forum_steps(checklist).into_iter().find(|(done, _)| !done).map(|(_, step)| step)
    }

    fn servers_to_pick(&self, pending: Option<&str>) -> Vec<ForumServerInfo> {
        let Some(checklist) = self.forum.setup.checklist.as_ref().filter(|_| pending == Some(PICK_A_SERVER)) else {
            return Vec::new();
        };
        let servers = checklist.servers.iter();
        servers.map(|server| ForumServerInfo { id: server.id.to_string(), name: server.name.clone() }).collect()
    }
}

fn not_listed(id: &str, servers: &[ForumServerInfo]) -> String {
    let listed: Vec<String> = servers.iter().map(|server| format!("{} ({})", server.id, server.name)).collect();
    format!("`{id}` is not a server the forum can go in; they are: {}", listed.join(", "))
}

impl App {
    pub(in crate::app) fn forum_info(&self, via: Via) -> Option<ForumInfo> {
        let messenger = self.messenger(via);
        let on = via == Via::Discord && self.config.discord_forum;
        let saved = messenger.saved.forum.as_ref();
        if !on && saved.is_none() {
            return None;
        }
        let pending = messenger.pending_step();
        Some(ForumInfo {
            on,
            ready: messenger.forum_ready(),
            guild: saved.map(|forum| forum.guild.to_string()),
            channel: saved.map(|forum| forum.channel.to_string()),
            servers: messenger.servers_to_pick(pending.as_deref()),
            pending,
        })
    }

    pub(in crate::app) fn forum_server_request(&mut self, pick: &ForumServer) -> Handled {
        let servers = self.forum_info(Via::Discord).map_or_else(Vec::new, |forum| forum.servers);
        if servers.is_empty() {
            return Err(conflict(self.no_forum_pick()));
        }
        let Some(at) = servers.iter().position(|server| server.id == pick.id) else {
            return Err(Refusal::new(Code::NotFound, not_listed(&pick.id, &servers)));
        };
        self.pick_forum_server(Via::Discord, at);
        Ok(Some(json(&servers[at])))
    }

    fn no_forum_pick(&self) -> String {
        let messenger = self.messenger(Via::Discord);
        if messenger.token.is_none() {
            return NO_BOT.into();
        }
        if !messenger.paired() {
            return NOT_PAIRED.into();
        }
        if !self.config.discord_forum {
            return MODE_OFF.into();
        }
        let note = messenger.forum_note();
        format!("no server is waiting to be picked: {}", if note.is_empty() { NOT_READ } else { &note })
    }

    pub(in crate::app) fn post_info(&self, workspace: u64) -> Option<PostInfo> {
        let messenger = self.messenger(Via::Discord);
        let guild = messenger.saved.forum.as_ref()?.guild;
        let posts = &messenger.forum.posts;
        let thread = posts.thread(workspace)?;
        Some(PostInfo { url: format!("{CHANNELS}/{guild}/{thread}"), archived: posts.archived(workspace) })
    }
}
