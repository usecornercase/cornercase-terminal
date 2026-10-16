use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{Asked, Route};
use crate::agents::activity::{MOON, SHELL, Status};
use crate::app::App;
use crate::control::{Command, LastMessage};
use crate::messenger::{self, Keyboard, Tap, Via};
use crate::project::Hierarchy;
use crate::text::{CRUMB_SEPARATOR, truncate_right};

const SETTLE: Duration = Duration::from_secs(1);
const EXCERPT: usize = 15;
const SCREEN: usize = 40;
const MOST_LISTED: usize = 25;
const LABEL: usize = 40;
const PLACE: usize = 120;
const MODEL: usize = 40;
pub(super) const AGENTS: &str = "Agents";
const WORKING: &str = "working";
const WAITING: &str = "waiting";

pub(super) struct Watch {
    state: &'static str,
    since: Instant,
    settled: &'static str,
    busy: bool,
    question: Option<String>,
    asked: Instant,
    looked: Instant,
}

enum Notice {
    Waiting(u64),
    Finished(u64, bool),
}

pub(super) struct Agent {
    pub(super) kind: String,
    pub(super) place: String,
    pub(super) status: Option<Status>,
    pub(super) screen: String,
}

pub(super) fn icon(status: Option<Status>) -> &'static str {
    match status {
        Some(Status::Waiting | Status::Failed) => "!",
        Some(Status::Done) => "✓",
        Some(Status::Pending) => "●",
        Some(Status::Working) => MOON,
        Some(Status::Idle) | None => "○",
    }
}

fn label(text: &str) -> String {
    let mut out: String = text.chars().take(LABEL).collect();
    if text.chars().count() > LABEL {
        out.pop();
        out.push('…');
    }
    out
}

fn with_screen(via: Via, header: &str, screen: &str, lines: usize) -> String {
    let m = via.markup();
    let screen = m.pre(&excerpt(screen, lines), m.room(header));
    format!("{header}\n{screen}")
}

fn question(screen: &str) -> Option<String> {
    if messenger::choices(screen).is_empty() {
        return None;
    }
    let rows = excerpt(screen, EXCERPT);
    let rows: Vec<&str> =
        rows.lines().map(|row| row.trim_matches(|c: char| c.is_whitespace() || messenger::MARKS.contains(c))).collect();
    Some(rows.join("\n"))
}

fn excerpt(screen: &str, lines: usize) -> String {
    let rows: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let end = rows.iter().rposition(|row| !row.is_empty()).map_or(0, |i| i + 1);
    rows[end.saturating_sub(lines)..end].join("\n")
}

impl App {
    pub(super) fn agent_at(&self, pane: u64) -> Option<Agent> {
        let (p, w, t) = self.projects.tab_of_pane(pane)?;
        let project = &self.projects[p];
        let workspace = &project.workspaces[w];
        let term = workspace.tabs[t].pane_by(pane)?;
        let kind = term.agent.agent()?.to_string();
        let mut place = self.project_label(project);
        if project.workspaces.len() > 1 {
            place = format!("{place}{CRUMB_SEPARATOR}{}", workspace.label());
        }
        let screen = term.emulator.screen_text().unwrap_or_default();
        let place = truncate_right(&place, PLACE);
        Some(Agent { kind, place, status: term.agent.status(), screen })
    }

    pub(super) fn agent_panes(&self) -> Vec<u64> {
        let places = self.agent_places().into_iter();
        places.map(|(p, w, t, i)| self.projects[p].workspaces[w].tabs[t].panes[i].id).collect()
    }

    pub(in crate::app) fn watch_messengers(&mut self, now: Instant) {
        for via in Via::ALL {
            self.watch_messenger(via, now);
        }
    }

    fn watch_messenger(&mut self, via: Via, now: Instant) {
        if !self.messenger(via).paired() {
            self.messenger_mut(via).watches.clear();
            return;
        }
        let mut seen = HashSet::new();
        let mut notices = Vec::new();
        let members = self.run_panes(false);
        for term in self.projects.terms() {
            let Some(state) = term.agent.state() else { continue };
            let state = if term.agent.reported_done() { "done" } else { state };
            seen.insert(term.id);
            let fresh =
                Watch { state, since: now, settled: state, busy: false, question: None, asked: now, looked: now };
            let watch = self.messengers[via.index()].watches.entry(term.id).or_insert(fresh);
            if watch.state != state {
                watch.state = state;
                watch.since = now;
            }
            let over = !matches!(state, WORKING | WAITING);
            watch.busy |= !over;
            if now.saturating_duration_since(watch.since) < SETTLE {
                continue;
            }
            if state == WAITING && watch.settled != WAITING {
                watch.question = question(&term.emulator.screen_text().unwrap_or_default());
                watch.asked = now;
                notices.push(Notice::Waiting(term.id));
            } else if state == WAITING
                && term.input_at.is_some_and(|input| input > watch.asked && term.output_at > input)
                && term.output_at > watch.looked
                && now.saturating_duration_since(term.output_at) >= SETTLE
            {
                watch.looked = now;
                let next = question(&term.emulator.screen_text().unwrap_or_default());
                if next.is_some() && next != watch.question {
                    watch.question = next;
                    watch.asked = now;
                    notices.push(Notice::Waiting(term.id));
                }
            } else if over
                && !(members.contains(&term.id) && state != "failed")
                && !term.agent.reported_idle()
                && (watch.busy || matches!(state, "done" | "failed") && watch.settled != state)
            {
                notices.push(Notice::Finished(term.id, state == SHELL));
            }
            watch.settled = state;
            watch.busy &= !over;
        }
        self.messenger_mut(via).watches.retain(|pane, _| seen.contains(pane));
        if !self.messenger(via).away {
            return;
        }
        for notice in notices {
            match notice {
                Notice::Waiting(pane) => self.notice_waiting(via, pane),
                Notice::Finished(pane, shell) => self.notice_finished(via, pane, shell),
            }
        }
    }

    pub(super) fn notice_waiting(&mut self, via: Via, pane: u64) {
        let Some(agent) = self.agent_at(pane) else { return };
        let messenger = self.messenger(via);
        let mut keyboard: Keyboard = messenger::choices(&agent.screen)
            .into_iter()
            .map(|(digit, text)| {
                vec![messenger.tap(label(&format!("{digit} · {text}")), &Tap::Key(pane, digit.into()))]
            })
            .collect();
        keyboard.push(vec![
            messenger.tap("esc", &Tap::Key(pane, "esc".into())),
            messenger.tap("screen", &Tap::Screen(pane)),
        ]);
        let m = via.markup();
        let header =
            format!("{} · {}", m.bold(&format!("! {} needs you", m.escape(&agent.kind))), m.escape(&agent.place));
        let text = with_screen(via, &header, &agent.screen, EXCERPT);
        self.say(via, text, keyboard, Some(Route::Pane(pane)), None);
    }

    fn notice_finished(&mut self, via: Via, pane: u64, shell: bool) {
        let Some(agent) = self.agent_at(pane) else { return };
        let (icon, what) = if agent.status == Some(Status::Failed) {
            ("!", "failed")
        } else if shell {
            ("✓", "ended its turn")
        } else {
            ("✓", "finished")
        };
        let m = via.markup();
        let mut header =
            format!("{} · {}", m.bold(&format!("{icon} {} {what}", m.escape(&agent.kind))), m.escape(&agent.place));
        if shell {
            header.push_str(" · a background shell still runs");
        }
        let command = Command::LastMessage(LastMessage { pane: Some(pane), tab: None });
        self.chat_request(via, command, Asked::Finished { pane, header }, None);
    }

    pub(super) fn finished_read(&mut self, via: Via, pane: u64, header: &str, reply: Result<Value, String>) {
        let keyboard = vec![vec![self.messenger(via).tap("screen", &Tap::Screen(pane))]];
        let m = via.markup();
        let text = reply.ok().and_then(|value| value["text"].as_str().map(String::from));
        let parts = match text {
            Some(text) if !text.trim().is_empty() => m.markdown(&text),
            _ => {
                let screen = self.agent_at(pane).map(|agent| agent.screen).unwrap_or_default();
                vec![m.pre(&excerpt(&screen, EXCERPT), m.room(header))]
            }
        };
        self.say_long(via, header, parts, keyboard, Some(Route::Pane(pane)));
    }

    pub(super) fn send_screen(&mut self, via: Via, pane: u64) {
        let Some(agent) = self.agent_at(pane) else {
            self.say(via, super::chat::GONE.into(), Vec::new(), None, None);
            return;
        };
        let messenger = self.messenger(via);
        let keyboard = vec![vec![
            messenger.tap("refresh", &Tap::Screen(pane)),
            messenger.tap("esc", &Tap::Key(pane, "esc".into())),
        ]];
        let m = via.markup();
        let header = format!("{} {} · {}", icon(agent.status), m.bold(&m.escape(&agent.kind)), m.escape(&agent.place));
        let text = with_screen(via, &header, &agent.screen, SCREEN);
        self.say(via, text, keyboard, Some(Route::Pane(pane)), None);
    }

    pub(super) fn status_message(&self, via: Via) -> (Vec<String>, Keyboard) {
        let messenger = self.messenger(via);
        let m = via.markup();
        let mut lines = Vec::new();
        let mut keyboard: Keyboard = Vec::new();
        let panes = self.agent_panes();
        for &pane in panes.iter().take(MOST_LISTED) {
            let Some(agent) = self.agent_at(pane) else { continue };
            let details = self.projects.pane_by(pane).and_then(|term| term.context.context()).map(|c| {
                let percent = c.percent.map(|p| format!(" · {p}%")).unwrap_or_default();
                format!(" · {}{percent}", truncate_right(&c.model, MODEL))
            });
            lines.push(format!(
                "{} {} · {}{}",
                icon(agent.status),
                m.escape(&agent.kind),
                m.escape(&agent.place),
                m.escape(&details.unwrap_or_default())
            ));
            keyboard.push(vec![
                messenger.tap(label(&format!("✎ {} · {}", agent.kind, agent.place)), &Tap::Write(pane)),
                messenger.tap("screen", &Tap::Screen(pane)),
                messenger.tap("esc", &Tap::Key(pane, "esc".into())),
            ]);
        }
        if panes.is_empty() {
            lines.push("No agent is running.".into());
        } else if panes.len() > MOST_LISTED {
            lines.push(format!("… and {} more", panes.len() - MOST_LISTED));
        }
        lines.push(String::new());
        lines.push(if messenger.away {
            "Away is on: I message you here when an agent needs you or finishes.".into()
        } else {
            "Away is off: /away to get messages here.".into()
        });
        keyboard.push(vec![messenger.tap("+ new agent", &Tap::New)]);
        (m.lines(&lines), keyboard)
    }
}
