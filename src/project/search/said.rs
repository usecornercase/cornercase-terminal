use std::collections::HashMap;
use std::num::NonZero;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::agents::context::{Record, Role};
use crate::text::Needle;

pub(crate) const MAX_PER_PANE: usize = 20;
pub(crate) const MAX_MATCHES: usize = 300;
pub(crate) const CORPUS_FOR: Duration = Duration::from_secs(5);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);
const MAX_THREADS: usize = 8;
const WIDTH: usize = 160;
const LEAD: usize = 40;
const MARKS: [char; 6] = ['●', '⎿', '>', '›', '-', '*'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) tab: u64,
    pub(crate) pane: u64,
    pub(crate) label: String,
    pub(crate) who: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub(crate) place: Place,
    pub(crate) screen: Arc<str>,
    pub(crate) record: Option<Record>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Prompt,
    Reply,
    Screen,
}

impl Origin {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Reply => "reply",
            Self::Screen => "screen",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) place: Place,
    pub(crate) origin: Origin,
    pub(crate) line: Option<usize>,
    pub(crate) snippet: String,
    pub(crate) hit: Range<usize>,
    pub(crate) written: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) matches: Vec<Match>,
    pub(crate) capped: bool,
}

pub(crate) fn snippet(line: &str, hit: &Range<usize>) -> (String, Range<usize>) {
    let before = line.get(..hit.start).map_or(0, |head| head.chars().count());
    let len = line.get(hit.clone()).map_or(0, |found| found.chars().count());
    let mut chars: Vec<char> = line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let lead = chars.iter().take_while(|c| c.is_whitespace()).count().min(before);
    chars.drain(..lead);
    let (start, end) = (before - lead, before - lead + len);
    let total = chars.len();
    let from = if total <= WIDTH { 0 } else { start.saturating_sub(LEAD).min(total - WIDTH) };
    let to = (from + WIDTH).min(total);
    let mut text: String = chars[from..to].iter().collect();
    let mut hit = start - from..end.min(to) - from;
    if from > 0 {
        text.insert(0, '…');
        hit = hit.start + 1..hit.end + 1;
    }
    if to < total {
        text.push('…');
    }
    (text, hit)
}

fn first_hit(text: &str, needle: &Needle) -> Option<(usize, String, Range<usize>)> {
    text.lines().enumerate().find_map(|(i, line)| {
        let found = needle.find(line)?;
        let (shown, hit) = snippet(line, &found);
        Some((i, shown, hit))
    })
}

fn normal(text: &str) -> String {
    let text = text.trim_start_matches(|c: char| c.is_whitespace() || MARKS.contains(&c));
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

struct Hits<'a> {
    source: &'a Source,
    needle: &'a Needle,
    cancel: &'a AtomicBool,
    found: Vec<Match>,
    said: Vec<String>,
    capped: bool,
}

impl Hits<'_> {
    fn scan_record(&mut self, record: &Record) {
        let (source, needle, cancel) = (self.source, self.needle, self.cancel);
        let _ = record.said(&mut |spoken| {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            let Some((_, snippet, hit)) = first_hit(&spoken.said.text, needle) else { return true };
            if self.found.len() == MAX_PER_PANE {
                self.capped = true;
                return false;
            }
            let origin = if spoken.role == Role::Prompt { Origin::Prompt } else { Origin::Reply };
            self.said.push(normal(&spoken.said.text));
            self.found.push(Match {
                place: source.place.clone(),
                origin,
                line: None,
                snippet,
                hit,
                written: spoken.said.written,
            });
            true
        });
    }

    fn scan_screen(&mut self) {
        let lines: Vec<&str> = self.source.screen.lines().collect();
        for (i, line) in lines.iter().enumerate().rev() {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            let Some(range) = self.needle.find(line) else { continue };
            let line_normal = normal(line);
            if !line_normal.is_empty() && self.said.iter().any(|text| text.contains(&line_normal)) {
                continue;
            }
            if self.found.len() == MAX_PER_PANE {
                self.capped = true;
                break;
            }
            let (snippet, hit) = snippet(line, &range);
            self.found.push(Match {
                place: self.source.place.clone(),
                origin: Origin::Screen,
                line: Some(i),
                snippet,
                hit,
                written: None,
            });
        }
    }
}

fn pane(source: &Source, needle: &Needle, cancel: &AtomicBool) -> (Vec<Match>, bool) {
    let mut hits = Hits { source, needle, cancel, found: Vec::new(), said: Vec::new(), capped: false };
    if let Some(record) = &source.record {
        hits.scan_record(record);
    }
    if !hits.capped {
        hits.scan_screen();
    }
    (hits.found, hits.capped)
}

type Done = Mutex<Vec<(usize, Vec<Match>, bool)>>;

fn work(sources: &[Source], needle: &Needle, cancel: &AtomicBool, next: &AtomicUsize, done: &Done) {
    while !cancel.load(Ordering::Relaxed) {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some(source) = sources.get(i) else { break };
        let (found, capped) = pane(source, needle, cancel);
        if !found.is_empty() {
            done.lock().push((i, found, capped));
        }
    }
}

pub(crate) fn search(sources: &[Source], query: &str, cancel: &AtomicBool) -> Found {
    let Some(needle) = Needle::new(query) else { return Found::default() };
    let threads = std::thread::available_parallelism().map_or(4, NonZero::get).min(MAX_THREADS);
    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| work(sources, &needle, cancel, &next, &done));
        }
    });
    let mut done = done.into_inner();
    done.sort_unstable_by_key(|(i, ..)| *i);
    let mut capped = done.iter().any(|(_, _, capped)| *capped);
    let mut matches: Vec<Match> = done.into_iter().flat_map(|(_, found, _)| found).collect();
    if matches.len() > MAX_MATCHES {
        matches.truncate(MAX_MATCHES);
        capped = true;
    }
    Found { matches, capped }
}

#[derive(Debug)]
struct Cached {
    at: Instant,
    size: (u16, u16),
    text: Arc<str>,
}

#[derive(Debug)]
struct Running {
    generation: u64,
    query: String,
    cancel: Arc<AtomicBool>,
    at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct Sessions {
    cache: HashMap<u64, Cached>,
    corpus: Option<(Instant, Arc<Vec<Source>>)>,
    generation: u64,
    running: Option<Running>,
    answer: Option<(String, Arc<Found>)>,
}

impl Sessions {
    pub(crate) fn corpus(&self, now: Instant) -> Option<Arc<Vec<Source>>> {
        let (at, sources) = self.corpus.as_ref()?;
        (now.duration_since(*at) < CORPUS_FOR).then(|| Arc::clone(sources))
    }

    pub(crate) fn built(&mut self, now: Instant, sources: Vec<Source>) -> Arc<Vec<Source>> {
        self.cache.retain(|pane, _| sources.iter().any(|source| source.place.pane == *pane));
        let sources = Arc::new(sources);
        self.corpus = Some((now, Arc::clone(&sources)));
        sources
    }

    pub(crate) fn screen(
        &mut self,
        pane: u64,
        at: Instant,
        size: (u16, u16),
        read: impl FnOnce() -> String,
    ) -> Arc<str> {
        if let Some(cached) = self.cache.get(&pane).filter(|c| c.at == at && c.size == size) {
            return Arc::clone(&cached.text);
        }
        let text: Arc<str> = read().into();
        self.cache.insert(pane, Cached { at, size, text: Arc::clone(&text) });
        text
    }

    pub(crate) fn request(&mut self, query: &str, now: Instant) -> Option<(u64, Arc<AtomicBool>)> {
        let query = query.trim();
        if query.is_empty() || self.answer.as_ref().is_some_and(|(asked, _)| asked == query) {
            return None;
        }
        if let Some(running) = &self.running {
            if running.query == query && now.duration_since(running.at) < GIVE_UP_AFTER {
                return None;
            }
            running.cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.running = Some(Running {
            generation: self.generation,
            query: query.to_string(),
            cancel: Arc::clone(&cancel),
            at: now,
        });
        Some((self.generation, cancel))
    }

    pub(crate) fn answered(&mut self, generation: u64, query: &str, found: Found) -> bool {
        if self.running.as_ref().is_none_or(|running| running.generation != generation) {
            return false;
        }
        self.running = None;
        self.answer = Some((query.to_string(), Arc::new(found)));
        true
    }

    pub(crate) fn answer(&self, query: &str) -> Option<Arc<Found>> {
        self.answer.as_ref().filter(|(asked, _)| asked == query.trim()).map(|(_, found)| Arc::clone(found))
    }

    pub(crate) fn searching(&self) -> bool {
        self.running.is_some()
    }

    pub(crate) fn idle(&self) -> bool {
        self.running.is_none() && self.answer.is_none() && self.cache.is_empty() && self.corpus.is_none()
    }

    pub(crate) fn forget(&mut self) {
        if let Some(running) = &self.running {
            running.cancel.store(true, Ordering::Relaxed);
        }
        *self = Self { generation: self.generation, ..Self::default() };
    }
}

#[cfg(test)]
mod tests;
