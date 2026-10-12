use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::activity::Status;
use crate::log::Stamp;
use crate::state;

pub const WEEKS: usize = 26;
pub const BLOCKS: usize = WEEKS * 7;
const RETAIN: u64 = (BLOCKS as u64 + 1) * 24;
pub const SHARE_BASE: &str = "https://usecornercase.dev/stats/";
pub const EMPTY: &str = "nothing counted yet: agents running in cornercase count from now on";
pub const LABELS: [&str; 9] = [
    "agents started",
    "prompts",
    "turns",
    "agent time",
    "most at once",
    "waited for you",
    "tokens out",
    "tokens in",
    "tokens from cache",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Claude,
    Codex,
    Gemini,
    Opencode,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Gemini, Self::Opencode];
    pub fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Opencode => "opencode",
        }
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    #[serde(rename = "in")]
    pub input: u64,
    pub out: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    #[must_use]
    pub fn difference(self, before: Self) -> Self {
        Self {
            input: self.input.saturating_sub(before.input),
            out: self.out.saturating_sub(before.out),
            cache_read: self.cache_read.saturating_sub(before.cache_read),
            cache_write: self.cache_write.saturating_sub(before.cache_write),
        }
    }
    pub fn decreased(self, before: Self) -> bool {
        self.input < before.input
            || self.out < before.out
            || self.cache_read < before.cache_read
            || self.cache_write < before.cache_write
    }
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

#[derive(Debug, Default)]
pub struct Counted {
    pub prompts: Vec<SystemTime>,
    pub tokens: Vec<(SystemTime, Tokens)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    agents: u64,
    prompts: u64,
    turns: u64,
    agent_ms: u64,
    waited_ms: u64,
    tokens: Tokens,
}

impl Counts {
    fn packed(self) -> [u64; 9] {
        [
            self.agents,
            self.prompts,
            self.turns,
            self.agent_ms,
            self.waited_ms,
            self.tokens.input,
            self.tokens.out,
            self.tokens.cache_read,
            self.tokens.cache_write,
        ]
    }
    fn unpacked(a: [u64; 9]) -> Self {
        Self {
            agents: a[0],
            prompts: a[1],
            turns: a[2],
            agent_ms: a[3],
            waited_ms: a[4],
            tokens: Tokens { input: a[5], out: a[6], cache_read: a[7], cache_write: a[8] },
        }
    }
    fn add(&mut self, other: Self) {
        let a = self.packed();
        let b = other.packed();
        *self = Self::unpacked(std::array::from_fn(|i| a[i].saturating_add(b[i])));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bucket {
    by_agent: [Counts; 4],
    busy_ms: u64,
    most_at_once: u64,
}

impl Bucket {
    fn add(&mut self, other: &Self) {
        for (a, b) in self.by_agent.iter_mut().zip(other.by_agent) {
            a.add(b);
        }
        self.busy_ms = self.busy_ms.saturating_add(other.busy_ms);
        self.most_at_once = self.most_at_once.max(other.most_at_once);
    }
    fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for a in self.by_agent {
            counts.add(a);
        }
        counts
    }
    fn window(&self) -> Window {
        let counts = self.counts();
        Window {
            counts: Metrics::from(counts),
            busy_seconds: seconds(self.busy_ms),
            most_at_once: self.most_at_once,
            by_agent: Kind::ALL
                .into_iter()
                .map(|kind| (kind.name().into(), Metrics::from(self.by_agent[kind.index()])))
                .collect(),
        }
    }
}

type Hour = (u64, [[u64; 9]; 4], u64, u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub version: u32,
    pub since: u64,
    pub total: Bucket,
    pub hours: Vec<Hour>,
}

#[derive(Debug)]
pub struct Stats {
    since: SystemTime,
    total: Bucket,
    hours: BTreeMap<u64, Bucket>,
    previous: HashMap<u64, Sample>,
    started: HashMap<u64, (Kind, i32)>,
    revision: u64,
    pruned_hour: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub pane: u64,
    pub kind: Kind,
    pub pid: i32,
    pub status: Option<Status>,
    pub background: bool,
    pub resuming: bool,
}

impl Sample {
    fn working(self) -> bool {
        self.status == Some(Status::Working) && !self.background
    }
    fn in_turn(self) -> bool {
        matches!(self.status, Some(Status::Working | Status::Waiting)) && !self.background
    }
    fn over(self) -> bool {
        matches!(self.status, Some(Status::Idle | Status::Done)) || self.background
    }
}

fn epoch(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
fn hour(at: SystemTime) -> u64 {
    epoch(at) / 3_600
}
#[expect(clippy::cast_precision_loss, reason = "millisecond counters retain practical duration precision")]
fn seconds(ms: u64) -> f64 {
    ms as f64 / 1_000.0
}

impl Stats {
    pub fn new(now: SystemTime) -> Self {
        Self {
            since: now,
            total: Bucket::default(),
            hours: BTreeMap::new(),
            previous: HashMap::new(),
            started: HashMap::new(),
            revision: 0,
            pruned_hour: None,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    fn add(&mut self, at: SystemTime, kind: Kind, counts: Counts) {
        self.revision = self.revision.saturating_add(1);
        self.total.by_agent[kind.index()].add(counts);
        self.hours.entry(hour(at)).or_default().by_agent[kind.index()].add(counts);
    }
    pub fn counted(&mut self, kind: Kind, counted: Counted) {
        for at in counted.prompts {
            self.add(at, kind, Counts { prompts: 1, ..Counts::default() });
        }
        for (at, tokens) in counted.tokens {
            if !tokens.is_empty() {
                self.add(at, kind, Counts { tokens, ..Counts::default() });
            }
        }
    }
    pub fn sample(&mut self, now: SystemTime, elapsed: Duration, samples: &[Sample]) {
        let ms = u64::try_from(elapsed.min(Duration::from_secs(2)).as_millis()).unwrap_or(2_000);
        let mut simultaneous = 0;
        let mut busy = false;
        for &sample in samples {
            if self.started.insert(sample.pane, (sample.kind, sample.pid)) != Some((sample.kind, sample.pid))
                && !sample.resuming
            {
                self.add(now, sample.kind, Counts { agents: 1, ..Counts::default() });
            }
            if sample.working() {
                simultaneous += 1;
            }
            if let Some(previous) = self.previous.get(&sample.pane).copied()
                && previous.kind == sample.kind
                && previous.pid == sample.pid
            {
                busy |= previous.working();
                let counts = Counts {
                    turns: u64::from(previous.in_turn() && sample.over()),
                    agent_ms: if previous.working() { ms } else { 0 },
                    waited_ms: if previous.status.is_some_and(Status::needs_you) { ms } else { 0 },
                    ..Counts::default()
                };
                if counts != Counts::default() {
                    self.add(now, sample.kind, counts);
                }
            }
        }
        if simultaneous > 0 || busy {
            let addition =
                Bucket { busy_ms: if busy { ms } else { 0 }, most_at_once: simultaneous, ..Bucket::default() };
            let bucket = self.hours.entry(hour(now)).or_default();
            if addition.busy_ms > 0
                || addition.most_at_once > bucket.most_at_once
                || addition.most_at_once > self.total.most_at_once
            {
                self.revision = self.revision.saturating_add(1);
                self.total.add(&addition);
                bucket.add(&addition);
            }
        }
        self.previous = samples.iter().map(|s| (s.pane, *s)).collect();
    }
    pub fn saved(&self, now: SystemTime) -> Saved {
        let oldest = hour(now).saturating_sub(RETAIN - 1);
        Saved {
            version: 1,
            since: epoch(self.since),
            total: self.total.clone(),
            hours: self
                .hours
                .range(oldest..)
                .map(|(&at, bucket)| (at, bucket.by_agent.map(Counts::packed), bucket.busy_ms, bucket.most_at_once))
                .collect(),
        }
    }
    pub fn prune(&mut self, now: SystemTime) {
        let current = hour(now);
        if self.pruned_hour == Some(current) {
            return;
        }
        self.pruned_hour = Some(current);
        let oldest = current.saturating_sub(RETAIN - 1);
        if self.hours.first_key_value().is_some_and(|(&at, _)| at < oldest) {
            self.hours = self.hours.split_off(&oldest);
            self.revision = self.revision.saturating_add(1);
        }
    }
    fn window(&self, now: SystemTime, hours: u64) -> Bucket {
        let end = hour(now);
        let start = end.saturating_sub(hours - 1);
        let mut bucket = Bucket::default();
        for (_, other) in self.hours.range(start..=end) {
            bucket.add(other);
        }
        bucket
    }
    pub fn days(&self, now: SystemTime) -> u64 {
        now.duration_since(self.since).unwrap_or_default().as_secs() / 86_400
    }
    pub fn view(&self, now: SystemTime) -> View {
        let blocks: Vec<Block> = (0..BLOCKS)
            .map(|i| {
                let offset = i as u64 * 24;
                let seconds = hour(now).checked_sub(offset).map_or(0.0, |end| {
                    let mut bucket = Bucket::default();
                    for (_, b) in self.hours.range(end.saturating_sub(23)..=end) {
                        bucket.add(b);
                    }
                    seconds(bucket.counts().agent_ms)
                });
                Block { agent_seconds: seconds }
            })
            .collect();
        View {
            since: Stamp(self.since).to_string(),
            windows: BTreeMap::from([
                ("24h".into(), self.window(now, 24).window()),
                ("7d".into(), self.window(now, 168).window()),
                ("total".into(), self.total.window()),
            ]),
            blocks,
            days: self.days(now),
            empty: self.total == Bucket::default(),
        }
    }
}

pub fn path(session: &Path) -> PathBuf {
    session.with_file_name("stats.json")
}
pub fn load(path: &Path, now: SystemTime) -> Stats {
    let mut stats = Stats::new(now);
    if let Some(saved) = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
        .filter(|s| s.version == 1)
        && let Some(since) = UNIX_EPOCH.checked_add(Duration::from_secs(saved.since))
    {
        stats.since = since;
        stats.total = saved.total;
        stats.hours = saved
            .hours
            .into_iter()
            .map(|(at, agents, busy_ms, most_at_once)| {
                (at, Bucket { by_agent: agents.map(Counts::unpacked), busy_ms, most_at_once })
            })
            .collect();
        stats.prune(now);
    }
    stats
}
pub fn save(path: &Path, saved: &Saved) -> std::io::Result<()> {
    state::write(path, saved, true)
}

#[derive(Debug)]
pub struct Saver {
    path: PathBuf,
    revision: u64,
    dirty: Option<Instant>,
}

impl Saver {
    pub fn new(path: PathBuf, revision: u64) -> Self {
        Self { path, revision, dirty: None }
    }
    pub fn observe(&mut self, stats: &Stats, wall: SystemTime, now: Instant) -> std::io::Result<bool> {
        if stats.revision() == self.revision {
            return Ok(false);
        }
        let since = *self.dirty.get_or_insert(now);
        if now.saturating_duration_since(since) < Duration::from_secs(60) {
            return Ok(false);
        }
        self.flush(stats, wall)?;
        Ok(true)
    }
    pub fn flush(&mut self, stats: &Stats, wall: SystemTime) -> std::io::Result<()> {
        if stats.revision() != self.revision {
            save(&self.path, &stats.saved(wall))?;
            self.revision = stats.revision();
            self.dirty = None;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub agents: u64,
    pub prompts: u64,
    pub turns: u64,
    pub agent_seconds: f64,
    pub waited_seconds: f64,
    pub tokens: Tokens,
}
impl From<Counts> for Metrics {
    fn from(c: Counts) -> Self {
        Self {
            agents: c.agents,
            prompts: c.prompts,
            turns: c.turns,
            agent_seconds: seconds(c.agent_ms),
            waited_seconds: seconds(c.waited_ms),
            tokens: c.tokens,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Window {
    #[serde(flatten)]
    pub counts: Metrics,
    pub busy_seconds: f64,
    pub most_at_once: u64,
    pub by_agent: BTreeMap<String, Metrics>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub agent_seconds: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub since: String,
    pub windows: BTreeMap<String, Window>,
    pub blocks: Vec<Block>,
    #[serde(skip)]
    pub days: u64,
    #[serde(skip)]
    pub empty: bool,
}
impl Default for View {
    fn default() -> Self {
        Stats::new(UNIX_EPOCH).view(UNIX_EPOCH)
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "nonnegative durations are bounded to u64")]
pub fn duration(secs: f64) -> String {
    let minutes = (secs.max(0.0) / 60.0) as u64;
    let (h, m) = (minutes / 60, minutes % 60);
    if h == 0 {
        format!("{m}m")
    } else if m == 0 {
        format!("{h}h")
    } else {
        format!("{h}h {m:02}m")
    }
}
#[expect(clippy::cast_precision_loss, reason = "display abbreviations do not require exact large integers")]
fn abbreviated(n: u64, units: &[(u64, &str)]) -> String {
    for &(scale, suffix) in units {
        if n >= scale {
            let value = n as f64 / scale as f64;
            return if value < 10.0 { format!("{value:.1}{suffix}") } else { format!("{value:.0}{suffix}") };
        }
    }
    n.to_string()
}
#[expect(clippy::cast_precision_loss, reason = "display abbreviations do not require exact large integers")]
pub fn count(n: u64) -> String {
    if n < 10_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        abbreviated(n, &[(1_000_000_000, "B"), (1_000_000, "M")])
    }
}
pub fn tokens(n: u64) -> String {
    abbreviated(n, &[(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "k")])
}

impl View {
    pub fn headline(&self, short: bool) -> String {
        let w = &self.windows["24h"];
        if w.busy_seconds < 60.0 {
            return "no agent worked in the last 24 h".into();
        }
        let separator = if short { " · " } else { ": " };
        format!(
            "{:.1}× in parallel in the last 24 h{separator}{} of {}work in {}",
            w.counts.agent_seconds / w.busy_seconds,
            duration(w.counts.agent_seconds),
            if short { "" } else { "agent " },
            duration(w.busy_seconds)
        )
    }
    pub fn columns(&self) -> [[String; 9]; 3] {
        ["24h", "7d", "total"].map(|key| {
            let w = &self.windows[key];
            let c = &w.counts;
            [
                count(c.agents),
                count(c.prompts),
                count(c.turns),
                duration(c.agent_seconds),
                count(w.most_at_once),
                duration(c.waited_seconds),
                tokens(c.tokens.out),
                tokens(c.tokens.input.saturating_add(c.tokens.cache_write)),
                tokens(c.tokens.cache_read),
            ]
        })
    }
    pub fn levels(&self, weeks: usize) -> Vec<Option<u8>> {
        let shown = weeks.min(WEEKS) * 7;
        let days = usize::try_from(self.days.saturating_add(1)).unwrap_or(usize::MAX).min(shown);
        let mut positive: Vec<f64> =
            self.blocks.iter().take(days).map(|b| b.agent_seconds).filter(|n| *n > 0.0).collect();
        positive.sort_by(f64::total_cmp);
        (0..shown)
            .map(|i| {
                if i >= days {
                    return None;
                }
                let n = self.blocks[i].agent_seconds;
                if n <= 0.0 || positive.is_empty() {
                    return Some(0);
                }
                let thresholds = [1, 2, 3].map(|q| positive[(positive.len() * q).div_ceil(4).saturating_sub(1)]);
                Some(1 + u8::try_from(thresholds.iter().filter(|&&t| n > t).count()).unwrap_or(3))
            })
            .collect()
    }
    pub fn text(&self) -> String {
        let mut out = self.headline(false);
        out.push_str("\n\n");
        let _ = writeln!(out, "{:18} {:>10} {:>10} {:>10}", "", "24 h", "7 days", "total");
        let c = self.columns();
        for (i, label) in LABELS.iter().enumerate() {
            let _ = writeln!(out, "{label:18} {:>10} {:>10} {:>10}", c[0][i], c[1][i], c[2][i]);
        }
        out.push('\n');
        let levels = self.levels(WEEKS);
        for row in 0..7 {
            for week in (0..WEEKS).rev() {
                out.push(match levels[week * 7 + (6 - row)] {
                    None => ' ',
                    Some(0) => '·',
                    Some(1) => '░',
                    Some(2) => '▒',
                    Some(3) => '▓',
                    _ => '█',
                });
            }
            out.push('\n');
        }
        let _ =
            write!(out, "last 26 weeks, one block per 24 h, the newest bottom right · counting for {} days", self.days);
        out
    }
    pub fn share_link(&self) -> String {
        let mut link = format!("{SHARE_BASE}#v1");
        for (key, name) in [("24h", "24h"), ("7d", "7d"), ("total", "all")] {
            let w = &self.windows[key];
            let c = &w.counts;
            let _ = write!(
                link,
                "&{name}={}.{}.{}.{}.{}.{}",
                c.agents,
                c.prompts,
                whole_seconds(c.agent_seconds),
                whole_seconds(w.busy_seconds),
                w.most_at_once,
                c.tokens.out
            );
        }
        let levels: Vec<u8> = self.levels(WEEKS).into_iter().flatten().collect();
        let mut bytes = vec![0_u8; (levels.len() * 3).div_ceil(8)];
        for (i, level) in levels.into_iter().enumerate() {
            for bit in 0..3 {
                if level & (1 << (2 - bit)) != 0 {
                    let at = i * 3 + bit;
                    bytes[at / 8] |= 1 << (7 - at % 8);
                }
            }
        }
        let _ =
            write!(link, "&days={}&map={}", self.days, base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes));
        link
    }
}
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "nonnegative seconds are bounded to u64")]
fn whole_seconds(n: f64) -> u64 {
    n.max(0.0) as u64
}
pub fn share_link(stats: &Stats, now: SystemTime) -> String {
    stats.view(now).share_link()
}

#[cfg(test)]
pub(crate) mod tests;
