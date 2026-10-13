use std::fmt::Write;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use rustix::fs::{Access, access};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::issues::http::{self, Service};

pub use crate::version::CURRENT;

use crate::version::newer;

pub const LATEST_ENV: &str = "CORNERCASE_RELEASES_URL";
pub const DEFAULT_LATEST: &str = "https://api.github.com/repos/usecornercase/cornercase-terminal/releases/latest";
pub const INSTALLER: &str = "curl -fsSL https://usecornercase.dev/install.sh | sh";
pub const BREW: &str = "brew update && brew upgrade cornercase";
pub const CHECK_EVERY: Duration = Duration::from_hours(1);
const SERVICE: Service = Service { name: "GitHub", rejected: "GitHub refused the update check" };
const BINARY: &str = "cornercase";
const BREW_DIRS: [&str; 3] = ["/Cellar/", "/homebrew/", "/linuxbrew/"];
const NOTES_HEADING: &str = "## Release Notes";
const CHANGELOG: &str = "CHANGELOG.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: Vec<(String, String)>,
    assets: Vec<(String, String)>,
}

impl Release {
    fn asset(&self, name: &str) -> Result<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, url)| url.as_str())
            .ok_or_else(|| Error::Api(format!("the {} release has no {name}", self.version)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    Replace(PathBuf),
    Command(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Updated(String),
    Manual(String, &'static str),
}

#[derive(Debug)]
pub struct Updates {
    pub enabled: bool,
    pub url: String,
    pub install: Install,
    pub checked: Option<Instant>,
    pub available: Option<Release>,
    pub installed: bool,
}

impl Updates {
    pub fn from_env() -> Self {
        let exe = std::env::current_exe().and_then(fs::canonicalize).ok();
        Self {
            enabled: !cfg!(debug_assertions),
            url: std::env::var(LATEST_ENV).unwrap_or_else(|_| DEFAULT_LATEST.into()),
            install: exe.map_or(Install::Command(INSTALLER), |exe| install(&exe, target())),
            checked: None,
            available: None,
            installed: false,
        }
    }

    pub fn due(&self, now: Instant) -> bool {
        self.enabled && self.checked.is_none_or(|at| now.duration_since(at) >= CHECK_EVERY)
    }
}

pub fn release(json: &Value) -> Option<Release> {
    let version = json.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    let assets = json
        .get("assets")?
        .as_array()?
        .iter()
        .filter_map(|a| {
            Some((a.get("name")?.as_str()?.to_string(), a.get("browser_download_url")?.as_str()?.to_string()))
        })
        .collect();
    let body = notes(json.get("body").and_then(Value::as_str).unwrap_or_default());
    let notes = if body.is_empty() { Vec::new() } else { vec![(version.clone(), body)] };
    Some(Release { version, notes, assets })
}

fn notes(body: &str) -> String {
    let Some((_, after)) = body.split_once(NOTES_HEADING) else { return String::new() };
    let end = after.find("\n## ").unwrap_or(after.len());
    after[..end].trim().to_string()
}

pub fn with_changelog(mut release: Release, current: &str) -> Release {
    let changelog = release.asset(CHANGELOG).and_then(|url| http::download(&SERVICE, url));
    if let Ok(changelog) = changelog {
        let notes = changelog_notes(&String::from_utf8_lossy(&changelog), current, &release.version);
        if notes.first().is_some_and(|(version, _)| *version == release.version) {
            release.notes = notes;
        }
    }
    release
}

fn changelog_notes(changelog: &str, current: &str, latest: &str) -> Vec<(String, String)> {
    format!("\n{changelog}")
        .split("\n## ")
        .skip(1)
        .filter_map(|section| {
            let (heading, body) = section.split_once('\n').unwrap_or((section, ""));
            let version = heading.trim();
            let body = body.trim();
            (newer(version, current) && !newer(version, latest) && !body.is_empty())
                .then(|| (version.to_string(), body.to_string()))
        })
        .collect()
}

pub fn check(url: &str, current: &str) -> Result<Option<Release>> {
    let answer = http::get(&SERVICE, url, &[], &[("User-Agent", BINARY)])?;
    if !answer.ok() {
        return Err(Error::Api(format!("GitHub answered {} to the update check", answer.status)));
    }
    let release = release(&answer.json).ok_or_else(|| Error::Api("GitHub sent a release without a version".into()))?;
    Ok(newer(&release.version, current).then_some(release))
}

pub fn target() -> Option<&'static str> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some("x86_64-unknown-linux-gnu"),
        ("aarch64", "linux") => Some("aarch64-unknown-linux-gnu"),
        ("x86_64", "macos") => Some("x86_64-apple-darwin"),
        ("aarch64", "macos") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

pub fn install(exe: &Path, target: Option<&str>) -> Install {
    let path = exe.to_string_lossy();
    if BREW_DIRS.iter().any(|dir| path.contains(dir)) {
        return Install::Command(BREW);
    }
    let writable = exe.parent().is_some_and(|dir| access(dir, Access::WRITE_OK).is_ok());
    if target.is_none() || !writable {
        return Install::Command(INSTALLER);
    }
    Install::Replace(exe.to_path_buf())
}

pub fn install_latest(url: &str, current: &str, install: &Install, target: Option<&str>) -> Result<Outcome> {
    let Some(release) = check(url, current)? else { return Ok(Outcome::UpToDate) };
    match (install, target) {
        (Install::Replace(exe), Some(target)) => {
            update(&release, target, exe)?;
            Ok(Outcome::Updated(release.version))
        }
        (Install::Command(command), _) => Ok(Outcome::Manual(release.version, command)),
        (Install::Replace(_), None) => Ok(Outcome::Manual(release.version, INSTALLER)),
    }
}

pub fn update(release: &Release, target: &str, exe: &Path) -> Result<()> {
    let name = format!("{BINARY}-{target}.tar.gz");
    let archive = http::download(&SERVICE, release.asset(&name)?)?;
    let sums = http::download(&SERVICE, release.asset(&format!("{name}.sha256"))?)?;
    replace(&archive, &String::from_utf8_lossy(&sums), &release.version, exe)
}

pub fn replace(archive: &[u8], sums: &str, version: &str, exe: &Path) -> Result<()> {
    verify(archive, sums)?;
    let dir = exe.parent().ok_or_else(|| Error::Api(format!("`{}` has no folder", exe.display())))?;
    let staging = dir.join(format!(".{BINARY}-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let result = fs::create_dir(&staging)
        .map_err(Error::from)
        .and_then(|()| unpack(archive, &staging))
        .and_then(|binary| check_binary(&binary, version).map(|()| binary))
        .and_then(|binary| fs::rename(&binary, exe).map_err(Error::from));
    let _ = fs::remove_dir_all(&staging);
    result
}

fn verify(archive: &[u8], sums: &str) -> Result<()> {
    let expected = sums.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    if expected == hex(&Sha256::digest(archive)) {
        Ok(())
    } else {
        Err(Error::Api("the download does not match its checksum".into()))
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn unpack(archive: &[u8], staging: &Path) -> Result<PathBuf> {
    let file = staging.join("archive.tar.gz");
    fs::write(&file, archive)?;
    let output = Command::new("tar").arg("-xzf").arg(&file).arg("-C").arg(staging).output()?;
    if !output.status.success() {
        return Err(Error::Api(format!(
            "could not unpack the download: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let nested = fs::read_dir(staging)?.flatten().map(|entry| entry.path().join(BINARY));
    let binary = std::iter::once(staging.join(BINARY))
        .chain(nested)
        .find(|path| path.is_file())
        .ok_or_else(|| Error::Api(format!("the download has no `{BINARY}`")))?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    Ok(binary)
}

fn check_binary(binary: &Path, version: &str) -> Result<()> {
    let output = Command::new(binary).arg("--version").output()?;
    let printed = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && printed.split_whitespace().nth(1) == Some(version) {
        Ok(())
    } else {
        Err(Error::Api(format!(
            "the new {BINARY} does not run here: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests;
