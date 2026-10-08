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

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
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

pub fn newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

fn parse(version: &str) -> Option<(u64, u64, u64, bool)> {
    let version = version.trim().trim_start_matches('v');
    let (core, pre) = version.split_once('-').map_or((version, None), |(core, pre)| (core, Some(pre)));
    let mut parts = core.split('+').next()?.split('.').map(|p| p.parse::<u64>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some((major, minor, patch, pre.is_none()))
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

fn hex(bytes: &[u8]) -> String {
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
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_util::{FakeHttp, TempDir, write_executable};

    mod newer {
        use super::*;

        #[rstest]
        #[case::patch("0.1.1", "0.1.0", true)]
        #[case::minor_beats_patch("0.2.0", "0.1.9", true)]
        #[case::numbers_not_text("0.10.0", "0.9.0", true)]
        #[case::with_a_v("v0.2.0", "0.1.0", true)]
        #[case::same("0.1.0", "0.1.0", false)]
        #[case::older("0.1.0", "0.2.0", false)]
        #[case::release_beats_its_prerelease("0.2.0", "0.2.0-rc.1", true)]
        #[case::prerelease_is_older("0.2.0-rc.1", "0.2.0", false)]
        #[case::garbage("latest", "0.1.0", false)]
        #[case::too_few_parts("1.2", "0.1.0", false)]
        fn compares_versions(#[case] latest: &str, #[case] current: &str, #[case] expected: bool) {
            assert_eq!(newer(latest, current), expected);
        }
    }

    mod check {
        use super::*;

        const LATEST: &str = r#"{"tag_name": "v9.0.0", "assets": [
            {"name": "cornercase-x86_64-unknown-linux-gnu.tar.gz", "browser_download_url": "https://x/a.tar.gz"}
        ]}"#;

        fn latest(server: &FakeHttp) -> String {
            format!("{}/releases/latest", server.url())
        }

        #[test]
        fn finds_a_newer_release_and_its_assets() {
            let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);

            let release = check(&latest(&server), "0.1.0").expect("check").expect("a newer release");

            assert_eq!(release.version, "9.0.0");
            assert_eq!(release.asset("cornercase-x86_64-unknown-linux-gnu.tar.gz").ok(), Some("https://x/a.tar.gz"));
        }

        #[rstest]
        #[case::only_the_notes(
            "## Release Notes\n\n- Faster.\n\n### Fixes\n\n- A crash.\n\n## Install cornercase 9.0.0\n\ncurl …",
            "- Faster.\n\n### Fixes\n\n- A crash."
        )]
        #[case::without_notes("## Install cornercase 9.0.0\n\ncurl …", "")]
        #[case::notes_at_the_end("## Release Notes\n\n- Faster.\n", "- Faster.")]
        fn keeps_the_release_notes_of_the_body(#[case] body: &str, #[case] expected: &str) {
            let json = serde_json::json!({"tag_name": "v9.0.0", "assets": [], "body": body});
            let notes = release(&json).expect("a release").notes;
            assert_eq!(notes.first().map_or("", |(_, notes)| notes.as_str()), expected);
        }

        #[test]
        fn ignores_a_release_that_is_not_newer() {
            let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
            assert_eq!(check(&latest(&server), "9.0.0").expect("check"), None);
        }

        #[test]
        fn sends_a_user_agent() {
            let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);

            let _ = check(&latest(&server), "0.1.0");

            assert!(server.request(0).to_lowercase().contains("user-agent: cornercase"), "{}", server.request(0));
        }

        #[test]
        fn fails_when_there_is_no_release() {
            let server = FakeHttp::start(vec![("GET /releases/latest", 404, r#"{"message": "Not Found"}"#)]);

            let err = check(&latest(&server), "0.1.0").err().map(|e| e.to_string());

            assert_eq!(err.as_deref(), Some("GitHub answered 404 to the update check"));
        }
    }

    mod changelog {
        use super::*;

        const CHANGELOG: &str = "# Changelog\n\nIntro.\n\n## 9.1.0\n\n- Later.\n\n## 9.0.0\n\n- Newest.\n\n\
                                 ## 8.0.0\n\n- Middle.\n\n### Fixes\n\n- A crash.\n\n## 7.0.0\n\n\
                                 ## 0.1.0\n\n- Current.\n\n## 0.0.9\n\n- Older.\n";

        fn notes(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
            pairs.iter().map(|(version, notes)| ((*version).to_string(), (*notes).to_string())).collect()
        }

        fn with_body(assets: &Value) -> Release {
            let body = "## Release Notes\n\n- From the body.\n";
            release(&serde_json::json!({"tag_name": "v9.0.0", "assets": assets, "body": body})).expect("a release")
        }

        #[test]
        fn keeps_every_version_after_the_current_one_up_to_the_latest() {
            assert_eq!(
                changelog_notes(CHANGELOG, "0.1.0", "9.0.0"),
                notes(&[("9.0.0", "- Newest."), ("8.0.0", "- Middle.\n\n### Fixes\n\n- A crash.")])
            );
        }

        #[test]
        fn the_release_notes_come_from_its_changelog() {
            let server = FakeHttp::start(vec![("GET /CHANGELOG.md", 200, CHANGELOG)]);
            let url = format!("{}/CHANGELOG.md", server.url());

            let release = with_changelog(
                with_body(&serde_json::json!([{"name": "CHANGELOG.md", "browser_download_url": url}])),
                "0.1.0",
            );

            assert_eq!(
                release.notes,
                notes(&[("9.0.0", "- Newest."), ("8.0.0", "- Middle.\n\n### Fixes\n\n- A crash.")])
            );
        }

        #[test]
        fn a_release_without_a_changelog_keeps_its_own_notes() {
            assert_eq!(
                with_changelog(with_body(&serde_json::json!([])), "0.1.0").notes,
                notes(&[("9.0.0", "- From the body.")])
            );
        }

        #[test]
        fn a_changelog_without_the_release_keeps_its_own_notes() {
            let server = FakeHttp::start(vec![("GET /CHANGELOG.md", 200, "## 8.0.0\n\n- Middle.\n")]);
            let url = format!("{}/CHANGELOG.md", server.url());

            let release = with_changelog(
                with_body(&serde_json::json!([{"name": "CHANGELOG.md", "browser_download_url": url}])),
                "0.1.0",
            );

            assert_eq!(release.notes, notes(&[("9.0.0", "- From the body.")]));
        }

        #[test]
        fn a_changelog_that_cannot_be_downloaded_keeps_the_release_notes() {
            let server = FakeHttp::start(vec![("GET /CHANGELOG.md", 404, "")]);
            let url = format!("{}/CHANGELOG.md", server.url());

            let release = with_changelog(
                with_body(&serde_json::json!([{"name": "CHANGELOG.md", "browser_download_url": url}])),
                "0.1.0",
            );

            assert_eq!(release.notes, notes(&[("9.0.0", "- From the body.")]));
        }
    }

    mod install {
        use super::*;

        #[rstest]
        #[case::cellar("/opt/homebrew/Cellar/cornercase/0.1.0/bin/cornercase")]
        #[case::linuxbrew("/home/linuxbrew/.linuxbrew/bin/cornercase")]
        fn homebrew_installs_ask_for_brew(#[case] exe: &str) {
            assert_eq!(install(Path::new(exe), target()), Install::Command(BREW));
        }

        #[test]
        fn a_writable_folder_is_replaced_in_place() {
            let tmp = TempDir::new();
            let exe = tmp.path().join("cornercase");

            assert_eq!(install(&exe, Some("x86_64-unknown-linux-gnu")), Install::Replace(exe));
        }

        #[test]
        fn a_folder_we_cannot_write_asks_for_the_installer() {
            assert_eq!(install(Path::new("/cornercase-missing/cornercase"), target()), Install::Command(INSTALLER));
        }

        #[test]
        fn an_unknown_platform_asks_for_the_installer() {
            let tmp = TempDir::new();
            assert_eq!(install(&tmp.path().join("cornercase"), None), Install::Command(INSTALLER));
        }
    }

    mod replace {
        use super::*;

        const NEW: &str = "#!/bin/sh\necho 'cornercase 9.0.0'\n";

        fn archive(tmp: &TempDir, script: &str) -> Vec<u8> {
            let dir = tmp.path().join("build").join("cornercase-x86_64-unknown-linux-gnu");
            fs::create_dir_all(&dir).expect("archive folder");
            write_executable(&dir.join("cornercase"), script);
            let file = tmp.path().join("build.tar.gz");
            let status = Command::new("tar")
                .arg("-czf")
                .arg(&file)
                .arg("-C")
                .arg(tmp.path().join("build"))
                .arg("cornercase-x86_64-unknown-linux-gnu")
                .status()
                .expect("run tar");
            assert!(status.success());
            fs::read(file).expect("read archive")
        }

        fn sums(bytes: &[u8]) -> String {
            let hex = hex(&Sha256::digest(bytes));
            format!("{hex} *cornercase-x86_64-unknown-linux-gnu.tar.gz\n")
        }

        fn installed(tmp: &TempDir) -> PathBuf {
            let bin = tmp.path().join("bin");
            fs::create_dir_all(&bin).expect("bin folder");
            let exe = bin.join("cornercase");
            fs::write(&exe, "old").expect("old binary");
            exe
        }

        #[test]
        fn swaps_the_binary_for_the_new_one() {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let bytes = archive(&tmp, NEW);

            replace(&bytes, &sums(&bytes), "9.0.0", &exe).expect("replace");

            assert_eq!(fs::read_to_string(&exe).expect("new binary"), NEW);
            assert_eq!(fs::read_dir(exe.parent().expect("bin")).expect("bin").count(), 1);
        }

        #[test]
        fn keeps_the_old_binary_when_the_checksum_differs() {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let bytes = archive(&tmp, NEW);

            let result = replace(&bytes, &sums(b"other"), "9.0.0", &exe);

            assert_refused_checksum(result, &exe);
        }

        fn assert_refused_checksum<T: std::fmt::Debug>(result: Result<T>, exe: &Path) {
            let err = result.err().map(|e| e.to_string());
            assert_eq!(err.as_deref(), Some("the download does not match its checksum"));
            assert_eq!(fs::read_to_string(exe).expect("old binary"), "old");
        }

        fn publish(tmp: &TempDir, sums: impl FnOnce(&[u8]) -> String) -> (FakeHttp, FakeHttp) {
            let bytes = archive(tmp, NEW);
            let checksums = sums(&bytes);
            let files = FakeHttp::start(vec![("GET /a.tar.gz", 200, bytes), ("GET /a.sha256", 200, checksums.into())]);
            let name = "cornercase-x86_64-unknown-linux-gnu.tar.gz";
            let json = serde_json::json!({"tag_name": "v9.0.0", "assets": [
                {"name": name, "browser_download_url": format!("{}/a.tar.gz", files.url())},
                {"name": format!("{name}.sha256"), "browser_download_url": format!("{}/a.sha256", files.url())},
            ]});
            let releases = FakeHttp::start(vec![("GET /releases/latest", 200, json.to_string())]);
            (releases, files)
        }

        fn install_from(releases: &FakeHttp, current: &str, install: &Install) -> Result<Outcome> {
            let url = format!("{}/releases/latest", releases.url());
            install_latest(&url, current, install, Some("x86_64-unknown-linux-gnu"))
        }

        #[test]
        fn installs_the_latest_release_over_the_binary() {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let (releases, _files) = publish(&tmp, sums);

            let outcome = install_from(&releases, "0.1.0", &Install::Replace(exe.clone())).expect("install");

            assert_eq!(outcome, Outcome::Updated("9.0.0".into()));
            assert_eq!(fs::read_to_string(&exe).expect("new binary"), NEW);
        }

        #[test]
        fn leaves_an_up_to_date_binary_alone() {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let (releases, files) = publish(&tmp, sums);

            let outcome = install_from(&releases, "9.0.0", &Install::Replace(exe.clone())).expect("install");

            assert_eq!(outcome, Outcome::UpToDate);
            assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
            assert!(files.requests().is_empty(), "{:?}", files.requests());
        }

        #[test]
        fn a_bad_checksum_keeps_the_old_binary() {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let (releases, _files) = publish(&tmp, |_| sums(b"other"));

            let result = install_from(&releases, "0.1.0", &Install::Replace(exe.clone()));

            assert_refused_checksum(result, &exe);
        }

        #[rstest]
        #[case::homebrew(BREW)]
        #[case::read_only_folder(INSTALLER)]
        fn an_install_we_cannot_replace_gets_its_command(#[case] command: &'static str) {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let (releases, files) = publish(&tmp, sums);

            let outcome = install_from(&releases, "0.1.0", &Install::Command(command)).expect("install");

            assert_eq!(outcome, Outcome::Manual("9.0.0".into(), command));
            assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
            assert!(files.requests().is_empty(), "{:?}", files.requests());
        }

        #[test]
        fn keeps_the_old_binary_when_the_new_one_does_not_run() {
            let tmp = TempDir::new();
            let exe = installed(&tmp);
            let bytes = archive(&tmp, "#!/bin/sh\necho 'missing library' >&2\nexit 127\n");

            let err = replace(&bytes, &sums(&bytes), "9.0.0", &exe).err().map(|e| e.to_string());

            assert_eq!(err.as_deref(), Some("the new cornercase does not run here: missing library"));
            assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
            assert_eq!(fs::read_dir(exe.parent().expect("bin")).expect("bin").count(), 1);
        }
    }
}
