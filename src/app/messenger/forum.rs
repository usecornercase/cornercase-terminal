use super::Messenger;
use super::every::Every;
use crate::discord::forum::{Checklist, NAME};
use crate::discord::gateway::DIRECT_ONLY;
use crate::text::CRUMB_SEPARATOR;

mod cleanup;
mod posts;
mod replies;
mod setup;
mod socket;
mod tags;

pub(super) use posts::Pointer;

const GUILD_INSTALL: &str = "turn on Guild Install: Developer Portal › Installation";
const CONTENT: &str = "turn on Message Content Intent: Developer Portal › Bot";
const CONTENT_ON: &str = "Message Content Intent is on";
const ADD_THE_APP: &str = "add the app to your server";
const PICK_A_SERVER: &str = "pick the server for the forum";
const MAKE_THE_FORUM: &str = "a private forum channel, #cornercase, is made for you";

#[derive(Debug, Default)]
pub(super) struct Forum {
    setup: Setup,
    posts: posts::Posts,
    heard: replies::Heard,
    labels: tags::Labels,
    refused: bool,
}

#[derive(Debug, Default)]
struct Setup {
    every: Every,
    checklist: Option<Checklist>,
    chosen: Option<i64>,
    relink: bool,
}

impl Messenger {
    pub(super) fn forum_ready(&self) -> bool {
        self.token.is_some() && self.saved.forum.is_some()
    }

    pub(super) fn forget_forum(&mut self) {
        self.saved.forum = None;
        self.forum.posts.forget();
        self.forum.labels = tags::Labels::default();
        self.forum.setup = Setup::default();
    }

    pub(super) fn content_refused(&self) -> bool {
        self.forum.refused && self.forum_ready()
    }

    pub(super) fn refuse_content(&mut self, refused: bool) {
        self.forum.refused = refused;
    }

    pub(super) fn forum_note(&self) -> String {
        if self.content_refused() {
            return DIRECT_ONLY.into();
        }
        if let Some(forum) = self.saved.forum.as_ref().filter(|_| self.forum_ready()) {
            let place = format!("{}{CRUMB_SEPARATOR}#{NAME}", forum.guild_name);
            return if self.forum.posts.failed { format!("{place} · {}", posts::FAILED) } else { place };
        }
        let setup = &self.forum.setup;
        if let Some(failed) = &setup.every.failed {
            return failed.clone();
        }
        let Some(checklist) = &setup.checklist else { return String::new() };
        self.forum_steps(checklist).into_iter().find(|(done, _)| !done).map(|(_, step)| step).unwrap_or_default()
    }

    fn chosen_server(&self) -> Option<i64> {
        self.forum.setup.chosen.or(self.saved.forum.as_ref().map(|forum| forum.guild))
    }

    fn forum_steps(&self, checklist: &Checklist) -> [(bool, String); 4] {
        let picked = checklist.pick(self.chosen_server());
        let server = match picked {
            Some(server) => (true, format!("the app is in {}", server.name)),
            None if checklist.servers.len() > 1 => (false, PICK_A_SERVER.into()),
            None => (false, ADD_THE_APP.into()),
        };
        let made = self.saved.forum.as_ref().is_some_and(|forum| picked.is_some_and(|s| s.id == forum.guild));
        [
            (
                checklist.guild_install,
                if checklist.guild_install { "Guild Install is on" } else { GUILD_INSTALL }.into(),
            ),
            content_step(checklist.content && !self.forum.refused),
            server,
            (made, MAKE_THE_FORUM.into()),
        ]
    }
}

fn content_step(on: bool) -> (bool, String) {
    (on, if on { CONTENT_ON } else { CONTENT }.into())
}
