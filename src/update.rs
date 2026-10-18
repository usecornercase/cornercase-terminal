pub(crate) mod restart;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use rustix::fs::{Access, access};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::http::{self, Service};
use crate::text::hex;

use crate::version::newer;

pub const LATEST_ENV: &str = "CORNERCASE_RELEASES_URL";
pub(crate) const DEFAULT_LATEST: &str =
    "https://api.github.com/repos/usecornercase/cornercase-terminal/releases/latest";
pub(crate) const INSTALLER: &str = "curl -fsSL https://usecornercase.dev/install.sh | sh";
pub(crate) const BREW: &str = "brew update && brew upgrade cornercase";
pub(crate) const CHECK_EVERY: Duration = Duration::from_hours(1);
const SERVICE: Service = Service { name: "GitHub", rejected: "GitHub refused the update check" };
const BINARY: &str = "cornercase";
const BREW_DIRS: [&str; 3] = ["/Cellar/", "/homebrew/", "/linuxbrew/"];
const NOTES_HEADING: &str = "## Release Notes";
const CHANGELOG: &str = "CHANGELOG.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Release {
    pub(crate) version: String,
    pub(crate) notes: Vec<(String, String)>,
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
pub(crate) enum Install {
    Replace(PathBuf),
    Command(&'static str),
}

impl Install {
    pub(crate) fn on(&self, target: Option<&str>) -> Self {
        match (self, target) {
            (Self::Replace(_), None) => Self::Command(INSTALLER),
            _ => self.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    UpToDate,
    Updated(String),
    Manual(String, &'static str),
}

#[derive(Debug)]
#[expect(clippy::struct_excessive_bools, reason = "independent facts of a check and an install")]
pub(crate) struct Updates {
    pub(crate) enabled: bool,
    pub(crate) url: String,
    pub(crate) install: Install,
    pub(crate) checked: Option<Instant>,
    pub(crate) available: Option<Release>,
    pub(crate) installed: bool,
    pub(crate) checking: bool,
    pub(crate) answered: Option<SystemTime>,
    pub(crate) error: Option<String>,
    pub(crate) installing: bool,
    pub(crate) install_error: Option<String>,
}

impl Updates {
    pub(crate) fn from_env() -> Self {
        let exe = std::env::current_exe().and_then(fs::canonicalize).ok();
        Self {
            enabled: !cfg!(debug_assertions),
            url: std::env::var(LATEST_ENV).unwrap_or_else(|_| DEFAULT_LATEST.into()),
            install: exe.map_or(Install::Command(INSTALLER), |exe| install(&exe, target())),
            checked: None,
            available: None,
            installed: false,
            checking: false,
            answered: None,
            error: None,
            installing: false,
            install_error: None,
        }
    }

    pub(crate) fn due(&self, now: Instant) -> bool {
        self.enabled && self.checked.is_none_or(|at| now.duration_since(at) >= CHECK_EVERY)
    }
}

pub(crate) fn release(json: &Value) -> Option<Release> {
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

pub(crate) fn with_changelog(mut release: Release, current: &str) -> Release {
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

pub(crate) fn check(url: &str, current: &str) -> Result<Option<Release>> {
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

pub(crate) fn install(exe: &Path, target: Option<&str>) -> Install {
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

pub(crate) fn install_latest(url: &str, current: &str, install: &Install, target: Option<&str>) -> Result<Outcome> {
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

pub(crate) fn update(release: &Release, target: &str, exe: &Path) -> Result<()> {
    let name = format!("{BINARY}-{target}.tar.gz");
    let archive = http::download(&SERVICE, release.asset(&name)?)?;
    let sums = http::download(&SERVICE, release.asset(&format!("{name}.sha256"))?)?;
    replace(&archive, &String::from_utf8_lossy(&sums), &release.version, exe)
}

pub(crate) fn replace(archive: &[u8], sums: &str, version: &str, exe: &Path) -> Result<()> {
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
pub(crate) mod tests;
