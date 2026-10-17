use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rustix::fs::{FlockOperation, flock};
use rustix::io::Errno;
use rustix::process::Signal;

use super::registry::{Entry, Plugin, Registry, Watch};
use super::run::{Places, private_dir};
use crate::log;
use crate::panics;
use crate::process;

const CHECK_EVERY: Duration = Duration::from_millis(250);
const LOCK_FILE: &str = "running.lock";
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const STOP_WITHIN: Duration = Duration::from_secs(2);
const RUNNING_DIR: &str = "running";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Service,
    Window,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Window => "window process",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    plugin: String,
    entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Spec {
    plugin: Plugin,
    command: Vec<String>,
}

struct Running {
    spec: Spec,
    child: Child,
    since: Instant,
}

struct Waiting {
    failures: u32,
    at: Instant,
}

pub(crate) type Notes = Arc<Mutex<BTreeMap<String, String>>>;

pub(crate) struct Supervisor {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    notes: Notes,
    #[cfg_attr(not(test), expect(dead_code, reason = "tests wait until the supervisor has looked"))]
    steps: Arc<AtomicUsize>,
}

impl Supervisor {
    pub(crate) fn start(kind: Kind, places: Places, env: Vec<(String, String)>) -> Self {
        let (tx, rx) = mpsc::channel();
        let notes = Notes::default();
        let shared = notes.clone();
        let steps = Arc::new(AtomicUsize::new(0));
        let stepped = steps.clone();
        let thread = thread::spawn(move || {
            let mut state =
                State { kind, places, env, notes: shared, running: BTreeMap::new(), waiting: BTreeMap::new() };
            let mut watch = Watch::new(state.places.registry.clone());
            let mut wanted = BTreeMap::new();
            loop {
                if let Some(plugins) = watch.changed() {
                    wanted = state.wanted(&plugins);
                }
                let _ = panics::contain(|| state.step(&wanted, Instant::now()));
                stepped.fetch_add(1, Ordering::SeqCst);
                match rx.recv_timeout(CHECK_EVERY) {
                    Err(RecvTimeoutError::Timeout) => {}
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            state.stop_all();
        });
        Self { stop: Some(tx), thread: Some(thread), notes, steps }
    }

    #[cfg(test)]
    fn steps(&self) -> usize {
        self.steps.load(Ordering::SeqCst)
    }

    pub(crate) fn note(&self, plugin: &str) -> Option<String> {
        self.notes.lock().get(plugin).cloned()
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct State {
    kind: Kind,
    places: Places,
    env: Vec<(String, String)>,
    notes: Notes,
    running: BTreeMap<Key, Running>,
    waiting: BTreeMap<Key, Waiting>,
}

impl State {
    fn wanted(&self, plugins: &[Plugin]) -> BTreeMap<Key, Spec> {
        let mut wanted = BTreeMap::new();
        for plugin in plugins {
            let Some(manifest) = plugin.runs() else { continue };
            let entries = match self.kind {
                Kind::Service => &manifest.service,
                Kind::Window => &manifest.window,
            };
            for entry in entries {
                let key = Key { plugin: plugin.entry.id.clone(), entry: entry.id.clone() };
                wanted.insert(key, Spec { plugin: plugin.clone(), command: entry.command.clone() });
            }
        }
        wanted
    }

    fn step(&mut self, wanted: &BTreeMap<Key, Spec>, now: Instant) {
        let gone: Vec<Key> = self
            .running
            .iter()
            .filter(|(key, run)| wanted.get(key) != Some(&run.spec))
            .map(|(k, _)| k.clone())
            .collect();
        for key in gone {
            if let Some(run) = self.running.remove(&key) {
                self.stop(&key, run);
            }
        }
        self.waiting.retain(|key, _| wanted.contains_key(key));
        self.reap(now);
        for (key, spec) in wanted {
            if self.running.contains_key(key) || self.waiting.get(key).is_some_and(|w| w.at > now) {
                continue;
            }
            self.spawn(key, spec, now);
        }
        self.publish(now);
    }

    fn spawn(&mut self, key: &Key, spec: &Spec, now: Instant) {
        let kind = self.kind.name();
        let lock = try_shared(&self.places, &key.plugin);
        let child = match &lock {
            Ok(Some(_)) if !on_disk(&self.places, &spec.plugin.entry) => return,
            Ok(Some(_)) if !spec.plugin.root().is_dir() => {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, "the plugin's folder is gone"))
            }
            Ok(Some(_)) => {
                self.places.command(&spec.plugin, &spec.command, &self.env, None).and_then(|mut c| c.spawn())
            }
            Ok(None) => return,
            Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
        };
        match child {
            Ok(child) => {
                log::info!("plugins", "started", kind = kind, plugin = key.plugin, entry = key.entry, pid = child.id());
                self.places.note(
                    &key.plugin,
                    &format!("{kind} started"),
                    &[("entry", &key.entry), ("pid", &child.id())],
                );
                let run = Running { spec: spec.clone(), child, since: now };
                match record(&self.places, &key.plugin, run.child.id(), &format!("{kind} {}", key.entry)) {
                    Ok(()) => {
                        self.running.insert(key.clone(), run);
                    }
                    Err(e) => {
                        log::warning!(
                            "plugins",
                            "cannot record a running process, stopping it",
                            kind = kind,
                            plugin = key.plugin,
                            entry = key.entry,
                            error = e
                        );
                        self.places.note(
                            &key.plugin,
                            &format!("{kind} stopped"),
                            &[("entry", &key.entry), ("error", &e)],
                        );
                        stop_children(&self.places, vec![run]);
                        self.retry(key, now, false);
                    }
                }
            }
            Err(e) => {
                log::warning!(
                    "plugins",
                    "cannot start",
                    kind = kind,
                    plugin = key.plugin,
                    entry = key.entry,
                    error = e
                );
                self.places.note(&key.plugin, &format!("{kind} cannot start"), &[("entry", &key.entry), ("error", &e)]);
                self.retry(key, now, false);
            }
        }
    }

    fn reap(&mut self, now: Instant) {
        let mut ended = Vec::new();
        for (key, run) in &mut self.running {
            match run.child.try_wait() {
                Ok(Some(status)) => ended.push((key.clone(), status.to_string(), now.duration_since(run.since))),
                Ok(None) => {}
                Err(e) => ended.push((key.clone(), e.to_string(), now.duration_since(run.since))),
            }
        }
        for (key, status, ran) in ended {
            if let Some(run) = self.running.remove(&key) {
                forget(&self.places, &key.plugin, run.child.id());
            }
            let kind = self.kind.name();
            log::warning!("plugins", "exited", kind = kind, plugin = key.plugin, entry = key.entry, status = status);
            self.places.note(&key.plugin, &format!("{kind} exited"), &[("entry", &key.entry), ("status", &status)]);
            self.retry(&key, now, ran >= HEALTHY_AFTER);
        }
    }

    fn retry(&mut self, key: &Key, now: Instant, healthy: bool) {
        let failures = if healthy { 1 } else { self.waiting.get(key).map_or(1, |w| w.failures + 1) };
        let wait = backoff(failures);
        self.waiting.insert(key.clone(), Waiting { failures, at: now + wait });
    }

    fn publish(&self, now: Instant) {
        let mut notes = BTreeMap::new();
        for key in self.running.keys() {
            notes.entry(key.plugin.clone()).or_insert_with(|| format!("{} {} runs", self.kind.name(), key.entry));
        }
        for (key, waiting) in &self.waiting {
            if self.running.contains_key(key) {
                continue;
            }
            let secs = waiting.at.saturating_duration_since(now).as_secs() + 1;
            let note = format!("{} {} stopped · starting again in {secs} s", self.kind.name(), key.entry);
            notes.insert(key.plugin.clone(), note);
        }
        *self.notes.lock() = notes;
    }

    fn stop(&self, key: &Key, run: Running) {
        stop_children(&self.places, vec![run]);
        let kind = self.kind.name();
        log::info!("plugins", "stopped", kind = kind, plugin = key.plugin, entry = key.entry);
        self.places.note(&key.plugin, &format!("{kind} stopped"), &[("entry", &key.entry)]);
    }

    fn stop_all(&mut self) {
        let running = std::mem::take(&mut self.running);
        for key in running.keys() {
            self.places.note(&key.plugin, &format!("{} stopped", self.kind.name()), &[("entry", &key.entry)]);
        }
        if !running.is_empty() {
            log::info!("plugins", "stopped every one", kind = self.kind.name(), count = running.len());
        }
        stop_children(&self.places, running.into_values().collect());
    }
}

pub(crate) fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1u64 << failures.saturating_sub(1).min(6)).min(MAX_BACKOFF)
}

fn lock_file(places: &Places, plugin: &str) -> std::io::Result<File> {
    let dir = places.state(plugin);
    private_dir(&dir)?;
    fs::OpenOptions::new().create(true).write(true).truncate(false).mode(0o600).open(dir.join(LOCK_FILE))
}

fn locked(file: File, operation: FlockOperation) -> std::io::Result<Option<File>> {
    match flock(&file, operation) {
        Ok(()) => Ok(Some(file)),
        Err(Errno::WOULDBLOCK) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn try_shared(places: &Places, plugin: &str) -> std::io::Result<Option<File>> {
    locked(lock_file(places, plugin)?, FlockOperation::NonBlockingLockShared)
}

pub(crate) fn exclude_spawns(places: &Places, plugin: &str, until: Instant) -> Result<(), String> {
    let file = lock_file(places, plugin).map_err(|e| format!("cannot open the lock of `{plugin}`: {e}"))?;
    loop {
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(()),
            Err(Errno::WOULDBLOCK) if Instant::now() < until => thread::sleep(Duration::from_millis(20)),
            Err(Errno::WOULDBLOCK) => return Err(format!("a supervisor is still starting `{plugin}`")),
            Err(e) => return Err(format!("cannot lock `{plugin}`: {e}")),
        }
    }
}

fn on_disk(places: &Places, entry: &Entry) -> bool {
    Registry::load(&places.registry).entry(&entry.id).is_ok_and(|current| current == entry)
}

fn running_dir(places: &Places, plugin: &str) -> std::path::PathBuf {
    places.state(plugin).join(RUNNING_DIR)
}

fn record(places: &Places, plugin: &str, pid: u32, what: &str) -> std::io::Result<()> {
    let dir = running_dir(places, plugin);
    private_dir(&dir)?;
    let mut file =
        fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(dir.join(pid.to_string()))?;
    file.write_all(what.as_bytes())
}

fn forget(places: &Places, plugin: &str, pid: u32) {
    let _ = fs::remove_file(running_dir(places, plugin).join(pid.to_string()));
}

pub(crate) fn running(places: &Places, plugin: &str) -> Result<Vec<i32>, String> {
    let dir = running_dir(places, plugin);
    let files = match fs::read_dir(&dir) {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", dir.display())),
    };
    let mut alive = Vec::new();
    for file in files {
        let file = file.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        match file.file_name().to_str().and_then(|name| name.parse::<i32>().ok()) {
            Some(pid) if process::alive(pid) => alive.push(pid),
            _ => {
                let _ = fs::remove_file(file.path());
            }
        }
    }
    alive.sort_unstable();
    Ok(alive)
}

fn stop_children(places: &Places, mut running: Vec<Running>) {
    let all: Vec<(String, u32)> =
        running.iter().map(|run| (run.spec.plugin.entry.id.clone(), run.child.id())).collect();
    stop_all_of(&mut running);
    for (plugin, pid) in all {
        forget(places, &plugin, pid);
    }
}

fn stop_all_of(running: &mut Vec<Running>) {
    for run in running.iter() {
        super::run::signal(&run.child, Signal::TERM);
    }
    let until = Instant::now() + STOP_WITHIN;
    while Instant::now() < until {
        running.retain_mut(|run| !matches!(run.child.try_wait(), Ok(Some(_)) | Err(_)));
        if running.is_empty() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    for mut run in std::mem::take(running) {
        super::run::signal(&run.child, Signal::KILL);
        let _ = run.child.wait();
    }
}

#[cfg(test)]
mod tests;
