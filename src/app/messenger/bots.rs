use std::collections::VecDeque;

use serde_json::Value;

use super::notices::label;
use super::{Asked, Delivery, Route};
use crate::app::App;
use crate::app::bots::Live;
use crate::bots::chat::{Kind, Line};
use crate::bots::{Bot, ChatApps};
use crate::control::Command;
use crate::control::bots::{BotRequest, BotSay};
use crate::messenger::{Keyboard, Message, Tap, Via};
use crate::text::truncate_right;

const LABEL: usize = 120;
const MOST_LISTED: usize = 25;
pub(super) const BOT_GONE: &str = "That bot is gone. /bots lists the bots now.";
const NO_BOTS: &str = "No bots yet: make one in cornercase, in the Bots section.";
const NO_SUCH_BOT: &str = "No bot has that name.";
pub(super) const BOTS: &str = "Bots";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Written {
    pub(super) message: i64,
    pub(super) chat: i64,
}

impl Written {
    pub(super) fn of(message: &Message) -> Self {
        Self { message: message.id, chat: message.chat }
    }
}

#[derive(Debug, Default)]
pub(super) struct Asks(VecDeque<(u64, u64, Written)>);

impl Asks {
    pub(super) fn push(&mut self, key: u64, line: u64, written: Written) {
        self.0.push_back((key, line, written));
    }

    pub(super) fn take(&mut self, key: u64, lines: &[u64]) -> Option<Written> {
        let mut newest = None;
        self.0.retain(|&(asked, line, written)| {
            let answered = asked == key && lines.contains(&line);
            if answered {
                newest = Some(written);
            }
            !answered
        });
        newest
    }

    pub(super) fn forget(&mut self, key: u64) {
        self.0.retain(|(asked, ..)| *asked != key);
    }
}

pub(super) fn named<'a, 'b>(args: &'a str, bots: &'b [&'b Bot]) -> Option<(&'b Bot, &'a str)> {
    let args = args.trim();
    if let Some((name, text)) = args.split_once('\n') {
        return crate::bots::find(bots, name).map(|bot| (bot, text.trim()));
    }
    let spaces = args.char_indices().rev().filter(|(_, c)| c.is_whitespace()).map(|(at, _)| at);
    std::iter::once(args.len())
        .chain(spaces)
        .find_map(|end| crate::bots::find(bots, &args[..end]).map(|bot| (bot, args[end..].trim())))
}

fn wanted(chat_apps: ChatApps, away: bool) -> bool {
    match chat_apps {
        ChatApps::Always => true,
        ChatApps::Away => away,
        ChatApps::Off => false,
    }
}

fn state(live: &Live) -> String {
    let state = if live.answering() { "answering" } else { "idle" };
    let unread = if live.unread > 0 { format!(" · {} unread", live.unread) } else { String::new() };
    format!("🤖 {} · {state}{unread}", live.bot.name)
}

impl App {
    pub(in crate::app) fn chat_apps_ready(&self) -> bool {
        Via::ALL.into_iter().any(|via| self.messenger(via).paired())
    }

    fn bot_delivers(&self, at: usize, via: Via) -> bool {
        let messenger = self.messenger(via);
        messenger.paired() && wanted(self.panels.bots.live[at].bot.chat_apps, messenger.away)
    }

    pub(in crate::app) fn bot_line_to_chat_apps(&mut self, at: usize, line: &Line, asked: &[u64]) {
        let key = self.panels.bots.live[at].key;
        for via in Via::ALL {
            let written = self.messenger_mut(via).bot_asks.take(key, asked);
            let written = written.filter(|written| self.still_heard(via, written.chat));
            let unasked = matches!(line.kind, Kind::Bot | Kind::Error) && self.bot_delivers(at, via);
            if written.is_some() || unasked {
                self.send_bot_line(via, at, line, written);
            }
        }
    }

    fn still_heard(&self, via: Via, chat: i64) -> bool {
        self.messenger(via).chat().is_some_and(|paired| paired.id == chat) || self.knows(via, chat)
    }

    pub(in crate::app) fn forget_bot_asks(&mut self, key: u64) {
        for via in Via::ALL {
            self.messenger_mut(via).bot_asks.forget(key);
        }
    }

    pub(in crate::app) fn bot_waiting_to_chat_apps(&mut self, at: usize, pane: u64) {
        for via in Via::ALL {
            if self.bot_delivers(at, via) && !self.messenger(via).away {
                self.notice_waiting(via, pane, None);
            }
        }
    }

    fn send_bot_line(&mut self, via: Via, at: usize, line: &Line, written: Option<Written>) {
        let key = self.panels.bots.live[at].key;
        let title = self.bot_title(via, at, line);
        let m = via.markup();
        let parts = match line.kind {
            Kind::Bot => m.markdown(&line.text),
            Kind::Error => {
                let lines: Vec<String> = line
                    .text
                    .lines()
                    .enumerate()
                    .map(|(i, text)| m.escape(&if i == 0 { format!("✗ {text}") } else { text.to_string() }))
                    .collect();
                m.lines(&lines)
            }
            _ => vec![m.escape(&line.text)],
        };
        let (reply_to, to) = (written.map(|w| w.message), written.map(|w| w.chat));
        let delivery = Delivery { route: Some(Route::Bot(key)), reply_to, to, ..Delivery::default() };
        self.say_long(via, &title, parts, delivery);
    }

    fn bot_title(&self, via: Via, at: usize, line: &Line) -> String {
        let m = via.markup();
        let name = &self.panels.bots.live[at].bot.name;
        let title = m.bold(&m.escape(&format!("🤖 {name}")));
        match line.routine.or(line.run) {
            Some(run) => {
                let (routine, _) = self.run_label(run, line.at);
                format!("{title} · {}", m.escape(&format!("↻ {}", truncate_right(&routine, LABEL))))
            }
            None => title,
        }
    }

    pub(super) fn write_to_bot(&mut self, via: Via, key: u64, text: String, written: Written) {
        let Some(at) = self.panels.bots.by_key(key) else {
            self.reply(via, written.message, BOT_GONE);
            return;
        };
        let bot = self.panels.bots.live[at].bot.id.clone();
        let say = BotSay { bot, text, wait: false };
        self.chat_request(via, Command::Bot(BotRequest::Say(say)), Asked::Bot { written, key }, None);
    }

    pub(super) fn bot_said(&mut self, via: Via, (written, key): (Written, u64), reply: Result<Value, String>) {
        match reply {
            Ok(value) => {
                self.post_reaction(via, written.message);
                if let Some(line) = value["line"]["id"].as_u64() {
                    self.messenger_mut(via).bot_asks.push(key, line, written);
                }
            }
            Err(why) => self.reply(via, written.message, &why),
        }
    }

    pub(super) fn bots_command(&mut self, via: Via, lead: Option<&str>) {
        let (lines, keyboard) = self.bots_message(via);
        let m = via.markup();
        let header = match lead {
            Some(lead) => format!("{}\n\n{}", m.escape(lead), m.bold(BOTS)),
            None => m.bold(BOTS),
        };
        self.say_long(via, &header, lines, Delivery { keyboard, ..Delivery::default() });
    }

    fn bots_message(&self, via: Via) -> (Vec<String>, Keyboard) {
        let messenger = self.messenger(via);
        let m = via.markup();
        let (now, zone) = (crate::time::now(), self.window_zone());
        let live = &self.panels.bots.live;
        let mut lines = Vec::new();
        let mut keyboard: Keyboard = Vec::new();
        for bot in live.iter().take(MOST_LISTED) {
            let next = self.bot_next_run(&bot.bot.id);
            let next = next.map(|at| format!(" · next {}", crate::queue::clock::short(at, now, &zone)));
            lines.push(m.escape(&format!("{}{}", state(bot), next.unwrap_or_default())));
            keyboard.push(vec![messenger.tap(label(&format!("✎ {}", bot.bot.name)), &Tap::Bot(bot.key))]);
        }
        if live.is_empty() {
            lines.push(m.escape(NO_BOTS));
        } else if live.len() > MOST_LISTED {
            lines.push(format!("… and {} more", live.len() - MOST_LISTED));
        }
        (m.lines(&lines), keyboard)
    }

    pub(super) fn bot_command(&mut self, via: Via, message: &Message) {
        let args = super::chat::command(&message.text).map_or("", |(_, args)| args);
        if args.is_empty() {
            self.bots_command(via, None);
            return;
        }
        let found = {
            let bots: Vec<&Bot> = self.panels.bots.live.iter().map(|live| &live.bot).collect();
            named(args, &bots).map(|(bot, text)| (bot.id.clone(), text.to_string()))
        };
        let Some((key, text)) =
            found.and_then(|(id, text)| self.panels.bots.by_id(&id).map(|at| (self.panels.bots.live[at].key, text)))
        else {
            self.bots_command(via, Some(NO_SUCH_BOT));
            return;
        };
        if text.is_empty() {
            self.tap_bot(via, key, message.chat);
        } else {
            self.write_to_bot(via, key, text, Written::of(message));
        }
    }

    pub(super) fn tap_bot(&mut self, via: Via, key: u64, chat: i64) {
        let m = via.markup();
        let Some(at) = self.panels.bots.by_key(key) else {
            self.say(via, m.escape(BOT_GONE), Delivery { to: Some(chat), ..Delivery::default() });
            return;
        };
        let name = self.panels.bots.live[at].bot.name.clone();
        let text = format!("Write to {}:", m.bold(&m.escape(&format!("🤖 {name}"))));
        self.ask(via, text, &format!("message for {name}"), (Route::Bot(key), Some(chat)));
    }
}

#[cfg(test)]
mod tests;
