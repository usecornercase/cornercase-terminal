use std::fmt::{self, Write as _};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};

use super::chat::{Kind, Line};
use super::folder;
use crate::issues::Secret;
use crate::os::fs::{NONBLOCK, OpenOptionsExt};
use crate::persist::{self, Kind as Write};

pub(crate) const NAME_MAX: usize = 40;
pub(crate) const VALUE_MAX: usize = 8192;
pub(crate) const SECRETS_MAX: usize = 20;
pub(crate) const REDACT_MIN: usize = 6;
pub(crate) const OWN_ONLY: &str = "a bot reads only its own secrets";
pub(crate) const ONLY_ITSELF: &str = "only the bot itself reads its secrets";
pub(crate) const NO_CHANGE: &str = "a bot cannot save or remove secrets";
pub(crate) const WHAT_IT_IS: &str =
    "The bot reads this value: it reaches the bot's agent and stays in that agent's own records on this computer.";
const MAGIC: &[u8] = b"cc-bot-secrets-1";
const FILE_MAX: u64 = 4 * 1024 * 1024;
const KEY_LEN: usize = 32;

pub(crate) fn key_path(dir: &Path) -> PathBuf {
    let mut path = dir.as_os_str().to_os_string();
    path.push(".key");
    PathBuf::from(path)
}

pub(crate) fn unreadable(id: &str) -> String {
    format!("the secrets of {id} cannot be read")
}

pub(crate) fn unreadable_hint(id: &str) -> String {
    format!("{}; cornercase bot secret remove {id} --all starts again", unreadable(id))
}

pub(crate) fn name_refusal(name: &str) -> Option<String> {
    let length = name.chars().count();
    if length == 0 || length > NAME_MAX {
        return Some(format!("a secret's name takes 1 to {NAME_MAX} characters, not {length}"));
    }
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    (!name.chars().all(allowed)).then(|| String::from("a secret's name takes only a-z, 0-9, - and _"))
}

pub(crate) fn value_refusal(value: &str) -> Option<String> {
    if value.is_empty() {
        return Some(String::from("the value is empty"));
    }
    (value.len() > VALUE_MAX).then(|| format!("the value is {} bytes; a secret takes at most 8,192", value.len()))
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) value: Secret,
    pub(crate) saved: i64,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entry({}, ..)", self.name)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Kept {
    Open(Vec<Entry>),
    Unreadable,
}

impl fmt::Debug for Kept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(entries) => write!(f, "Open({} secrets)", entries.len()),
            Self::Unreadable => f.write_str("Unreadable"),
        }
    }
}

impl Default for Kept {
    fn default() -> Self {
        Self::Open(Vec::new())
    }
}

impl Kept {
    pub(crate) fn entries(&self) -> &[Entry] {
        match self {
            Self::Open(entries) => entries,
            Self::Unreadable => &[],
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.entries().len()
    }

    pub(crate) fn is_unreadable(&self) -> bool {
        matches!(self, Self::Unreadable)
    }

    pub(crate) fn find(&self, name: &str) -> Option<&Entry> {
        self.entries().iter().find(|entry| entry.name == name)
    }

    pub(crate) fn with(&self, name: &str, value: &str, saved: i64) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self.entries().iter().filter(|entry| entry.name != name).cloned().collect();
        entries.push(Entry { name: name.to_string(), value: Secret(value.to_string()), saved });
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    pub(crate) fn without(&self, name: &str) -> Vec<Entry> {
        self.entries().iter().filter(|entry| entry.name != name).cloned().collect()
    }

    pub(crate) fn redacted(&self, retired: &[Entry], mut line: Line) -> Line {
        let all: Vec<Entry> = self.entries().iter().chain(retired).cloned().collect();
        if all.iter().all(|entry| entry.value.0.len() < REDACT_MIN) {
            return line;
        }
        let kinds = [Kind::Step, Kind::Bot, Kind::Error, Kind::Started, Kind::Finished, Kind::Waiting];
        if !kinds.contains(&line.kind) {
            return line;
        }
        line.text = redact(&line.text, &all);
        line.body = line.body.map(|body| redact(&body, &all));
        line
    }
}

pub(crate) fn redact(text: &str, entries: &[Entry]) -> String {
    let mut values: Vec<&Entry> = entries.iter().filter(|entry| entry.value.0.len() >= REDACT_MIN).collect();
    values.sort_by_key(|entry| std::cmp::Reverse(entry.value.0.len()));
    let mut taken: Vec<(usize, usize, &str)> = Vec::new();
    for entry in values {
        for (start, found) in text.match_indices(entry.value.0.as_str()) {
            let end = start + found.len();
            if !taken.iter().any(|(from, to, _)| start < *to && *from < end) {
                taken.push((start, end, entry.name.as_str()));
            }
        }
    }
    taken.sort_unstable_by_key(|(start, ..)| *start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end, name) in taken {
        out.push_str(&text[at..start]);
        let _ = write!(out, "[secret {name}]");
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Key([u8; KEY_LEN]);

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum KeyState {
    #[default]
    Missing,
    Ready(Key),
    Broken,
}

fn read_capped(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    let file = std::fs::OpenOptions::new().read(true).custom_flags(NONBLOCK).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("it is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(io::Error::other("the file is too big"));
    }
    Ok(bytes)
}

pub(crate) fn read_key(path: &Path) -> KeyState {
    match read_capped(path, KEY_LEN as u64) {
        Ok(bytes) => <[u8; KEY_LEN]>::try_from(bytes).map_or(KeyState::Broken, |bytes| KeyState::Ready(Key(bytes))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => KeyState::Missing,
        Err(_) => KeyState::Broken,
    }
}

pub(crate) fn make_key(path: &Path) -> io::Result<Key> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut bytes = [0u8; KEY_LEN];
    crate::os::random::fill(&mut bytes)?;
    let dir = path.parent().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    std::fs::create_dir_all(dir)?;
    let temp = persist::temporary(path, std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))?;
    let made = install(&temp, path, &bytes);
    let _ = std::fs::remove_file(&temp);
    made?;
    crate::os::fs::sync_dir(dir)?;
    Ok(Key(bytes))
}

fn install(temp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(temp)?;
    #[cfg(windows)]
    crate::os::fs::make_private(temp)?;
    crate::os::fs::set_file_mode(&file, 0o600)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    match std::fs::hard_link(temp, path) {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => create_in_place(path, bytes),
        linked => linked,
    }
}

fn create_in_place(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    #[cfg(windows)]
    crate::os::fs::make_private(path)?;
    let written =
        crate::os::fs::set_file_mode(&file, 0o600).and_then(|()| file.write_all(bytes)).and_then(|()| file.sync_all());
    if written.is_err() {
        drop(file);
        let _ = std::fs::remove_file(path);
    }
    written
}

fn cipher(key: &Key) -> io::Result<LessSafeKey> {
    UnboundKey::new(&AES_256_GCM, &key.0).map(LessSafeKey::new).map_err(|_| io::Error::other("the key does not fit"))
}

pub(crate) fn seal(key: &Key, id: &str, entries: &[Entry]) -> io::Result<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    crate::os::random::fill(&mut nonce)?;
    let mut bytes = serde_json::to_vec(&serde_json::json!({ "secrets": entries }))?;
    let sealed = cipher(key)?.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(id.as_bytes()),
        &mut bytes,
    );
    sealed.map_err(|_| io::Error::other("the secrets could not be sealed"))?;
    let mut file = MAGIC.to_vec();
    file.extend_from_slice(&nonce);
    file.extend_from_slice(&bytes);
    Ok(file)
}

#[derive(Deserialize)]
struct Opened {
    secrets: Vec<Entry>,
}

pub(crate) fn open(key: &Key, id: &str, file: &[u8]) -> Option<Vec<Entry>> {
    let rest = file.strip_prefix(MAGIC)?;
    let (nonce, sealed) = rest.split_at_checked(NONCE_LEN)?;
    let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
    let mut bytes = sealed.to_vec();
    let plain = cipher(key).ok()?.open_in_place(nonce, Aad::from(id.as_bytes()), &mut bytes).ok()?;
    serde_json::from_slice::<Opened>(plain).ok().map(|opened| opened.secrets)
}

pub(crate) fn load(bot: &Path, id: &str, key: &KeyState) -> Kept {
    match read_capped(&bot.join(folder::SECRETS_FILE), FILE_MAX) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Kept::default(),
        Err(_) => Kept::Unreadable,
        Ok(bytes) => match key {
            KeyState::Ready(key) => open(key, id, &bytes).map_or(Kept::Unreadable, Kept::Open),
            KeyState::Missing | KeyState::Broken => Kept::Unreadable,
        },
    }
}

pub(crate) fn save(bot: &Path, id: &str, key: &Key, entries: &[Entry]) -> io::Result<()> {
    if entries.is_empty() {
        return erase(bot);
    }
    persist::write_bytes(&bot.join(folder::SECRETS_FILE), &seal(key, id, entries)?, Write::Private).map(drop)
}

pub(crate) fn erase(bot: &Path) -> io::Result<()> {
    match std::fs::remove_file(bot.join(folder::SECRETS_FILE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
