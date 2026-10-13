use std::fmt::{self, Display, Write as _};
use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::protocol;
use crate::state;

pub const LEVEL_ENV: &str = "CORNERCASE_LOG";
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const SLOW: Duration = Duration::from_secs(1);
const QUEUE: usize = 4096;
const FILE: &str = "server.log";
pub const PRIVATE: u32 = 0o600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
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

    pub fn parse(text: &str) -> Option<Self> {
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

pub fn path() -> PathBuf {
    match std::env::var_os(protocol::SOCKET_ENV) {
        Some(socket) => Path::new(&socket).with_extension("log"),
        None => state::path().with_file_name(FILE),
    }
}

pub fn rotated(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".1");
    name.into()
}

pub fn level_from_env() -> Level {
    std::env::var(LEVEL_ENV).ok().and_then(|text| Level::parse(&text)).unwrap_or(Level::Info)
}

pub fn start(path: &Path, level: Level) -> io::Result<()> {
    let mut sink = Sink::open(path.to_path_buf(), MAX_BYTES, true)?;
    let (lines, rx) = mpsc::sync_channel(QUEUE);
    thread::Builder::new().name("log".into()).spawn(move || sink.drain(&rx))?;
    let _ = LOGGER.set(Logger { level, lines, dropped: AtomicU64::new(0) });
    Ok(())
}

pub fn enabled(level: Level) -> bool {
    LOGGER.get().is_some_and(|logger| level <= logger.level)
}

pub fn write(level: Level, target: &str, message: &str, fields: &[(&str, &dyn Display)]) {
    let Some(logger) = LOGGER.get().filter(|logger| level <= logger.level) else { return };
    if logger.lines.try_send(line(SystemTime::now(), level, target, message, fields)).is_err() {
        logger.dropped.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn write_fields(level: Level, target: &str, message: &str, fields: &[(&str, String)]) {
    if enabled(level) {
        let fields: Vec<(&str, &dyn Display)> =
            fields.iter().map(|(key, value)| (*key, value as &dyn Display)).collect();
        write(level, target, message, &fields);
    }
}

pub fn line(at: SystemTime, level: Level, target: &str, message: &str, fields: &[(&str, &dyn Display)]) -> String {
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

pub struct Stamp(pub SystemTime);

impl Stamp {
    pub fn parse(text: &str) -> Option<SystemTime> {
        let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
        let [year, month, day] = numbers(date, '-')?;
        let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
        let [hour, minute, second] = numbers(clock, ':')?;
        let valid = (1..=12).contains(&month) && (1..=31).contains(&day) && hour < 24 && minute < 60 && second < 60;
        if !valid || fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let nanos = format!("{fraction:0<9}").parse().ok()?;
        let secs = days(year, month, day)? * 86_400 + hour * 3600 + minute * 60 + second;
        UNIX_EPOCH.checked_add(Duration::new(secs, nanos))
    }
}

fn numbers(text: &str, separator: char) -> Option<[u64; 3]> {
    let mut parts = text.split(separator).map(|part| part.bytes().all(|b| b.is_ascii_digit()).then(|| part.parse()));
    let found = [parts.next()??.ok()?, parts.next()??.ok()?, parts.next()??.ok()?];
    parts.next().is_none().then_some(found)
}

fn days(year: u64, month: u64, day: u64) -> Option<u64> {
    let year = year.checked_sub(u64::from(month <= 2))?;
    let era = year / 400;
    let yoe = year % 400;
    let doy = (153 * if month > 2 { month - 3 } else { month + 9 } + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe).checked_sub(719_468)
}

impl Display for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let since = self.0.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = since.as_secs();
        let (year, month, day) = civil(secs / 86_400);
        let (hour, minute, second) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
        let millis = since.subsec_millis();
        write!(f, "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
    }
}

fn civil(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

pub struct Job {
    level: Level,
    target: &'static str,
    what: &'static str,
    fields: Vec<(&'static str, String)>,
    started: Instant,
}

impl Job {
    pub fn new(level: Level, target: &'static str, what: &'static str) -> Self {
        Self { level, target, what, fields: Vec::new(), started: Instant::now() }
    }

    #[must_use]
    pub fn with(mut self, key: &'static str, value: impl Display) -> Self {
        if LOGGER.get().is_some() {
            self.fields.push((key, value.to_string()));
        }
        self
    }

    #[must_use]
    pub fn begin(self) -> Self {
        self.log(self.level, "started", &[]);
        Self { started: Instant::now(), ..self }
    }

    pub fn done(self) {
        self.end(None);
    }

    pub fn finish<T, E: Display>(self, result: &Result<T, E>) {
        match result {
            Ok(_) => self.end(None),
            Err(e) => self.end(Some(e)),
        }
    }

    fn end(self, error: Option<&dyn Display>) {
        let took = self.started.elapsed();
        let (ms, slow) = (took.as_millis(), took >= SLOW);
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

pub fn tail(path: &Path, lines: usize) -> io::Result<String> {
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

pub struct Follower {
    path: PathBuf,
    file: File,
    inode: u64,
}

impl Follower {
    pub fn at_end(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let inode = file.metadata()?.ino();
        file.seek(SeekFrom::End(0))?;
        Ok(Self { path: path.to_path_buf(), file, inode })
    }

    pub fn read(&mut self) -> io::Result<Vec<u8>> {
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
