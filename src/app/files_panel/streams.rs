use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use super::FilesEvent;
use super::feed::{self, Fed};
use crate::app::control::{json, none};
use crate::app::events::Streamed;
use crate::app::{App, AppEvent};
use crate::control;
use crate::control::contract::Refusal;
use crate::control::files::{Line, Listing};
use crate::error::Result;
use crate::files::disk::Stamp;
use crate::files::{CHECK_EVERY, GIVE_UP_AFTER, LIST_EVERY};
use crate::log::{self, Job, Level};
use crate::project::Hierarchy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum Watch {
    Folders(Vec<String>),
    File(String),
}

impl Watch {
    fn name(&self) -> &'static str {
        match self {
            Self::Folders(_) => "list",
            Self::File(_) => "read",
        }
    }
}

pub(in crate::app) struct Opened {
    pub(in crate::app) client: u64,
    pub(in crate::app) key: u64,
    pub(in crate::app) follow: bool,
    pub(in crate::app) workspace: Option<u64>,
    pub(in crate::app) watch: Watch,
}

struct Stream {
    id: u64,
    client: u64,
    key: Option<u64>,
    follow: bool,
    workspace: Option<u64>,
    watch: Watch,
    revision: u64,
    last: Option<Listing>,
    stamp: Option<Stamp>,
    due: Instant,
    running: Option<(u64, Instant)>,
}

#[derive(Default)]
pub(in crate::app) struct Streams {
    open: Vec<Stream>,
    issued: u64,
    generation: u64,
    every: Option<Duration>,
    searches: Vec<(u64, u64, Arc<AtomicBool>)>,
}

enum Sent {
    First(u64, Value),
    Later(String),
}

impl Streams {
    pub(in crate::app) fn follows(&self, client: u64) -> bool {
        self.open.iter().any(|s| s.client == client && s.follow)
    }

    pub(in crate::app) fn search(&mut self, client: u64, key: u64) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.searches.push((client, key, Arc::clone(&cancel)));
        cancel
    }

    pub(in crate::app) fn searched(&mut self, key: u64) {
        self.searches.retain(|(_, searching, _)| *searching != key);
    }

    #[cfg(test)]
    pub(in crate::app) fn set_every(&mut self, every: Duration) {
        self.every = Some(every);
    }

    #[cfg(test)]
    pub(in crate::app) fn is_empty(&self) -> bool {
        self.open.is_empty() && self.searches.is_empty()
    }

    fn every(&self, watch: &Watch) -> Duration {
        self.every.unwrap_or(match watch {
            Watch::Folders(_) => LIST_EVERY,
            Watch::File(_) => CHECK_EVERY,
        })
    }

    fn find(&mut self, id: u64) -> Option<&mut Stream> {
        self.open.iter_mut().find(|s| s.id == id)
    }

    fn start(&mut self, opened: &Opened, now: Instant) -> u64 {
        self.issued += 1;
        self.open.push(Stream {
            id: self.issued,
            client: opened.client,
            key: Some(opened.key),
            follow: opened.follow,
            workspace: opened.workspace,
            watch: opened.watch.clone(),
            revision: 0,
            last: None,
            stamp: None,
            due: now,
            running: None,
        });
        self.issued
    }

    fn begin(&mut self, id: u64, now: Instant) -> Option<(u64, Option<Stamp>)> {
        self.generation += 1;
        let generation = self.generation;
        let stream = self.find(id)?;
        stream.running = Some((generation, now));
        Some((generation, stream.stamp))
    }

    pub(in crate::app) fn wake(&self, now: Instant) -> Option<Duration> {
        let next = |s: &Stream| match s.running {
            Some((_, since)) => (since + GIVE_UP_AFTER).saturating_duration_since(now),
            None => s.due.saturating_duration_since(now),
        };
        self.open.iter().map(next).min()
    }
}

impl Stream {
    fn sent<T: Serialize>(&mut self, content: T) -> Option<Sent> {
        self.revision += 1;
        let line = Line {
            boot: control::server_token().to_string(),
            stream: self.follow.then_some(self.id),
            revision: self.revision,
            content,
        };
        match self.key.take() {
            Some(key) => Some(Sent::First(key, json(&line))),
            None => line.streamed().map(Sent::Later),
        }
    }
}

impl App {
    pub(in crate::app) fn start_file_stream(&mut self, opened: &Opened, now: Instant) {
        let id = self.clients.file_streams.start(opened, now);
        log::info!(
            "control",
            "files",
            client = opened.client,
            stream = id,
            watch = opened.watch.name(),
            follow = opened.follow
        );
        self.spawn_file_feed(id, now);
    }

    pub(in crate::app) fn forget_file_streams(&mut self, client: u64) {
        let streams = &mut self.clients.file_streams;
        streams.open.retain(|s| s.client != client);
        streams.searches.retain(|(searching, _, cancel)| {
            let ours = *searching == client;
            if ours {
                cancel.store(true, Ordering::Relaxed);
            }
            !ours
        });
    }

    fn file_root(&self, workspace: Option<u64>) -> Option<std::path::PathBuf> {
        Some(self.projects.workspace_by(workspace?)?.path.clone())
    }

    fn spawn_file_feed(&mut self, id: u64, now: Instant) {
        let Some((generation, stamp)) = self.clients.file_streams.begin(id, now) else { return };
        let Some(stream) = self.clients.file_streams.open.iter().find(|s| s.id == id) else { return };
        let (workspace, watch) = (stream.workspace, stream.watch.clone());
        let (root, home) = (self.file_root(workspace), self.home.clone());
        let job = Job::new(Level::Debug, "files", watch.name()).with("stream", id);
        self.spawn_job(
            job,
            move || feed_of(workspace, &watch, root.as_deref(), home.as_deref(), stamp),
            move |fed, _| {
                let fed = fed.map_err(|e| e.to_string());
                Some(AppEvent::Files(FilesEvent::Fed { stream: id, generation, fed }))
            },
        );
    }

    pub(in crate::app) fn file_fed(&mut self, stream: u64, generation: u64, fed: std::result::Result<Fed, String>) {
        let now = Instant::now();
        let streams = &mut self.clients.file_streams;
        let every = streams.open.iter().find(|s| s.id == stream).map(|s| streams.every(&s.watch));
        let Some((s, every)) = streams.find(stream).zip(every) else { return };
        if s.running.map(|(running, _)| running) != Some(generation) {
            return;
        }
        s.running = None;
        s.due = now + every;
        let (client, follow, first) = (s.client, s.follow, s.key);
        let sent = match fed {
            Ok(Fed::Listing(listing)) => {
                let sent = (s.last.as_ref() != Some(&listing)).then(|| s.sent(&listing)).flatten();
                s.last = Some(listing);
                sent
            }
            Ok(Fed::Read(Some(read))) => {
                let (stamp, read) = *read;
                s.stamp = Some(stamp);
                s.sent(&read)
            }
            Ok(Fed::Read(None)) => None,
            Err(e) => {
                self.file_feed_failed(stream, first, &e);
                return;
            }
        };
        if let Some(sent) = sent {
            self.file_sent(stream, client, sent);
        }
        if !follow {
            self.clients.file_streams.open.retain(|s| s.id != stream);
        }
    }

    fn file_sent(&mut self, stream: u64, client: u64, sent: Sent) {
        match sent {
            Sent::First(key, value) => self.files_replied(key, Ok(value)),
            Sent::Later(text) => {
                log::debug!("control", "files", stream = stream, bytes = text.len());
                self.clients.events.push(client, Streamed::Snapshot(text));
            }
        }
    }

    fn file_feed_failed(&mut self, stream: u64, first: Option<u64>, error: &str) {
        let Some(key) = first else {
            log::debug!("control", "files failed", stream = stream, error = error);
            return;
        };
        self.clients.file_streams.open.retain(|s| s.id != stream);
        self.files_replied(key, Err(Refusal::from(format!("cannot read the files: {error}"))));
    }

    pub(in crate::app) fn files_replied(&mut self, key: u64, reply: std::result::Result<Value, Refusal>) {
        self.clients.file_streams.searched(key);
        self.answer_read(key, |_| reply);
    }

    pub(in crate::app) fn refresh_file_streams(&mut self, now: Instant) {
        self.end_gone_file_streams();
        let due: Vec<u64> = self
            .clients
            .file_streams
            .open
            .iter()
            .filter(|s| s.running.map_or(s.due <= now, |(_, since)| now.duration_since(since) >= GIVE_UP_AFTER))
            .map(|s| s.id)
            .collect();
        for id in due {
            self.spawn_file_feed(id, now);
        }
    }

    fn end_gone_file_streams(&mut self) {
        let gone = |app: &Self, s: &Stream| s.workspace.is_some_and(|w| !app.workspace_usable(w));
        let ended: Vec<Stream> = {
            let (kept, ended) =
                std::mem::take(&mut self.clients.file_streams.open).into_iter().partition(|s| !gone(self, s));
            self.clients.file_streams.open = kept;
            ended
        };
        for stream in ended {
            log::info!("control", "files ended", client = stream.client, stream = stream.id, reason = "workspace gone");
            match (stream.key, stream.workspace) {
                (Some(key), Some(workspace)) => self.files_replied(key, Err(none("workspace", workspace))),
                _ => self.clients.events.push(stream.client, Streamed::End),
            }
        }
    }
}

fn feed_of(
    workspace: Option<u64>,
    watch: &Watch,
    root: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
    stamp: Option<Stamp>,
) -> Result<Fed> {
    let targets = |asked: &[String]| -> Result<Vec<feed::Target>> {
        asked
            .iter()
            .map(|path| feed::target(path, root, home).map_err(|r| crate::error::Error::Control(r.message)))
            .collect()
    };
    Ok(match watch {
        Watch::Folders(folders) => Fed::Listing(feed::list_folders(workspace, &targets(folders)?, root)),
        Watch::File(path) => {
            let target = feed::target(path, root, home).map_err(|r| crate::error::Error::Control(r.message))?;
            Fed::Read(feed::read_target(workspace, &target, stamp))
        }
    })
}
