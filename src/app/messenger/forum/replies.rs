use std::collections::VecDeque;
use std::time::Instant;

use ratatui::layout::Rect;

use super::super::Route;
use super::super::chat::command;
use crate::app::App;
use crate::messenger::{Message, Via};
use crate::project::Hierarchy;

const HEARD: usize = 256;
const COMMANDS: &str = "Commands work in the direct message with the bot.";
const NO_AGENT: &str = "No agent is running in this workspace. /new in the direct message starts one.";

#[derive(Debug, Default)]
pub(super) struct Heard(VecDeque<(i64, i64)>);

impl Heard {
    fn heard(&mut self, message: i64, thread: i64) {
        self.0.push_back((message, thread));
        while self.0.len() > HEARD {
            self.0.pop_front();
        }
    }

    fn thread_of(&self, message: i64) -> Option<i64> {
        self.0.iter().rev().find(|(heard, _)| *heard == message).map(|(_, thread)| *thread)
    }
}

impl App {
    pub(in crate::app::messenger) fn posted_by_owner(&self, via: Via, message: &Message) -> bool {
        let messenger = self.messenger(via);
        let owner = messenger.chat().is_some_and(|paired| paired.user == message.user);
        let server = messenger.saved.forum.as_ref().is_some_and(|forum| message.guild == Some(forum.guild));
        owner && server && self.knows(via, message.chat)
    }

    pub(in crate::app::messenger) fn heard(&mut self, via: Via, message: &Message) {
        self.messenger_mut(via).forum.heard.heard(message.id, message.chat);
    }

    pub(in crate::app::messenger) fn heard_in(&self, via: Via, message: i64) -> Option<i64> {
        self.messenger(via).forum.heard.thread_of(message)
    }

    pub(in crate::app::messenger) fn post_message(&mut self, via: Via, message: &Message, area: Rect) {
        let Some(workspace) = self.messenger(via).forum.posts.workspace_of(message.chat) else { return };
        if command(&message.text).is_some() {
            self.reply(via, message.id, COMMANDS);
            return;
        }
        let route = self.replied_route(via, message).or_else(|| self.open_ask_in(via, workspace));
        if let Some(route) = route {
            self.text_for(via, route, message, area);
            return;
        }
        let panes = self.agent_panes().into_iter().filter(|pane| self.workspace_of_pane(*pane) == Some(workspace));
        self.unrouted(via, message, panes.collect(), NO_AGENT);
    }

    fn open_ask_in(&mut self, via: Via, workspace: u64) -> Option<Route> {
        let (Route::Pane(pane), _) = self.messenger(via).open_ask? else { return None };
        if self.workspace_of_pane(pane) != Some(workspace) {
            return None;
        }
        self.messenger_mut(via).take_open_ask(Instant::now())
    }

    fn workspace_of_pane(&self, pane: u64) -> Option<u64> {
        let at = self.projects.tab_of_pane(pane)?;
        Some(self.projects.workspace(at.workspace()).id)
    }
}
