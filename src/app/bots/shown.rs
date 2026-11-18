use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::text::Line as Text;

use crate::app::App;
use crate::bots::chat::{self, Kind, Line};
use crate::bots::follow;
use crate::project::Hierarchy;
use crate::ui::{self, ChatEntry};

pub(in crate::app) struct Chat {
    pub(in crate::app) bot: String,
    pub(in crate::app) lines: Vec<Line>,
    pub(in crate::app) scroll: usize,
    pub(in crate::app) news: bool,
    pub(in crate::app) unfolded: HashSet<u64>,
    pub(in crate::app) anchor: Option<u64>,
    pub(super) width: u16,
    pub(super) bodies: HashMap<u64, Vec<Text<'static>>>,
    pub(super) card: Option<DrawnCard>,
}

#[derive(Debug, Clone)]
pub(super) struct DrawnCard {
    pub(super) card: ui::BotCard,
    pub(super) button: Rect,
}

impl Chat {
    pub(super) fn new(bot: String, lines: Vec<Line>) -> Self {
        Self {
            bot,
            lines,
            scroll: 0,
            news: false,
            unfolded: HashSet::new(),
            anchor: None,
            width: 0,
            bodies: HashMap::new(),
            card: None,
        }
    }
}

pub(super) struct Facts {
    name: String,
    held: Vec<u64>,
    zone: String,
    now: i64,
    retry: Option<u64>,
    answering: HashSet<u64>,
    again: HashSet<u64>,
    panes: HashSet<u64>,
    boot: String,
}

pub(in crate::app) struct Shown {
    pub(in crate::app) view: ui::ChatView,
    pub(in crate::app) card: ui::BotCard,
}

enum Group {
    Chat(Range<usize>),
    Run(u64, Vec<usize>),
}

fn groups(lines: &[Line]) -> Vec<Group> {
    let mut found: Vec<Group> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(run) = line.routine {
            match found.iter_mut().find(|group| matches!(group, Group::Run(found, _) if *found == run)) {
                Some(Group::Run(_, members)) => members.push(i),
                _ => found.push(Group::Run(run, vec![i])),
            }
            continue;
        }
        match found.last_mut() {
            Some(Group::Chat(last)) if line.kind == Kind::Step && lines[last.start].kind == Kind::Step => {
                last.end = i + 1;
            }
            _ => found.push(Group::Chat(i..i + 1)),
        }
    }
    found
}

fn clock(at: i64, facts: &Facts) -> String {
    crate::queue::clock::when(at, facts.now, &facts.zone)
}

fn duration(took: Duration) -> String {
    let seconds = took.as_secs();
    if seconds < 60 { format!("{seconds}s") } else { format!("{}m {}s", seconds / 60, seconds % 60) }
}

impl Chat {
    fn body(&mut self, line: &Line, width: u16) -> Vec<Text<'static>> {
        self.markdown(line.id, &line.text, width)
    }

    fn markdown(&mut self, id: u64, text: &str, width: u16) -> Vec<Text<'static>> {
        self.bodies
            .entry(id)
            .or_insert_with(|| crate::highlight::markdown::render(text, usize::from(width.max(1))))
            .clone()
    }

    fn agent_entry(&mut self, line: &Line, facts: &Facts, width: u16) -> ChatEntry {
        let kind = match line.kind {
            Kind::Started => ui::ChatAgentKind::Started,
            Kind::Finished => ui::ChatAgentKind::Finished { failed: follow::failed(&line.text) },
            _ => ui::ChatAgentKind::Waiting,
        };
        let shown = |agent: &&chat::AgentRef| agent.boot == facts.boot && facts.panes.contains(&agent.pane);
        let at = line.agent.as_ref().filter(shown).map(|agent| agent.pane);
        let text = line.body.as_deref().unwrap_or_default();
        let body = match kind {
            ui::ChatAgentKind::Finished { .. } => self.markdown(line.id, text, width.saturating_sub(2)),
            ui::ChatAgentKind::Waiting => text.lines().map(|row| Text::from(row.to_string())).collect(),
            ui::ChatAgentKind::Started => Vec::new(),
        };
        let open = self.unfolded.contains(&line.id);
        ChatEntry::Agent(ui::ChatAgentLine { id: line.id, state: kind, text: line.text.clone(), body, open, link: at })
    }

    fn run_block(&mut self, run: u64, members: &[usize], facts: &Facts, width: u16) -> ChatEntry {
        let lines: Vec<Line> = members.iter().map(|at| self.lines[*at].clone()).collect();
        let head = match lines.first() {
            Some(first) if first.kind == Kind::Notice && chat::head_parts(&first.text).is_some() => first.text.clone(),
            _ => String::from("↻ a routine"),
        };
        let steps: Vec<&Line> = lines.iter().filter(|line| line.kind == Kind::Step).collect();
        let answer = lines
            .iter()
            .find(|line| line.kind == Kind::Bot)
            .map(|line| (format!("{} · {}", facts.name, clock(line.at, facts)), self.body(line, width)));
        let error = lines.iter().find(|line| line.kind == Kind::Error).map(|line| line.text.clone());
        let ended =
            |line: &&Line| line.kind == Kind::Notice && (line.text == chat::STOPPED || line.text == chat::INTERRUPTED);
        let first = steps.first().copied().or(lines.first()).map_or(0, |line| line.id);
        ChatEntry::Run(ui::RunBlock {
            run,
            head,
            first,
            steps: steps.iter().map(|line| line.text.clone()).collect(),
            open: self.unfolded.contains(&first),
            answer,
            error,
            again: facts.again.contains(&run),
            end: lines.iter().find(ended).map(|line| line.text.clone()),
            answering: facts.answering.contains(&run),
        })
    }

    fn entry(&mut self, group: &Group, facts: &Facts, width: u16) -> ChatEntry {
        let group = match group {
            Group::Run(run, members) => return self.run_block(*run, members, facts, width),
            Group::Chat(range) => range,
        };
        let line = self.lines[group.start].clone();
        match line.kind {
            Kind::You => {
                let held = facts.held.contains(&line.id);
                let stamp = if held { String::from("held") } else { clock(line.at, facts) };
                ChatEntry::You { text: line.text, stamp }
            }
            Kind::Bot => {
                let head = format!("{} · {}", facts.name, clock(line.at, facts));
                ChatEntry::Bot { head, body: self.body(&line, width) }
            }
            Kind::Step => {
                let texts = self.lines[group.clone()].iter().map(|step| step.text.clone()).collect();
                ChatEntry::Steps { first: line.id, texts, open: self.unfolded.contains(&line.id) }
            }
            Kind::Error => ChatEntry::Error { id: line.id, text: line.text, retry: facts.retry == Some(line.id) },
            Kind::Notice => ChatEntry::Notice(line.text),
            Kind::Started | Kind::Finished | Kind::Waiting => self.agent_entry(&line, facts, width),
        }
    }

    pub(super) fn rows(&mut self, facts: &Facts, width: u16, need: usize, tone: ui::Tone) -> (Vec<ui::ChatRow>, bool) {
        if self.width != width {
            self.width = width;
            self.bodies.clear();
        }
        let all = groups(&self.lines);
        let mut entries = Vec::new();
        let (mut sum, mut gaps, mut reached) = (0, 0, true);
        for group in all.iter().rev() {
            let oldest_gap = usize::from(entries.last().is_some_and(|entry| !matches!(entry, ChatEntry::Steps { .. })));
            if sum + gaps - oldest_gap >= need {
                reached = false;
                break;
            }
            let entry = self.entry(group, facts, width);
            sum += ui::entry_rows(&entry, width, tone).len();
            gaps += usize::from(!matches!(entry, ChatEntry::Steps { .. }));
            entries.push(entry);
        }
        entries.reverse();
        (ui::chat_rows(&entries, width, tone), reached)
    }

    fn tail_start(&self) -> usize {
        groups(&self.lines).len().saturating_sub(2)
    }

    fn tail_height(&mut self, from: usize, facts: &Facts, width: u16, tone: ui::Tone) -> usize {
        let all = groups(&self.lines);
        let entries: Vec<ChatEntry> =
            all[from.min(all.len())..].iter().map(|group| self.entry(group, facts, width)).collect();
        ui::chat_rows(&entries, width, tone).len()
    }

    fn group_of(&self, line: u64) -> Option<usize> {
        let at = self.lines.iter().position(|found| found.id == line)?;
        groups(&self.lines).iter().position(|group| match group {
            Group::Chat(range) => range.contains(&at),
            Group::Run(_, members) => members.contains(&at),
        })
    }

    fn run_group(&self, run: u64) -> Option<usize> {
        groups(&self.lines).iter().position(|group| matches!(group, Group::Run(found, _) if *found == run))
    }
}

impl App {
    fn chat_facts(&self, at: usize) -> Facts {
        let live = &self.panels.bots.live[at];
        let last = self
            .panels
            .bots
            .shown
            .as_ref()
            .and_then(|shown| shown.lines.iter().rev().find(|line| line.routine.is_none()));
        let lines = self.panels.bots.shown.as_ref().map(|shown| shown.lines.as_slice()).unwrap_or_default();
        let ended = |line: &&Line| {
            line.kind == Kind::Error
                && live.turn.is_none()
                && chat::prompt_of(lines, line.id).is_some_and(|prompt| prompt.kind != Kind::Finished)
        };
        let retry = last.filter(ended).map(|line| line.id);
        Facts {
            name: live.bot.name.clone(),
            held: live.held.iter().map(|(id, _)| *id).collect(),
            zone: self.window_zone(),
            now: crate::time::now(),
            retry,
            answering: live.runs.iter().map(|run| run.entry).collect(),
            again: self.runs_again(),
            panes: self.projects.terms().map(|term| term.id).collect(),
            boot: crate::control::server_token().to_string(),
        }
    }

    fn runs_again(&self) -> HashSet<u64> {
        let Some(shown) = self.panels.bots.shown.as_ref() else { return HashSet::new() };
        let queue = self.panels.queued.queue();
        let runs: HashSet<u64> = shown.lines.iter().filter_map(|line| line.routine).collect();
        let again = |entry: &crate::queue::Entry| match entry.task.routine {
            Some(routine) => queue.entry(routine).is_some(),
            None => matches!(entry.state, crate::queue::State::Failed { .. } | crate::queue::State::Missed),
        };
        runs.into_iter().filter(|run| queue.entry(*run).is_some_and(again)).collect()
    }

    fn thinking_text(&self, at: usize) -> Option<ui::Thinking> {
        let turn = self.panels.bots.live[at].turn.as_ref()?;
        if turn.stopping {
            return Some(ui::Thinking { text: String::from("stopping…"), stopping: true });
        }
        let mut text = format!("thinking · {}", duration(turn.started.elapsed()));
        if turn.steps > 0 {
            text = format!("{text} · {}", crate::text::plural(turn.steps, "step", "steps"));
        }
        Some(ui::Thinking { text, stopping: false })
    }

    pub(super) fn chat_tone(&self) -> ui::Tone {
        ui::Tone { muted: ui::muted(self.themes.host()), surface: ui::surface_colour(self.themes.light()) }
    }

    pub(in crate::app) fn shown_chat(&mut self, area: Rect) -> Option<Shown> {
        let at = self.shown_bot()?;
        let tone = self.chat_tone();
        let draft = self.panels.bots.live[at].draft.clone();
        let areas = self.chat_geometry(area);
        let thinking = self.thinking_text(at);
        let body = ui::chat_body(areas.lines, thinking.is_some());
        let facts = self.chat_facts(at);
        let focused = self.chat_typing();
        let shown = self.panels.bots.shown.as_mut()?;
        let empty = shown.lines.is_empty() && thinking.is_none();
        let (rows, news) =
            if empty { (ui::empty_rows(body.width, tone), false) } else { shown.visible(&facts, body, tone) };
        let live = &self.panels.bots.live[at];
        let view = ui::ChatView {
            name: live.bot.name.clone(),
            detail: self.bot_detail(at),
            rows,
            empty,
            thinking,
            news,
            busy: live.turn.is_some(),
            input: draft,
            focused,
            placeholder: format!("Message {}…", live.bot.name),
        };
        Some(Shown { view, card: self.bot_card(at) })
    }
}

impl Chat {
    fn visible(&mut self, facts: &Facts, body: Rect, tone: ui::Tone) -> (Vec<ui::ChatRow>, bool) {
        let height = usize::from(body.height);
        if let Some(from) = self.anchor.take().and_then(|line| self.group_of(line)) {
            self.scroll = self.tail_height(from, facts, body.width, tone).saturating_sub(height);
            self.news = false;
        }
        let (rows, reached) = self.rows(facts, body.width, self.scroll + height, tone);
        let total = rows.len();
        if reached {
            self.scroll = self.scroll.min(total.saturating_sub(height));
        }
        if self.scroll == 0 {
            self.news = false;
        }
        let end = total.saturating_sub(self.scroll);
        let start = end.saturating_sub(height);
        (rows[start..end].to_vec(), self.news)
    }
}

impl App {
    pub(super) fn chat_took(&mut self, at: usize, line: &Line) {
        let id = self.panels.bots.live[at].bot.id.clone();
        let Some(shown) = self.panels.bots.shown.as_ref().filter(|shown| shown.bot == id) else { return };
        let (scrolled, width) = (shown.scroll > 0, shown.width);
        let (facts, tone) = (self.chat_facts(at), self.chat_tone());
        let Some(shown) = self.panels.bots.shown.as_mut() else { return };
        let tail = shown.tail_start();
        let from = line.routine.and_then(|run| shown.run_group(run)).map_or(tail, |at| at.min(tail));
        let before = if scrolled { shown.tail_height(from, &facts, width, tone) } else { 0 };
        shown.lines.push(line.clone());
        let height = if scrolled { shown.tail_height(from, &facts, width, tone).saturating_sub(before) } else { 0 };
        let extra = shown.lines.len().saturating_sub(super::window::CHAT_KEPT);
        shown.lines.drain(..extra);
        if scrolled {
            shown.scroll += height;
            shown.news = true;
        }
    }
}
