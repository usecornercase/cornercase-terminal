use ratatui::layout::Rect;

use super::{Asked, By, Route};
use crate::agents::activity::Status;
use crate::app::App;
use crate::control::{Command, Keys, SendText};
use crate::log;
use crate::project::Hierarchy;
use crate::telegram::{Message, Order, Press, Tap, Unread, Update, html};

pub(super) const GONE: &str = "That agent is gone. /status lists the agents now.";
const HELP: &str = "/status every agent and what it is doing\n/new start an agent\n/away message me here when an \
    agent needs you or finishes\n/back stop\n\nReply to one of my messages to write to that agent.";
const STALE: &str = "this button is from before cornercase restarted: /status lists the agents now";
const ONLY_TEXT: &str = "I only read text messages.";
const NO_AGENT: &str = "No agent is running. /new starts one.";
const WHICH: &str = "Which agent is this for?";
const HELD: usize = 16;

pub(super) fn welcome() -> String {
    format!("<b>cornercase is connected</b>: this chat now gets this computer's agents.\n\n{HELP}")
}

fn command(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim().strip_prefix('/')?;
    let (word, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let word = word.split_once('@').map_or(word, |(word, _)| word);
    Some((word, args.trim()))
}

impl App {
    pub(super) fn telegram_update(&mut self, update: Update, area: Rect) {
        match update {
            Update::Message(message) => self.telegram_message(&message, area),
            Update::Press(press) => self.telegram_press(&press, area),
        }
    }

    fn paired_with(&self, chat: i64, user: i64) -> bool {
        self.telegram.chat().is_some_and(|paired| paired.id == chat && paired.user == user)
    }

    fn telegram_message(&mut self, message: &Message, area: Rect) {
        if !self.paired_with(message.chat, message.user) {
            if !self.try_pairing(message) {
                log::info!("telegram", "message from another chat ignored", chat = message.chat);
            }
            return;
        }
        let kind = command(&message.text).map_or(if message.reply_to.is_some() { "reply" } else { "text" }, |(c, _)| c);
        log::info!("telegram", "message", kind = kind, bytes = message.text.len());
        if message.text.trim().is_empty() {
            self.reply(message.id, ONLY_TEXT);
            return;
        }
        if let Some((word, _)) = command(&message.text) {
            self.telegram_command(word, message, area);
            return;
        }
        match message.reply_to.and_then(|id| self.telegram.route(id)) {
            Some(Route::Pane(pane)) => self.deliver(pane, message.text.clone(), message.id),
            Some(Route::Branch(draft)) => self.draft_branch(draft, message.text.trim(), message.id),
            Some(Route::Prompt(draft)) => self.draft_prompt(draft, &message.text, message.id, area),
            None => self.unrouted(message),
        }
    }

    fn telegram_command(&mut self, word: &str, message: &Message, area: Rect) {
        match word {
            "status" => {
                let (status, keyboard) = self.status_message();
                self.say_long(super::notices::AGENTS, status, keyboard, None);
            }
            "new" => self.new_draft(None, area),
            "away" => {
                let _ = self.go_away(By::Telegram);
            }
            "back" => {
                if self.telegram.away {
                    self.come_back(By::Telegram);
                } else {
                    self.reply(message.id, "I wasn't messaging you here. /away starts.");
                }
            }
            "start" => self.say(welcome(), Vec::new(), None, None),
            _ => self.say(HELP.into(), Vec::new(), None, Some(message.id)),
        }
    }

    fn unrouted(&mut self, message: &Message) {
        let panes = self.agent_panes();
        match panes.as_slice() {
            [] => self.reply(message.id, NO_AGENT),
            [pane] => self.deliver(*pane, message.text.clone(), message.id),
            _ => {
                self.telegram.next_held += 1;
                let id = self.telegram.next_held;
                self.telegram.held.push_back((id, message.text.clone(), message.id));
                while self.telegram.held.len() > HELD {
                    self.telegram.held.pop_front();
                }
                let keyboard = panes
                    .into_iter()
                    .filter_map(|pane| {
                        let agent = self.agent_at(pane)?;
                        let text = format!("{} {} · {}", super::notices::icon(agent.status), agent.kind, agent.place);
                        Some(vec![self.telegram.tap(text, &Tap::Deliver(pane, id))])
                    })
                    .collect();
                self.say(WHICH.into(), keyboard, None, Some(message.id));
            }
        }
    }

    pub(super) fn deliver(&mut self, pane: u64, text: String, message: i64) {
        let Some(agent) = self.agent_at(pane) else {
            self.reply(message, GONE);
            return;
        };
        let term = self.projects.pane_by(pane);
        let (survey, dialog) = term.map_or((false, false), |term| (term.survey(), term.dialog()));
        if agent.status == Some(Status::Waiting) {
            let why = format!("{} is waiting for an answer: use the buttons below.", agent.kind);
            self.reply(message, &why);
            self.notice_waiting(pane);
        } else if survey {
            let keyboard = vec![vec![self.telegram.tap("dismiss the survey", &Tap::Key(pane, "0".into()))]];
            let html = "Claude Code's feedback survey is open above its input box: dismiss it, then send again.";
            self.say(html.into(), keyboard, Some(Route::Pane(pane)), Some(message));
        } else if dialog {
            let keyboard = vec![vec![
                self.telegram.tap("screen", &Tap::Screen(pane)),
                self.telegram.tap("esc", &Tap::Key(pane, "esc".into())),
            ]];
            let html = format!(
                "{} shows a panel or a dialog instead of its input box: look at its screen, close it with esc, \
                 then send again.",
                html::escape(&agent.kind)
            );
            self.say(html, keyboard, Some(Route::Pane(pane)), Some(message));
        } else {
            let send = SendText { pane: Some(pane), text: Some(text), enter: true, ..SendText::default() };
            self.telegram_request(Command::Send(send), Asked::Send { message }, None);
        }
    }

    fn telegram_press(&mut self, press: &Press, area: Rect) {
        if !self.paired_with(press.chat, press.user) {
            log::info!("telegram", "button from another chat ignored", chat = press.chat);
            return;
        }
        match Tap::read(&press.data, &self.telegram.run) {
            Err(Unread::Stale) => self.answer_press(&press.id, STALE),
            Err(Unread::Unknown) => self.answer_press(&press.id, ""),
            Ok(tap) => {
                log::info!("telegram", "button", tap = tap_name(&tap));
                self.telegram_tap(tap, press, area);
            }
        }
    }

    fn telegram_tap(&mut self, tap: Tap, press: &Press, area: Rect) {
        match tap {
            Tap::Key(pane, key) => {
                let Some(agent) = self.agent_at(pane) else {
                    self.answer_press(&press.id, GONE);
                    return;
                };
                let asked = Asked::Keys {
                    press: press.id.clone(),
                    chat_message: press.message,
                    key: key.clone(),
                    agent: agent.kind,
                };
                let keys = Keys { pane: Some(pane), tab: None, keys: vec![key] };
                self.telegram_request(Command::Keys(keys), asked, None);
            }
            Tap::Screen(pane) => {
                self.answer_press(&press.id, "");
                self.send_screen(pane);
            }
            Tap::Write(pane) => {
                self.answer_press(&press.id, "");
                let Some(agent) = self.agent_at(pane) else {
                    self.say(GONE.into(), Vec::new(), None, None);
                    return;
                };
                let html = format!("Write to <b>{}</b> in {}:", html::escape(&agent.kind), html::escape(&agent.place));
                self.ask(html, &format!("message for {}", agent.kind), Route::Pane(pane));
            }
            Tap::Deliver(pane, held) => {
                self.answer_press(&press.id, "");
                let found = self.telegram.held.iter().position(|(id, ..)| *id == held);
                let Some((_, text, message)) = found.and_then(|i| self.telegram.held.remove(i)) else { return };
                if let Some(agent) = self.agent_at(pane)
                    && let Some(chat) = self.telegram.chat().map(|c| c.id)
                {
                    let html = format!("For <b>{}</b> in {}.", html::escape(&agent.kind), html::escape(&agent.place));
                    self.post(Order::Edit { chat, message: press.message, html, keyboard: Vec::new() });
                }
                self.deliver(pane, text, message);
            }
            Tap::New => {
                self.answer_press(&press.id, "");
                self.new_draft(None, area);
            }
            Tap::Draft(draft, choice) => {
                self.answer_press(&press.id, "");
                self.draft_choice(draft, choice, press.message, area);
            }
        }
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
