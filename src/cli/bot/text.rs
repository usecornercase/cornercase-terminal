use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::bots::chat::{Kind, Line};
use crate::control::bots::{AgentInfo, BotInfo};
use crate::queue::clock;
use crate::text;

const WIDTH: usize = 120;
const LAST: usize = 60;

fn first(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
    text::truncate_right(line, WIDTH)
}

fn line_text(line: &Line) -> String {
    match line.kind {
        Kind::You => format!("you: {}", first(&line.text)),
        Kind::Step => format!("▸ {}", first(&line.text)),
        Kind::Bot => format!("answered: {}", first(&line.text)),
        Kind::Error => format!("error: {}", first(&line.text)),
        Kind::Notice => format!("notice: {}", first(&line.text)),
    }
}

pub(in crate::cli) fn event_text(bot: &str, news: (Option<&Line>, Option<&str>, Option<&str>)) -> String {
    let (line, state, change) = news;
    let what = line.map(line_text).or_else(|| state.or(change).map(str::to_string)).unwrap_or_default();
    format!("bot {bot}  {what}")
}

pub(super) fn when(at: i64, now: i64, zone: &str) -> String {
    let Some((timestamp, zone)) = Timestamp::from_second(at).ok().zip(clock::zone(zone).ok().or(Some(TimeZone::UTC)))
    else {
        return at.to_string();
    };
    let shown = timestamp.to_zoned(zone.clone());
    let today = Timestamp::from_second(now).ok().map(|now| now.to_zoned(zone));
    if today.is_some_and(|today| today.date() == shown.date()) {
        shown.strftime("%H:%M").to_string()
    } else {
        shown.strftime("%a %-d %b %H:%M").to_string()
    }
}

pub(super) fn agents(found: &[AgentInfo]) -> String {
    let mut out = String::new();
    let title = found.iter().map(|agent| agent.title.chars().count()).max().unwrap_or_default() + 2;
    for agent in found {
        let accesses = agent.accesses.join(", ");
        let note = match (&agent.why, agent.bot) {
            (Some(why), true) => format!("{accesses}  ({why})"),
            (Some(why), false) => why.clone(),
            (None, _) => accesses,
        };
        let _ = writeln!(out, "{:<10}{:<title$}{note}", agent.agent, agent.title);
    }
    out
}

pub(super) fn list(bots: &[BotInfo], now: i64, zone: &str) -> String {
    if bots.is_empty() {
        return String::from("no bots; cornercase bot new NAME makes one\n");
    }
    let rows: Vec<[String; 6]> = bots
        .iter()
        .map(|bot| {
            let state = if bot.held > 0 { format!("{} (+{} held)", bot.state, bot.held) } else { bot.state.clone() };
            let last = bot.last.as_ref().map_or_else(String::new, |last| {
                format!("{} · {}", when(last.at, now, zone), text::truncate_right(&text::one_line(&last.text), LAST))
            });
            [
                bot.id.clone(),
                text::truncate_right(&bot.name, 18),
                bot.agent.clone(),
                state,
                format!("{} unread", bot.unread),
                last,
            ]
        })
        .collect();
    let width = |column: usize| rows.iter().map(|row| row[column].chars().count()).max().unwrap_or_default() + 2;
    let widths: Vec<usize> = (0..5).map(width).collect();
    let mut out = String::new();
    for row in &rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate().take(5) {
            let _ = write!(line, "{cell:<width$}", width = widths[column]);
        }
        line.push_str(&row[5]);
        let _ = writeln!(out, "{}", line.trim_end());
    }
    out
}

struct Chat<'a> {
    name: &'a str,
    now: i64,
    zone: &'a str,
    out: String,
    open: bool,
}

impl Chat<'_> {
    fn header(&mut self, who: &str, at: i64, extra: &str) {
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        let _ = writeln!(self.out, "{who} · {}{extra}", when(at, self.now, self.zone));
    }

    fn add(&mut self, line: &Line) {
        match line.kind {
            Kind::You => {
                self.open = false;
                self.header("you", line.at, if line.held { " · held" } else { "" });
                let _ = writeln!(self.out, "{}", line.text);
            }
            Kind::Step => {
                self.begin(line.at);
                let _ = writeln!(self.out, "  ▸ {}", first(&line.text));
            }
            Kind::Bot => {
                self.begin(line.at);
                let _ = writeln!(self.out, "{}", line.text);
                self.open = false;
            }
            Kind::Error => {
                self.open = false;
                self.header(self.name, line.at, " · error");
                let _ = writeln!(self.out, "{}", line.text);
            }
            Kind::Notice => {
                self.open = false;
                if !self.out.is_empty() {
                    self.out.push('\n');
                }
                let _ = writeln!(self.out, "— {} · {} —", line.text, when(line.at, self.now, self.zone));
            }
        }
    }

    fn begin(&mut self, at: i64) {
        if !self.open {
            self.header(self.name, at, "");
            self.open = true;
        }
    }
}

pub(super) fn chat(name: &str, lines: &[Line], now: i64, zone: &str) -> String {
    if lines.is_empty() {
        return String::from("nothing said yet\n");
    }
    let mut chat = Chat { name, now, zone, out: String::new(), open: false };
    for line in lines {
        chat.add(line);
    }
    chat.out
}
