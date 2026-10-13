use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{Asked, Route};
use crate::agents::Known;
use crate::agents::activity::{SHELL, SPINNER, Status};
use crate::app::App;
use crate::control::{Command, LastMessage};
use crate::project::Hierarchy;
use crate::telegram::{self, Keyboard, Tap, html};
use crate::text::{CRUMB_SEPARATOR, truncate_right};

const SETTLE: Duration = Duration::from_secs(1);
const EXCERPT: usize = 15;
const SCREEN: usize = 40;
const MOST_LISTED: usize = 25;
const LABEL: usize = 40;
const PLACE: usize = 120;
const MODEL: usize = 40;
pub(super) const AGENTS: &str = "<b>Agents</b>";
const WORKING: &str = "working";
const WAITING: &str = "waiting";

pub(super) struct Watch {
    state: &'static str,
    since: Instant,
    settled: &'static str,
    busy: bool,
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
        Some(Status::Waiting) => "!",
        Some(Status::Done) => "✓",
        Some(Status::Working) => SPINNER[0],
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

fn with_screen(header: &str, screen: &str, lines: usize) -> String {
    let screen = html::pre(&excerpt(screen, lines), html::room(header));
    format!("{header}\n{screen}")
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
        let kind = term.agent.agent().map(Known::name)?.to_string();
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

    pub(in crate::app) fn watch_telegram(&mut self, now: Instant) {
        if !self.telegram.paired() {
            self.telegram.watches.clear();
            return;
        }
        let mut seen = HashSet::new();
        let mut notices = Vec::new();
        for term in self.projects.terms() {
            let Some(state) = term.agent.state() else { continue };
            seen.insert(term.id);
            let fresh = Watch { state, since: now, settled: state, busy: false };
            let watch = self.telegram.watches.entry(term.id).or_insert(fresh);
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
                notices.push(Notice::Waiting(term.id));
            } else if over && watch.busy {
                notices.push(Notice::Finished(term.id, state == SHELL));
            }
            watch.settled = state;
            watch.busy &= !over;
        }
        self.telegram.watches.retain(|pane, _| seen.contains(pane));
        if !self.telegram.away {
            return;
        }
        for notice in notices {
            match notice {
                Notice::Waiting(pane) => self.notice_waiting(pane),
                Notice::Finished(pane, shell) => self.notice_finished(pane, shell),
            }
        }
    }

    pub(super) fn notice_waiting(&mut self, pane: u64) {
        let Some(agent) = self.agent_at(pane) else { return };
        let mut keyboard: Keyboard = telegram::choices(&agent.screen)
            .into_iter()
            .map(|(digit, text)| {
                vec![self.telegram.tap(label(&format!("{digit} · {text}")), &Tap::Key(pane, digit.into()))]
            })
            .collect();
        keyboard.push(vec![
            self.telegram.tap("esc", &Tap::Key(pane, "esc".into())),
            self.telegram.tap("screen", &Tap::Screen(pane)),
        ]);
        let header = format!("<b>! {} needs you</b> · {}", html::escape(&agent.kind), html::escape(&agent.place));
        let html = with_screen(&header, &agent.screen, EXCERPT);
        self.say(html, keyboard, Some(Route::Pane(pane)), None);
    }

    fn notice_finished(&mut self, pane: u64, shell: bool) {
        let Some(agent) = self.agent_at(pane) else { return };
        let what = if shell { "ended its turn" } else { "finished" };
        let mut header = format!("<b>✓ {} {what}</b> · {}", html::escape(&agent.kind), html::escape(&agent.place));
        if shell {
            header.push_str(" · a background shell still runs");
        }
        let command = Command::LastMessage(LastMessage { pane: Some(pane), tab: None });
        self.telegram_request(command, Asked::Finished { pane, header }, None);
    }

    pub(super) fn finished_read(&mut self, pane: u64, header: &str, reply: Result<Value, String>) {
        let keyboard = vec![vec![self.telegram.tap("screen", &Tap::Screen(pane))]];
        let text = reply.ok().and_then(|value| value["text"].as_str().map(String::from));
        let parts = match text {
            Some(text) if !text.trim().is_empty() => html::markdown(&text),
            _ => {
                let screen = self.agent_at(pane).map(|agent| agent.screen).unwrap_or_default();
                vec![html::pre(&excerpt(&screen, EXCERPT), html::room(header))]
            }
        };
        self.say_long(header, parts, keyboard, Some(Route::Pane(pane)));
    }

    pub(super) fn send_screen(&mut self, pane: u64) {
        let Some(agent) = self.agent_at(pane) else {
            self.say(super::chat::GONE.into(), Vec::new(), None, None);
            return;
        };
        let keyboard = vec![vec![
            self.telegram.tap("refresh", &Tap::Screen(pane)),
            self.telegram.tap("esc", &Tap::Key(pane, "esc".into())),
        ]];
        let header =
            format!("{} <b>{}</b> · {}", icon(agent.status), html::escape(&agent.kind), html::escape(&agent.place));
        let html = with_screen(&header, &agent.screen, SCREEN);
        self.say(html, keyboard, Some(Route::Pane(pane)), None);
    }

    pub(super) fn status_message(&self) -> (Vec<String>, Keyboard) {
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
                html::escape(&agent.kind),
                html::escape(&agent.place),
                html::escape(&details.unwrap_or_default())
            ));
            keyboard.push(vec![
                self.telegram.tap(label(&format!("✎ {} · {}", agent.kind, agent.place)), &Tap::Write(pane)),
                self.telegram.tap("screen", &Tap::Screen(pane)),
                self.telegram.tap("esc", &Tap::Key(pane, "esc".into())),
            ]);
        }
        if panes.is_empty() {
            lines.push("No agent is running.".into());
        } else if panes.len() > MOST_LISTED {
            lines.push(format!("… and {} more", panes.len() - MOST_LISTED));
        }
        lines.push(String::new());
        lines.push(if self.telegram.away {
            "Away is on: I message you here when an agent needs you or finishes.".into()
        } else {
            "Away is off: /away to get messages here.".into()
        });
        keyboard.push(vec![self.telegram.tap("+ new agent", &Tap::New)]);
        (html::lines(&lines), keyboard)
    }
}
