use std::io;
use std::path::{Path, PathBuf};

use super::Bot;
use crate::persist::{self, Kind};

pub(crate) const BOT_FILE: &str = "bot.json";
pub(crate) const ABOUT_FILE: &str = "bot.md";
pub(crate) const MEMORY_FILE: &str = "memory.md";
pub(crate) const CHAT_FILE: &str = "chat.jsonl";

pub(crate) fn of(dir: &Path, id: &str) -> PathBuf {
    dir.join(id)
}

pub(crate) fn create(dir: &Path, bot: &Bot, about: &str) -> io::Result<PathBuf> {
    let folder = of(dir, &bot.id);
    crate::os::fs::create_private_dir_all(&folder)?;
    save(&folder, bot)?;
    persist::write_bytes(&folder.join(ABOUT_FILE), about.as_bytes(), Kind::Private)?;
    persist::write_bytes(&folder.join(MEMORY_FILE), b"", Kind::Private)?;
    persist::write_bytes(&folder.join(CHAT_FILE), b"", Kind::Private)?;
    Ok(folder)
}

pub(crate) fn save(folder: &Path, bot: &Bot) -> io::Result<()> {
    persist::write(&folder.join(BOT_FILE), bot, true)
}

pub(crate) fn remove(folder: &Path) -> io::Result<()> {
    std::fs::remove_dir_all(folder)
}

pub(crate) fn load_all(dir: &Path) -> Vec<Bot> {
    let Ok(listing) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut bots: Vec<Bot> = listing.flatten().filter_map(|entry| load(&entry.path())).collect();
    bots.sort_by(|a, b| a.created.cmp(&b.created).then_with(|| a.id.cmp(&b.id)));
    bots
}

fn load(folder: &Path) -> Option<Bot> {
    let bytes = std::fs::read(folder.join(BOT_FILE)).ok()?;
    let bot: Bot = serde_json::from_slice(&bytes).ok()?;
    (folder.file_name()?.to_str()? == bot.id).then_some(bot)
}

pub(crate) fn notes(folder: &Path) -> (String, String) {
    let read = |name: &str| std::fs::read_to_string(folder.join(name)).unwrap_or_default();
    (read(ABOUT_FILE), read(MEMORY_FILE))
}

#[cfg(test)]
mod tests;
