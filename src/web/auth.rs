use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::update::hex;

pub const COOKIE: &str = "cornercase";
pub const PAIRING_FOR: Duration = Duration::from_mins(10);
const TOKEN_BYTES: usize = 16;
const SECRET_BYTES: usize = 32;
const VERSION: u32 = 1;

pub fn random_hex(bytes: usize) -> io::Result<String> {
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
pub struct Pairing {
    pub token: String,
    until: Instant,
}

impl Pairing {
    pub fn new(now: Instant) -> io::Result<Self> {
        Ok(Self { token: random_hex(TOKEN_BYTES)?, until: now + PAIRING_FOR })
    }

    pub fn accepts(&self, token: &str, now: Instant) -> bool {
        now < self.until && same(&self.token, token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub hash: String,
    pub paired: u64,
    pub browser: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    version: u32,
    devices: Vec<Device>,
}

#[derive(Debug, Default)]
pub struct Devices {
    path: PathBuf,
    list: Vec<Device>,
}

impl Devices {
    pub fn load(path: PathBuf) -> Self {
        let list = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
            .filter(|saved| saved.version == VERSION)
            .map(|saved| saved.devices)
            .unwrap_or_default();
        Self { path, list }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn add(&mut self, browser: &str) -> io::Result<(String, String)> {
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

    pub fn check(&self, cookie: &str) -> Option<&str> {
        let (id, secret) = cookie.split_once('.')?;
        let hash = digest(secret);
        self.list.iter().find(|d| d.id == id && same(&d.hash, &hash)).map(|d| d.id.as_str())
    }

    pub fn clear(&mut self) -> io::Result<()> {
        save(&self.path, &Saved { version: VERSION, devices: Vec::new() })?;
        self.list.clear();
        Ok(())
    }
}

fn save(path: &Path, saved: &Saved) -> io::Result<()> {
    crate::persist::write(path, saved, true)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    use super::*;
    use crate::test_util::TempDir;

    mod pairing {
        use super::*;

        #[test]
        fn accepts_its_token_until_it_expires() {
            let now = Instant::now();
            let pairing = Pairing::new(now).expect("a token");

            assert!(pairing.accepts(&pairing.token.clone(), now));
            assert!(!pairing.accepts(&pairing.token.clone(), now + PAIRING_FOR));
        }

        #[test]
        fn refuses_another_token() {
            let now = Instant::now();
            let pairing = Pairing::new(now).expect("a token");

            assert!(!pairing.accepts("0".repeat(pairing.token.len()).as_str(), now));
            assert!(!pairing.accepts("", now));
        }

        #[test]
        fn tokens_differ_each_time() {
            let now = Instant::now();

            assert_ne!(Pairing::new(now).expect("one").token, Pairing::new(now).expect("two").token);
        }
    }

    mod devices {
        use super::*;

        #[test]
        fn a_paired_browser_is_known_by_its_cookie_after_a_reload() {
            let dir = TempDir::new();
            let file = dir.path().join("browsers.json");
            let mut devices = Devices::load(file.clone());

            let (id, cookie) = devices.add("Safari on iPhone").expect("paired");

            assert_eq!(Devices::load(file).check(&cookie), Some(id.as_str()));
        }

        #[test]
        fn the_file_keeps_only_a_hash_of_the_secret_and_only_its_owner_reads_it() {
            let dir = TempDir::new();
            let file = dir.path().join("browsers.json");
            let (_, cookie) = Devices::load(file.clone()).add("Safari").expect("paired");

            let text = std::fs::read_to_string(&file).expect("saved");
            let secret = cookie.split_once('.').expect("id and secret").1;

            assert!(!text.contains(secret));
            assert_eq!(std::fs::metadata(&file).expect("metadata").mode() & 0o777, 0o600);
        }

        #[test]
        fn a_wrong_or_malformed_cookie_is_refused() {
            let dir = TempDir::new();
            let mut devices = Devices::load(dir.path().join("browsers.json"));
            let (id, _) = devices.add("Safari").expect("paired");

            assert_eq!(devices.check(&format!("{id}.{}", "0".repeat(64))), None);
            assert_eq!(devices.check(&id), None);
            assert_eq!(devices.check(""), None);
        }

        #[test]
        fn clearing_forgets_every_browser() {
            let dir = TempDir::new();
            let file = dir.path().join("browsers.json");
            let mut devices = Devices::load(file.clone());
            let (_, cookie) = devices.add("Safari").expect("paired");

            devices.clear().expect("cleared");

            assert_eq!((devices.check(&cookie), Devices::load(file).len()), (None, 0));
        }

        #[test]
        fn a_corrupt_file_means_no_browsers() {
            let dir = TempDir::new();
            let file = dir.path().join("browsers.json");
            std::fs::write(&file, "{not json").expect("write");

            assert_eq!(Devices::load(file).len(), 0);
        }
    }
}
