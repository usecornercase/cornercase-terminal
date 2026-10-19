use std::ffi::OsStr;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::expand_home;
use crate::log;

const RATE: u16 = 22_050;
const VOLUME: f32 = 0.3;
const PER_NOTE: u16 = 3_087;
const GAP: Duration = Duration::from_secs(1);
const KILL_AFTER: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(50);
const FOLDER: &str = "sounds";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Kind {
    NeedsYou,
    Finished,
}

impl Kind {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::NeedsYou => "needs-you",
            Self::Finished => "finished",
        }
    }

    fn notes(self) -> [f32; 2] {
        match self {
            Self::NeedsYou => [880.0, 1318.5],
            Self::Finished => [1046.5, 784.0],
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::NeedsYou => 1,
            Self::Finished => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cue {
    pub(crate) kind: Kind,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Os {
    Macos,
    Linux,
}

impl Os {
    pub(super) fn current() -> Self {
        if cfg!(target_os = "macos") { Self::Macos } else { Self::Linux }
    }

    fn players(self) -> &'static [&'static str] {
        match self {
            Self::Macos => &["afplay"],
            Self::Linux => &["pw-play", "paplay", "aplay"],
        }
    }
}

fn player(os: Os, lookup: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    os.players().iter().find_map(|name| lookup(name))
}

pub(super) fn on_path(path: Option<&OsStr>) -> impl Fn(&str) -> Option<PathBuf> + '_ {
    move |name| {
        std::env::split_paths(path?)
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(name))
            .find(|candidate| executable(candidate))
    }
}

pub(super) fn executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn resolve(path: &str, home: Option<&Path>) -> Option<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let path = expand_home(path, home);
    if !path.is_absolute() {
        return None;
    }
    let readable = path.metadata().is_ok_and(|m| m.is_file()) && File::open(&path).is_ok();
    readable.then_some(path)
}

pub(crate) fn loudest(cues: &[Cue]) -> Option<Cue> {
    cues.iter()
        .fold(None, |best: Option<&Cue>, cue| match best {
            Some(best) if best.kind.rank() >= cue.kind.rank() => Some(best),
            _ => Some(cue),
        })
        .cloned()
}

pub(crate) fn local_machine(var: impl Fn(&str) -> Option<String>) -> bool {
    !["SSH_CONNECTION", "SSH_TTY"].iter().any(|name| var(name).is_some_and(|value| !value.is_empty()))
}

#[expect(clippy::cast_possible_truncation, reason = "samples stay within ±VOLUME of i16::MAX")]
fn wav(kind: Kind) -> Vec<u8> {
    let rate = f32::from(RATE);
    let note = f32::from(PER_NOTE) / rate;
    let mut samples = Vec::with_capacity(usize::from(PER_NOTE) * 2);
    for frequency in kind.notes() {
        for i in 0..PER_NOTE {
            let t = f32::from(i) / rate;
            let attack = (t / 0.005).min(1.0);
            let release = ((note - t) / 0.01).clamp(0.0, 1.0);
            let decay = (-t * 12.0).exp();
            let wave = (2.0 * std::f32::consts::PI * frequency * t).sin();
            samples.push((wave * attack * release * decay * VOLUME * f32::from(i16::MAX)) as i16);
        }
    }
    encode(&samples)
}

fn encode(samples: &[i16]) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&u32::from(RATE).to_le_bytes());
    out.extend_from_slice(&(u32::from(RATE) * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn builtin(dir: &Path, kind: Kind) -> io::Result<PathBuf> {
    let folder = dir.join(FOLDER);
    DirBuilder::new().recursive(true).mode(0o700).create(&folder)?;
    let path = folder.join(format!("{}.wav", kind.id()));
    let bytes = wav(kind);
    if std::fs::read(&path).is_ok_and(|old| old == bytes) {
        return Ok(path);
    }
    let tmp = folder.join(format!("{}.wav.{}.tmp", kind.id(), std::process::id()));
    let mut file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(&bytes)?;
    std::fs::rename(tmp, &path)?;
    Ok(path)
}

#[derive(Debug)]
pub(crate) struct Player {
    program: Option<PathBuf>,
    dir: PathBuf,
    home: Option<PathBuf>,
    started: Option<Instant>,
}

impl Player {
    pub(crate) fn new(program: Option<PathBuf>, dir: PathBuf, home: Option<PathBuf>) -> Self {
        Self { program, dir, home, started: None }
    }

    pub(crate) fn detect(dir: PathBuf) -> Self {
        let path = std::env::var_os("PATH");
        let program = player(Os::current(), on_path(path.as_deref()));
        Self::new(program, dir, std::env::var_os("HOME").map(PathBuf::from))
    }

    pub(crate) fn play(&mut self, cue: &Cue, now: Instant) -> bool {
        let Some(program) = self.program.clone() else {
            return false;
        };
        if self.started.is_some_and(|started| now.saturating_duration_since(started) < GAP) {
            return false;
        }
        self.started = Some(now);
        let (cue, dir, home) = (cue.clone(), self.dir.clone(), self.home.clone());
        thread::spawn(move || sound(&program, &cue, &dir, home.as_deref()));
        true
    }
}

fn sound(program: &Path, cue: &Cue, dir: &Path, home: Option<&Path>) {
    let file = match resolve(&cue.path, home) {
        Some(file) => file,
        None => match builtin(dir, cue.kind) {
            Ok(file) => file,
            Err(err) => {
                log::warning!("sound", "cannot write the built-in sound", reason = err);
                return;
            }
        },
    };
    match spawn(program, &file) {
        Ok(child) => reap(child),
        Err(err) => log::warning!("sound", "cannot play a sound", program = program.display(), reason = err),
    }
}

fn spawn(program: &Path, file: &Path) -> io::Result<Child> {
    let mut command = Command::new(program);
    if program.file_name() == Some(OsStr::new("aplay")) {
        command.arg("-q");
    }
    command.arg(file).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
}

fn reap(mut child: Child) {
    let deadline = Instant::now() + KILL_AFTER;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(None) => thread::sleep(POLL),
            _ => return,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests;
