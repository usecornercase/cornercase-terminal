use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::text::hex;

pub(super) const COOKIE: &str = "cornercase";
pub(super) const PAIRING_FOR: Duration = Duration::from_mins(10);
const TOKEN_BYTES: usize = 16;
const SECRET_BYTES: usize = 32;
const VERSION: u32 = 1;

pub(super) fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(hex(&buf))
}

fn digest(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, Clone)]
pub(super) struct Pairing {
    pub(crate) token: String,
    until: Instant,
}

impl Pairing {
    pub(super) fn new(now: Instant) -> io::Result<Self> {
        Ok(Self { token: random_hex(TOKEN_BYTES)?, until: now + PAIRING_FOR })
    }

    pub(super) fn accepts(&self, token: &str, now: Instant) -> bool {
        now < self.until && same(&self.token, token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Device {
    pub(crate) id: String,
    pub(crate) hash: String,
    pub(crate) paired: u64,
    pub(crate) browser: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    version: u32,
    devices: Vec<Device>,
}

#[derive(Debug, Default)]
pub(super) struct Devices {
    path: PathBuf,
    list: Vec<Device>,
}

impl Devices {
    pub(super) fn load(path: PathBuf) -> Self {
        let list = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
            .filter(|saved| saved.version == VERSION)
            .map(|saved| saved.devices)
            .unwrap_or_default();
        Self { path, list }
    }

    pub(super) fn len(&self) -> usize {
        self.list.len()
    }

    pub(super) fn add(&mut self, browser: &str) -> io::Result<(String, String)> {
        let id = random_hex(8)?;
        let secret = random_hex(SECRET_BYTES)?;
        let paired = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let browser = browser.chars().filter(|c| !c.is_control()).take(120).collect();
        let mut list = self.list.clone();
        list.push(Device { id: id.clone(), hash: digest(&secret), paired, browser });
        save(&self.path, &Saved { version: VERSION, devices: list.clone() })?;
        self.list = list;
        Ok((id.clone(), format!("{id}.{secret}")))
    }

    pub(super) fn check(&self, cookie: &str) -> Option<&str> {
        let (id, secret) = cookie.split_once('.')?;
        let hash = digest(secret);
        self.list.iter().find(|d| d.id == id && same(&d.hash, &hash)).map(|d| d.id.as_str())
    }

    pub(super) fn clear(&mut self) -> io::Result<()> {
        save(&self.path, &Saved { version: VERSION, devices: Vec::new() })?;
        self.list.clear();
        Ok(())
    }
}

fn save(path: &Path, saved: &Saved) -> io::Result<()> {
    crate::persist::write(path, saved, true)
}

#[cfg(test)]
mod tests;
