use std::time::Instant;

use ratatui::layout::Rect;

use super::{Asked, By, Delivery, Route};
use crate::agents::activity::Status;
use crate::app::App;
use crate::control::{Command, Keys, SendText};
use crate::log;
use crate::messenger::{Message, Order, Press, Tap, Unread, Update, Via};
use crate::project::Hierarchy;

pub(super) const GONE: &str = "That agent is gone. /status lists the agents now.";
const HELP: &str = "/status every agent and what it is doing\n/new start an agent\n/away message me here when an \
    agent needs you or finishes\n/back stop\n\nReply to one of my messages to write to that agent.";
const STALE: &str = "this button is from before cornercase restarted: /status lists the agents now";
const ONLY_TEXT: &str = "I only read text messages.";
const NO_AGENT: &str = "No agent is running. /new starts one.";
const WHICH: &str = "Which agent is this for?";
const HELD: usize = 16;

pub(super) fn welcome(via: Via) -> String {
    let connected = via.markup().bold("cornercase is connected");
    format!("{connected}: this chat now gets this computer's agents.\n\n{HELP}")
}

fn command(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim().strip_prefix('/')?;
    let (word, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let word = word.split_once('@').map_or(word, |(word, _)| word);
    Some((word, args.trim()))
}

impl App {
    pub(super) fn chat_update(&mut self, via: Via, update: Update, area: Rect) {
        match update {
            Update::Message(message) => self.chat_message(via, &message, area),
            Update::Press(press) => self.chat_press(via, &press, area),
        }
    }

    fn paired_with(&self, via: Via, chat: i64, user: i64) -> bool {
        self.messenger(via).chat().is_some_and(|paired| paired.id == chat && paired.user == user)
    }

    fn chat_message(&mut self, via: Via, message: &Message, area: Rect) {
        if !self.paired_with(via, message.chat, message.user) {
            if !self.try_pairing(via, message) {
                log::info!(via.target(), "message from another chat ignored", chat = message.chat);
            }
            return;
        }
        let kind = command(&message.text).map_or(if message.reply_to.is_some() { "reply" } else { "text" }, |(c, _)| c);
        log::info!(via.target(), "message", kind = kind, bytes = message.text.len());
        if message.text.trim().is_empty() {
            self.reply(via, message.id, ONLY_TEXT);
            return;
        }
        if let Some((word, _)) = command(&message.text) {
            self.messenger_mut(via).open_ask = None;
            self.chat_command(via, word, message, area);
            return;
        }
        let replied = message.reply_to.and_then(|id| self.messenger(via).route(id));
        let messenger = self.messenger_mut(via);
        let route = match replied {
            Some(route) => {
                if messenger.open_ask.is_some_and(|(open, _)| open == route) {
                    messenger.open_ask = None;
                }
                Some(route)
            }
            None => messenger.take_open_ask(Instant::now()),
        };
        match route {
            Some(Route::Pane(pane)) => self.deliver(via, pane, message.text.clone(), message.id),
            Some(Route::Branch(draft)) => self.draft_branch(via, draft, message.text.trim(), message.id),
            Some(Route::Prompt(draft)) => self.draft_prompt(via, draft, message, area),
            None => self.unrouted(via, message),
        }
    }

    fn chat_command(&mut self, via: Via, word: &str, message: &Message, area: Rect) {
        match word {
            "status" => {
                let (status, keyboard) = self.status_message(via);
                let header = via.markup().bold(super::notices::AGENTS);
                self.say_long(via, &header, status, Delivery { keyboard, ..Delivery::default() });
            }
            "new" => self.new_draft(via, None, area),
            "away" => {
                let _ = self.go_away(via, By::Chat);
            }
            "back" => {
                if self.messenger(via).away {
                    self.come_back(via, By::Chat);
                } else {
                    self.reply(via, message.id, "I wasn't messaging you here. /away starts.");
                }
            }
            "start" => self.say(via, welcome(via), Delivery::default()),
            _ => self.say(via, HELP.into(), Delivery { reply_to: Some(message.id), ..Delivery::default() }),
        }
    }

    fn unrouted(&mut self, via: Via, message: &Message) {
        let panes = self.agent_panes();
        match panes.as_slice() {
            [] => self.reply(via, message.id, NO_AGENT),
            [pane] => self.deliver(via, *pane, message.text.clone(), message.id),
            _ => {
                let messenger = self.messenger_mut(via);
                messenger.next_held += 1;
                let id = messenger.next_held;
                messenger.held.push_back((id, message.text.clone(), message.id));
                while messenger.held.len() > HELD {
                    messenger.held.pop_front();
                }
                let keyboard = panes
                    .into_iter()
                    .filter_map(|pane| {
                        let agent = self.agent_at(pane)?;
                        let text = format!("{} {} · {}", super::notices::icon(agent.status), agent.kind, agent.place);
                        Some(vec![self.messenger(via).tap(text, &Tap::Deliver(pane, id))])
                    })
                    .collect();
                self.say(via, WHICH.into(), Delivery { keyboard, reply_to: Some(message.id), ..Delivery::default() });
            }
        }
    }

    pub(super) fn deliver(&mut self, via: Via, pane: u64, text: String, message: i64) {
        let Some(agent) = self.agent_at(pane) else {
            self.reply(via, message, GONE);
            return;
        };
        let term = self.projects.pane_by(pane);
        let (survey, dialog) = term.map_or((false, false), |term| (term.survey(), term.dialog()));
        let messenger = self.messenger(via);
        if agent.status == Some(Status::Waiting) {
            let why = format!("{} is waiting for an answer: use the buttons below.", agent.kind);
            self.reply(via, message, &why);
            self.notice_waiting(via, pane);
        } else if survey {
            let keyboard = vec![vec![messenger.tap("dismiss the survey", &Tap::Key(pane, "0".into()))]];
            let text = "Claude Code's feedback survey is open above its input box: dismiss it, then send again.";
            self.say(via, text.into(), Delivery { keyboard, route: Some(Route::Pane(pane)), reply_to: Some(message) });
        } else if dialog {
            let keyboard = vec![vec![
                messenger.tap("screen", &Tap::Screen(pane)),
                messenger.tap("esc", &Tap::Key(pane, "esc".into())),
            ]];
            let text = format!(
                "{} shows a panel or a dialog instead of its input box: look at its screen, close it with esc, \
                 then send again.",
                via.markup().escape(&agent.kind)
            );
            self.say(via, text, Delivery { keyboard, route: Some(Route::Pane(pane)), reply_to: Some(message) });
        } else {
            let send = SendText { pane: Some(pane), text: Some(text), enter: true, ..SendText::default() };
            self.chat_request(via, Command::Send(send), Asked::Send { message }, None);
        }
    }

    fn chat_press(&mut self, via: Via, press: &Press, area: Rect) {
        if !self.paired_with(via, press.chat, press.user) {
            log::info!(via.target(), "button from another chat ignored", chat = press.chat);
            return;
        }
        match Tap::read(&press.data, &self.messenger(via).run) {
            Err(Unread::Stale) => self.answer_press(via, &press.id, STALE),
            Err(Unread::Unknown) => self.answer_press(via, &press.id, ""),
            Ok(tap) => {
                log::info!(via.target(), "button", tap = tap_name(&tap));
                self.chat_tap(via, tap, press, area);
            }
        }
    }

    fn chat_tap(&mut self, via: Via, tap: Tap, press: &Press, area: Rect) {
        match tap {
            Tap::Key(pane, key) => self.tap_key(via, pane, key, press),
            Tap::Screen(pane) => {
                self.answer_press(via, &press.id, "");
                self.send_screen(via, pane);
            }
            Tap::Write(pane) => {
                self.answer_press(via, &press.id, "");
                self.tap_write(via, pane);
            }
            Tap::Deliver(pane, held) => {
                self.answer_press(via, &press.id, "");
                self.tap_deliver(via, pane, held, press.message);
            }
            Tap::New => {
                self.answer_press(via, &press.id, "");
                self.new_draft(via, None, area);
            }
            Tap::Draft(draft, choice) => {
                self.answer_press(via, &press.id, "");
                self.draft_choice(via, draft, choice, press.message);
            }
        }
    }

    fn tap_key(&mut self, via: Via, pane: u64, key: String, press: &Press) {
        let Some(agent) = self.agent_at(pane) else {
            self.answer_press(via, &press.id, GONE);
            return;
        };
        let asked =
            Asked::Keys { press: press.id.clone(), chat_message: press.message, key: key.clone(), agent: agent.kind };
        let keys = Keys { pane: Some(pane), tab: None, keys: vec![key] };
        self.chat_request(via, Command::Keys(keys), asked, None);
    }

    fn tap_write(&mut self, via: Via, pane: u64) {
        let Some(agent) = self.agent_at(pane) else {
            self.say(via, GONE.into(), Delivery::default());
            return;
        };
        let m = via.markup();
        let text = format!("Write to {} in {}:", m.bold(&m.escape(&agent.kind)), m.escape(&agent.place));
        self.ask(via, text, &format!("message for {}", agent.kind), Route::Pane(pane));
    }

    fn tap_deliver(&mut self, via: Via, pane: u64, held: u32, edited_message: i64) {
        let messenger = self.messenger_mut(via);
        let found = messenger.held.iter().position(|(id, ..)| *id == held);
        let Some((_, text, message)) = found.and_then(|i| messenger.held.remove(i)) else { return };
        if let Some(agent) = self.agent_at(pane)
            && let Some(chat) = self.messenger(via).chat().map(|c| c.id)
        {
            let m = via.markup();
            let edited = format!("For {} in {}.", m.bold(&m.escape(&agent.kind)), m.escape(&agent.place));
            self.post(via, Order::Edit { chat, message: edited_message, text: edited, keyboard: Vec::new() });
        }
        self.deliver(via, pane, text, message);
    }
}

fn tap_name(tap: &Tap) -> &'static str {
    match tap {
        Tap::Key(..) => "keys",
        Tap::Screen(_) => "screen",
        Tap::Write(_) => "write",
        Tap::Deliver(..) => "deliver",
        Tap::New => "new",
        Tap::Draft(..) => "new agent",
    }
}
