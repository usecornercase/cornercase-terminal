use std::collections::BTreeMap;
use std::time::SystemTime;

use super::journal::{Entry, Event, Pointer, PullPlace};
use super::range::Range;
use super::{Choice, Named, Place};
use crate::agents::context::{Record, Role};
use crate::time::Stamp;

mod noise;
mod render;

const MAX_MESSAGES: usize = 4000;

pub(crate) struct Caps {
    pub(super) prompts: usize,
    pub(super) prompt_chars: usize,
    pub(super) reply_chars: usize,
    pub(super) screen: bool,
    pub(super) conversations: usize,
    pub(super) total: usize,
}

impl Caps {
    pub(crate) const fn of(choice: Choice) -> Self {
        match choice {
            Choice::Now => Self {
                prompts: 6,
                prompt_chars: 500,
                reply_chars: 800,
                screen: true,
                conversations: 60,
                total: 40 * 1024,
            },
            Choice::Yesterday => Self {
                prompts: 8,
                prompt_chars: 500,
                reply_chars: 800,
                screen: false,
                conversations: 60,
                total: 60 * 1024,
            },
            Choice::Week => Self {
                prompts: 4,
                prompt_chars: 300,
                reply_chars: 300,
                screen: false,
                conversations: 120,
                total: 100 * 1024,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LivePane {
    pub(crate) key: String,
    pub(crate) status: Option<String>,
    pub(crate) template: Entry,
    pub(crate) screen: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PullLive {
    pub(crate) workspace: String,
    pub(crate) number: u64,
    pub(crate) title: Option<String>,
    pub(crate) state: String,
    pub(crate) checks: Option<String>,
    pub(crate) url: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Live {
    pub(crate) panes: Vec<LivePane>,
    pub(crate) pulls: Vec<PullLive>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Dossier {
    pub(crate) text: String,
    pub(crate) places: BTreeMap<String, Place>,
    pub(crate) conversations: usize,
}

pub(super) struct Group {
    pub(super) key: String,
    pub(super) template: Entry,
    pub(super) first: i64,
    pub(super) last: i64,
    pub(super) ended: Option<i64>,
    pub(super) last_status: Option<String>,
    pub(super) live: Option<LivePane>,
    pub(super) prompted: bool,
    pub(super) worked: bool,
}

pub(super) struct Said {
    pub(super) at: Option<i64>,
    pub(super) text: String,
}

pub(super) struct Talk {
    pub(super) prompts: Vec<Said>,
    pub(super) reply: Option<String>,
}

fn active(entry: &Entry) -> bool {
    entry.event == Event::Prompt || (entry.event == Event::Status && entry.to.as_deref() == Some("working"))
}

fn groups(range: &Range, entries: &[Entry], live: &Live) -> Vec<Group> {
    let mut sorted: Vec<&Entry> = entries.iter().filter(|entry| entry.key().is_some()).collect();
    sorted.sort_by_key(|entry| entry.seconds());
    let mut found: Vec<Group> = Vec::new();
    for entry in sorted {
        let (Some(key), Some(at)) = (entry.key(), entry.seconds()) else { continue };
        let index = found.iter().position(|group| group.key == key).unwrap_or_else(|| {
            found.push(Group::new(&key, entry, at));
            found.len() - 1
        });
        found[index].add(entry, at);
    }
    found.retain(|group| range.choice == Choice::Now || group.prompted || group.worked);
    attach_live(found, range, live)
}

impl Group {
    fn new(key: &str, entry: &Entry, at: i64) -> Self {
        Self {
            key: key.to_string(),
            template: entry.clone(),
            first: at,
            last: at,
            ended: None,
            last_status: None,
            live: None,
            prompted: false,
            worked: false,
        }
    }

    fn add(&mut self, entry: &Entry, at: i64) {
        self.template = Entry { event: entry.event, to: entry.to.clone(), ..entry.clone() };
        self.last = self.last.max(at);
        self.first = self.first.min(at);
        self.prompted |= entry.event == Event::Prompt;
        self.worked |= active(entry);
        match entry.event {
            Event::Closed => self.ended = Some(at),
            Event::Seen => self.ended = None,
            Event::Status => {
                self.ended = None;
                self.last_status.clone_from(&entry.to);
            }
            Event::Prompt => {}
        }
    }
}

fn attach_live(mut found: Vec<Group>, range: &Range, live: &Live) -> Vec<Group> {
    for pane in &live.panes {
        match found.iter_mut().find(|group| group.key == pane.key) {
            Some(group) => {
                group.live = Some(pane.clone());
                group.template = Entry { event: Event::Seen, to: None, ..pane.template.clone() };
                group.ended = None;
            }
            None if range.choice == Choice::Now => found.push(live_only(pane, range)),
            None => {}
        }
    }
    found
}

fn live_only(pane: &LivePane, range: &Range) -> Group {
    let mut group = Group::new(&pane.key, &pane.template, range.to);
    group.live = Some(pane.clone());
    group
}

fn seconds_of(written: Option<&str>) -> Option<i64> {
    let since = Stamp::parse(written?)?.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(since.as_secs()).ok()
}

fn talk(record: &Record, range: &Range, caps: &Caps, prompted: bool) -> Talk {
    let mut all = Vec::new();
    let _ = record.said(&mut |spoken| {
        all.push(spoken);
        all.len() < MAX_MESSAGES
    });
    let stamped = all.iter().all(|spoken| spoken.said.written.is_some());
    if stamped {
        all.sort_by_key(|spoken| seconds_of(spoken.said.written.as_deref()));
    }
    let reply = all.iter().rev().find(|spoken| spoken.role == Role::Reply).map(|spoken| spoken.said.text.clone());
    let inside = |at: Option<i64>| at.map_or(prompted, |at| at >= range.from && at < range.to);
    let mut prompts: Vec<Said> = all
        .into_iter()
        .filter(|spoken| spoken.role == Role::Prompt)
        .map(|spoken| Said { at: seconds_of(spoken.said.written.as_deref()), text: spoken.said.text })
        .filter(|said| inside(said.at) && !noise::is_noise(&said.text))
        .collect();
    let keep = prompts.len().saturating_sub(caps.prompts);
    prompts.drain(..keep);
    Talk { prompts, reply }
}

fn pull_of(group: &Group, live: &Live) -> Option<(PullPlace, Option<String>)> {
    let workspace = group.template.workspace.as_ref()?;
    let pulled = group.template.pull_request.clone();
    let current = live.pulls.iter().find(|pull| pull.workspace == workspace.path);
    match (current, pulled) {
        (Some(pull), _) => Some((
            PullPlace {
                number: pull.number,
                url: pull.url.clone(),
                state: pull.state.clone(),
                ci: pull.checks.clone(),
            },
            pull.title.clone(),
        )),
        (None, Some(pulled)) => Some((pulled, None)),
        (None, None) => None,
    }
}

pub(crate) fn build(range: &Range, entries: &[Entry], live: &Live) -> Option<Dossier> {
    let caps = Caps::of(range.choice);
    let mut found = groups(range, entries, live);
    if found.is_empty() {
        return None;
    }
    found.sort_by_key(|group| render::order(group, range.choice));
    found.truncate(caps.conversations);
    let talks: Vec<Talk> = found
        .iter()
        .map(|group| match group.template.record.as_ref().map(Pointer::record) {
            Some(record) => talk(&record, range, &caps, group.prompted),
            None => Talk { prompts: Vec::new(), reply: None },
        })
        .collect();
    Some(render::render(range, &caps, &found, &talks, live))
}

pub(super) fn project_of(group: &Group) -> Option<&Named> {
    group.template.project.as_ref()
}

#[cfg(test)]
mod tests;
