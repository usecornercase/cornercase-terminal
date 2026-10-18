use std::fmt::{Display, Write as _};
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::time::Stamp;

pub(crate) const LEVEL_ENV: &str = "CORNERCASE_LOG";
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const SLOW: Duration = Duration::from_secs(1);
const QUEUE: usize = 4096;
pub(crate) const PRIVATE: u32 = 0o600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" | "trace" => Some(Self::Debug),
            _ => None,
        }
    }
}

struct Logger {
    level: Level,
    lines: SyncSender<String>,
    dropped: AtomicU64,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

pub(crate) fn rotated(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".1");
    name.into()
}

pub(crate) fn level_from_env() -> Level {
    std::env::var(LEVEL_ENV).ok().and_then(|text| Level::parse(&text)).unwrap_or(Level::Info)
}

pub(crate) fn start(path: &Path, level: Level) -> io::Result<()> {
    let mut sink = Sink::open(path.to_path_buf(), MAX_BYTES, true)?;
    let (lines, rx) = mpsc::sync_channel(QUEUE);
    thread::Builder::new().name("log".into()).spawn(move || sink.drain(&rx))?;
    let _ = LOGGER.set(Logger { level, lines, dropped: AtomicU64::new(0) });
    Ok(())
}

pub(crate) fn enabled(level: Level) -> bool {
    LOGGER.get().is_some_and(|logger| level <= logger.level)
}

pub(crate) fn write(level: Level, target: &str, message: &str, fields: &[(&str, &dyn Display)]) {
    let Some(logger) = LOGGER.get().filter(|logger| level <= logger.level) else { return };
    if logger.lines.try_send(line(SystemTime::now(), level, target, message, fields)).is_err() {
        logger.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn write_fields(level: Level, target: &str, message: &str, fields: &[(&str, String)]) {
    if enabled(level) {
        let fields: Vec<(&str, &dyn Display)> =
            fields.iter().map(|(key, value)| (*key, value as &dyn Display)).collect();
        write(level, target, message, &fields);
    }
}

pub(crate) fn line(
    at: SystemTime,
    level: Level,
    target: &str,
    message: &str,
    fields: &[(&str, &dyn Display)],
) -> String {
    let mut out = String::with_capacity(96);
    let _ = write!(out, "{} {:<5} {target}: {message}", Stamp(at), level.name());
    for (key, value) in fields {
        let _ = write!(out, " {key}=");
        quote(&mut out, &value.to_string());
    }
    out.push('\n');
    out
}

fn quote(out: &mut String, value: &str) {
    let plain = !value.is_empty() && value.chars().all(|c| !(c.is_whitespace() || c.is_control() || c == '"'));
    if plain {
        out.push_str(value);
    } else {
        let _ = write!(out, "{value:?}");
    }
}

pub(crate) struct Job {
    level: Level,
    target: &'static str,
    what: &'static str,
    fields: Vec<(&'static str, String)>,
    failed: Option<String>,
    started: Instant,
    slow: Duration,
}

impl Job {
    pub(crate) fn new(level: Level, target: &'static str, what: &'static str) -> Self {
        Self { level, target, what, fields: Vec::new(), failed: None, started: Instant::now(), slow: SLOW }
    }

    #[must_use]
    pub(crate) fn slow_after(self, slow: Duration) -> Self {
        Self { slow, ..self }
    }

    #[must_use]
    pub(crate) fn with(mut self, key: &'static str, value: impl Display) -> Self {
        if LOGGER.get().is_some() {
            self.fields.push((key, value.to_string()));
        }
        self
    }

    pub(crate) fn note(&mut self, key: &'static str, value: impl Display) {
        if LOGGER.get().is_some() {
            self.fields.push((key, value.to_string()));
        }
    }

    pub(crate) fn fail(&mut self, error: impl Display) {
        if LOGGER.get().is_some() {
            self.failed = Some(error.to_string());
        }
    }

    #[must_use]
    pub(crate) fn begin(self) -> Self {
        self.log(self.level, "started", &[]);
        Self { started: Instant::now(), ..self }
    }

    pub(crate) fn done(self) {
        self.end(None);
    }

    pub(crate) fn finish<T, E: Display>(self, result: &Result<T, E>) {
        match (result, self.failed.clone()) {
            (Err(e), _) => self.end(Some(e)),
            (Ok(_), Some(failed)) => self.end(Some(&failed)),
            (Ok(_), None) => self.end(None),
        }
    }

    fn end(self, error: Option<&dyn Display>) {
        let took = self.started.elapsed();
        let (ms, slow) = (took.as_millis(), took >= self.slow);
        let level = match (self.level, error, slow) {
            (Level::Info, Some(_), _) => Level::Warn,
            (Level::Debug, _, true) => Level::Info,
            (level, ..) => level,
        };
        match error {
            Some(e) => self.log(level, "failed", &[("ms", &ms), ("error", e)]),
            None => self.log(level, "done", &[("ms", &ms)]),
        }
    }

    fn log(&self, level: Level, how: &str, extra: &[(&str, &dyn Display)]) {
        if !enabled(level) {
            return;
        }
        let mut fields: Vec<(&str, &dyn Display)> = extra.to_vec();
        fields.extend(self.fields.iter().map(|(key, value)| (*key, value as &dyn Display)));
        write(level, self.target, &format!("{} {how}", self.what), &fields);
    }
}

struct Sink {
    path: PathBuf,
    file: File,
    max: u64,
    stderr: bool,
}

impl Sink {
    fn open(path: PathBuf, max: u64, stderr: bool) -> io::Result<Self> {
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        match std::fs::set_permissions(rotated(&path), Permissions::from_mode(PRIVATE)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let file = OpenOptions::new().create(true).append(true).mode(PRIVATE).open(&path)?;
        file.set_permissions(Permissions::from_mode(PRIVATE))?;
        if stderr {
            rustix::stdio::dup2_stderr(&file)?;
        }
        Ok(Self { path, file, max, stderr })
    }

    fn drain(&mut self, rx: &Receiver<String>) {
        for line in rx {
            self.write(&line);
            let dropped = LOGGER.get().map_or(0, |logger| logger.dropped.swap(0, Ordering::Relaxed));
            if dropped > 0 {
                let note = format!("{dropped} lines were dropped: the log could not keep up");
                self.write(&note_line(Level::Warn, &note));
            }
        }
    }

    fn write(&mut self, line: &str) {
        let size = self.file.metadata().map_or(0, |meta| meta.len());
        if size > 0 && size + line.len() as u64 > self.max {
            self.rotate();
        }
        let _ = self.file.write_all(line.as_bytes());
    }

    fn rotate(&mut self) {
        if let Err(e) = std::fs::rename(&self.path, rotated(&self.path)) {
            let note =
                format!("could not keep the old log in {}, so it starts over: {e}", rotated(&self.path).display());
            let _ = self.file.set_len(0);
            let _ = self.file.write_all(note_line(Level::Error, &note).as_bytes());
            return;
        }
        match Self::open(self.path.clone(), self.max, self.stderr) {
            Ok(sink) => *self = sink,
            Err(e) => {
                let note = format!("could not start a new log: {e}");
                let _ = self.file.write_all(note_line(Level::Error, &note).as_bytes());
            }
        }
    }
}

fn note_line(level: Level, message: &str) -> String {
    line(SystemTime::now(), level, "log", message, &[])
}

pub(crate) fn tail(path: &Path, lines: usize) -> io::Result<String> {
    let current = std::fs::read(path)?;
    let mut text = String::from_utf8_lossy(&current).into_owned();
    if text.lines().count() < lines
        && let Ok(older) = std::fs::read(rotated(path))
    {
        text = String::from_utf8_lossy(&older).into_owned() + &text;
    }
    let skip = text.lines().count().saturating_sub(lines);
    Ok(text.lines().skip(skip).flat_map(|line| [line, "\n"]).collect())
}

pub(crate) struct Follower {
    path: PathBuf,
    file: File,
    inode: u64,
}

impl Follower {
    pub(crate) fn at_end(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let inode = file.metadata()?.ino();
        file.seek(SeekFrom::End(0))?;
        Ok(Self { path: path.to_path_buf(), file, inode })
    }

    pub(crate) fn read(&mut self) -> io::Result<Vec<u8>> {
        let mut new = Vec::new();
        self.file.read_to_end(&mut new)?;
        let Ok(meta) = std::fs::metadata(&self.path) else { return Ok(new) };
        let read_to = self.file.stream_position()?;
        if meta.ino() != self.inode || meta.len() < read_to {
            self.file = File::open(&self.path)?;
            self.inode = meta.ino();
            self.file.read_to_end(&mut new)?;
        }
        Ok(new)
    }
}

macro_rules! event {
    ($level:expr, $target:expr, $message:expr $(, $key:ident = $value:expr)* $(,)?) => {
        if $crate::log::enabled($level) {
            $crate::log::write(
                $level,
                $target,
                $message,
                &[$((stringify!($key), &$value as &dyn ::std::fmt::Display)),*],
            );
        }
    };
}

macro_rules! error {
    ($($args:tt)*) => { $crate::log::event!($crate::log::Level::Error, $($args)*) };
}

macro_rules! warning {
    ($($args:tt)*) => { $crate::log::event!($crate::log::Level::Warn, $($args)*) };
}

macro_rules! info {
    ($($args:tt)*) => { $crate::log::event!($crate::log::Level::Info, $($args)*) };
}

macro_rules! debug {
    ($($args:tt)*) => { $crate::log::event!($crate::log::Level::Debug, $($args)*) };
}

pub(crate) use {debug, error, event, info, warning};

#[cfg(test)]
mod tests;
