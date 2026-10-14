use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{self, Config};

pub(crate) const OFF: &str = "off";
pub(crate) const ON: &str = "on";
pub(crate) const BUILTIN: &str = "builtin";
pub(crate) const FILE: &str = "a file…";

const RATE: u32 = 22_050;
const LEVEL: f32 = 0.35;
const POLL: Duration = Duration::from_millis(40);
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    Waiting,
    Done,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Done => "done",
        }
    }

    fn tones(self) -> &'static [Tone] {
        match self {
            Self::Waiting => &[Tone { hz: 880, ms: 80 }, Tone { hz: 0, ms: 36 }, Tone { hz: 1175, ms: 130 }],
            Self::Done => &[Tone { hz: 523, ms: 90 }, Tone { hz: 0, ms: 28 }, Tone { hz: 784, ms: 170 }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cue {
    pub(crate) kind: Kind,
    pub(crate) path: String,
}

struct Tone {
    hz: u32,
    ms: u32,
}

struct Request {
    kind: Kind,
    file: Option<PathBuf>,
}

struct Prepared {
    player: PathBuf,
    file: PathBuf,
    temp: bool,
}

struct Playing {
    child: Child,
    temp: Option<PathBuf>,
}

pub(crate) fn on(setting: &str) -> bool {
    setting.trim().eq_ignore_ascii_case(ON)
}

pub(crate) fn switch_label(setting: &str) -> &'static str {
    if on(setting) { ON } else { OFF }
}

pub(crate) fn stored(path: &str) -> &str {
    let path = path.trim();
    if path.is_empty() || path.eq_ignore_ascii_case(BUILTIN) { "" } else { path }
}

pub(crate) fn label(path: &str) -> String {
    if stored(path).is_empty() { "built-in".into() } else { path.trim().to_string() }
}

pub(crate) fn picked(path: &str) -> &'static str {
    if stored(path).is_empty() { BUILTIN } else { FILE }
}

pub(crate) fn switch_choices() -> Vec<(&'static str, &'static str)> {
    vec![(OFF, "no sound"), (ON, "on this computer, in every open window")]
}

pub(crate) fn source_choices() -> Vec<(&'static str, &'static str)> {
    vec![(BUILTIN, "a short tone"), (FILE, "a sound file on this computer")]
}

pub(crate) fn source_notice(waiting: bool, path: &str) -> String {
    let when = if waiting { "needs you" } else { "finished" };
    if path.is_empty() { format!("{when} plays the built-in sound") } else { format!("{when} plays {path}") }
}

pub(crate) fn check_path(input: &str, home: Option<&Path>) -> Result<String, &'static str> {
    let path = input.trim();
    if path.is_empty() || path.eq_ignore_ascii_case(BUILTIN) {
        return Ok(String::new());
    }
    if path.chars().any(char::is_control) {
        return Err("the path cannot contain control characters");
    }
    if !config::expand_home(path, home).is_absolute() {
        return Err("use an absolute path or one starting with ~/");
    }
    Ok(path.to_string())
}

pub(crate) fn cue(waiting: bool, config: &Config) -> Option<Cue> {
    if !on(&config.sound) {
        return None;
    }
    let kind = if waiting { Kind::Waiting } else { Kind::Done };
    let raw = if waiting { config.sound_waiting.as_str() } else { config.sound_done.as_str() };
    Some(Cue { kind, path: stored(raw).to_string() })
}

pub(crate) fn play(cue: Cue) {
    let Cue { kind, path } = cue;
    let _ = sender().send(Request { kind, file: resolve(&path) });
}

fn resolve(path: &str) -> Option<PathBuf> {
    let path = stored(path);
    if path.is_empty() {
        None
    } else {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Some(config::expand_home(path, home.as_deref()))
    }
}

fn sender() -> &'static Sender<Request> {
    static SENDER: OnceLock<Sender<Request>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let _ = thread::Builder::new().name("sound".into()).spawn(move || serve(&rx));
        tx
    })
}

fn serve(rx: &Receiver<Request>) {
    let mut playing = None;
    while let Some(request) = next(rx, &mut playing) {
        silence(&mut playing);
        playing = launch(&request);
    }
    silence(&mut playing);
}

fn next(rx: &Receiver<Request>, playing: &mut Option<Playing>) -> Option<Request> {
    loop {
        if let Some(current) = playing.as_mut()
            && running(&mut current.child)
        {
            match rx.recv_timeout(POLL) {
                Ok(request) => return Some(newest(request, rx)),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
        forget(playing);
        return rx.recv().ok().map(|request| newest(request, rx));
    }
}

fn newest(mut request: Request, rx: &Receiver<Request>) -> Request {
    while let Ok(next) = rx.try_recv() {
        request = next;
    }
    request
}

fn running(child: &mut Child) -> bool {
    match child.try_wait() {
        Ok(None) => true,
        Ok(Some(_)) => false,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            false
        }
    }
}

fn forget(playing: &mut Option<Playing>) {
    if let Some(playing) = playing.take()
        && let Some(path) = playing.temp
    {
        let _ = fs::remove_file(path);
    }
}

fn silence(playing: &mut Option<Playing>) {
    let Some(mut playing) = playing.take() else { return };
    if playing.child.try_wait().ok().flatten().is_none() {
        let _ = playing.child.kill();
        let _ = playing.child.wait();
    }
    if let Some(path) = playing.temp {
        let _ = fs::remove_file(path);
    }
}

fn launch(request: &Request) -> Option<Playing> {
    let prepared = prepare(request, which)?;
    let spawned = Command::new(&prepared.player)
        .arg(&prepared.file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(child) = spawned else {
        if prepared.temp {
            let _ = fs::remove_file(prepared.file);
        }
        return None;
    };
    Some(Playing { child, temp: prepared.temp.then_some(prepared.file) })
}

fn prepare(request: &Request, lookup: impl Fn(&str) -> Option<PathBuf>) -> Option<Prepared> {
    let player = player_names().iter().find_map(|name| lookup(name))?;
    match &request.file {
        Some(path) if path.is_file() => Some(Prepared { player, file: path.clone(), temp: false }),
        Some(_) => None,
        None => {
            let file = std::env::temp_dir().join(format!(
                "cornercase-sound-{}-{}-{}.wav",
                std::process::id(),
                request.kind.name(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(&file, wav(request.kind)).ok()?;
            Some(Prepared { player, file, temp: true })
        }
    }
}

fn player_names() -> &'static [&'static str] {
    if cfg!(target_os = "macos") { &["afplay"] } else { &["pw-play", "paplay", "aplay"] }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        can_run(&candidate).then_some(candidate)
    })
}

fn can_run(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

fn wav(kind: Kind) -> Vec<u8> {
    let samples = samples(kind);
    let data_len = u32::try_from(samples.len().saturating_mul(2)).unwrap_or(u32::MAX);
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend(b"RIFF");
    push_u32(&mut bytes, 36 + data_len);
    bytes.extend(b"WAVEfmt ");
    push_u32(&mut bytes, 16);
    push_u16(&mut bytes, 1);
    push_u16(&mut bytes, 1);
    push_u32(&mut bytes, RATE);
    push_u32(&mut bytes, RATE * 2);
    push_u16(&mut bytes, 2);
    push_u16(&mut bytes, 16);
    bytes.extend(b"data");
    push_u32(&mut bytes, data_len);
    for sample in samples {
        bytes.extend(sample.to_le_bytes());
    }
    bytes
}

#[expect(clippy::cast_precision_loss, reason = "a tone is a few hundred samples at a few kilohertz, which fits an f32")]
fn samples(kind: Kind) -> Vec<i16> {
    let mut out = Vec::new();
    for tone in kind.tones() {
        let count = usize::try_from(u64::from(RATE) * u64::from(tone.ms) / 1000).unwrap_or(0);
        let fade = (usize::try_from(RATE).unwrap_or(1) * 8 / 1000).min(count / 2).max(1);
        for i in 0..count {
            let env = if tone.hz == 0 {
                0.0
            } else if i < fade {
                i as f32 / fade as f32
            } else if i + fade >= count {
                (count - i) as f32 / fade as f32
            } else {
                1.0
            };
            let wave = if tone.hz == 0 {
                0.0
            } else {
                let t = i as f32 / RATE as f32;
                (2.0 * std::f32::consts::PI * tone.hz as f32 * t).sin()
            };
            out.push(sample(wave * env * LEVEL));
        }
    }
    out
}

fn sample(value: f32) -> i16 {
    let scaled = (value * f32::from(i16::MAX)).clamp(f32::from(i16::MIN), f32::from(i16::MAX));
    #[expect(clippy::cast_possible_truncation, reason = "the sample is clamped to the i16 range")]
    let rounded = scaled.round() as i32;
    i16::try_from(rounded).unwrap_or(0)
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

#[cfg(test)]
mod tests;
