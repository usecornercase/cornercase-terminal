use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use super::archive;
use super::manifest::{self, Manifest};
use super::registry::{Entry, Plugin, Registry, Source, short};
use super::run::{self, Places, private_dir};
use super::source::Origin;
use super::supervise;
use crate::git::worktree;
use crate::http::{self, Service};
use crate::process;
use crate::text;

const BUILD_WITHIN: Duration = Duration::from_mins(15);
const STOP_WAIT: Duration = Duration::from_secs(10);
const STAGING: &str = ".staging";
const CHECKOUT: &str = "plugin";
const SERVICE: Service = Service { name: "the plugin's download", rejected: "the plugin's download was refused" };
const GIT_ENV: [&str; 8] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CONFIG_PARAMETERS",
];

static STAGED: AtomicU32 = AtomicU32::new(0);

#[derive(Debug)]
pub(crate) struct Fetched {
    pub(crate) origin: Origin,
    pub(crate) commit: String,
    pub(crate) manifest: Manifest,
    pub(crate) replacing: Option<bool>,
    staging: Staging,
}

#[derive(Debug)]
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) manifest: Manifest,
    pub(crate) enabled: bool,
    pub(crate) previous: Option<String>,
}

#[derive(Debug)]
pub(crate) enum Update {
    UpToDate(String),
    Changed(Box<Fetched>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Removed {
    Unlinked,
    Deleted,
}

impl Fetched {
    fn checkout(&self) -> PathBuf {
        self.staging.0.join(CHECKOUT)
    }

    fn root(&self) -> PathBuf {
        let checkout = self.checkout();
        self.origin.subdir.as_ref().map_or_else(|| checkout.clone(), |subdir| checkout.join(subdir))
    }

    pub(crate) fn preview(&self) -> Vec<String> {
        let m = &self.manifest;
        let mut lines = vec![format!("{} {} ({})", m.name, m.version, m.id)];
        if !m.description.trim().is_empty() {
            lines.push(m.description.trim().to_string());
        }
        let at = self.origin.reference.as_ref().map_or_else(String::new, |r| format!(" (`{r}`)"));
        lines.push(format!("From {} at commit {}{at}.", self.origin.place(), self.commit));
        lines.push(format!("It {}; that is shown, not enforced.", m.asks()));
        match m.download_here() {
            Some(download) => {
                lines.push(format!("It downloads {} and checks its sha256 {}.", download.url, download.sha256));
            }
            None if m.build.is_empty() => {}
            None => {
                lines.push("Installing it runs, in its folder:".into());
                lines.extend(m.build.iter().map(|build| format!("  {}", shown(&build.command))));
            }
        }
        let commands = m.commands();
        if commands.is_empty() {
            lines.push("Once on, it runs nothing.".into());
        } else {
            lines.push("Once on, it runs:".into());
            lines.extend(commands.iter().map(|(what, command)| format!("  {what}: {}", shown(command))));
        }
        lines.push(match self.replacing {
            None => "It is installed turned off.".into(),
            Some(false) => "It replaces the version you have, which is off, and stays off.".into(),
            Some(true) => "It replaces the version you have, which is on: its new commands run right away.".into(),
        });
        lines.push(
            "Once on, it runs as you and can do anything you can, in every terminal: read its code or trust its author."
                .into(),
        );
        lines.iter().map(|line| text::escape_hidden(line)).collect()
    }
}

fn shown(command: &[String]) -> String {
    command
        .iter()
        .map(|arg| {
            if arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || "'\"\\$`".contains(c)) {
                format!("'{}'", arg.replace('\'', "'\\''"))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn fetch(origin: &Origin, places: &Places) -> Result<Fetched, String> {
    fetch_from(origin, places).map_err(|e| text::escape_hidden(&e))
}

fn fetch_from(origin: &Origin, places: &Places) -> Result<Fetched, String> {
    let staging = Staging(stage(places).map_err(|e| format!("cannot make a folder to fetch into: {e}"))?);
    let checkout = staging.0.join(CHECKOUT);
    git(
        origin,
        None,
        [
            OsStr::new("clone"),
            "--quiet".as_ref(),
            "--no-checkout".as_ref(),
            "--".as_ref(),
            origin.url.as_ref(),
            checkout.as_os_str(),
        ],
    )?;
    let pinned = origin.reference.as_ref().filter(|r| is_commit(r)).map(|r| r.to_ascii_lowercase());
    let candidates: Vec<String> = match &origin.reference {
        Some(_) if let Some(pinned) = &pinned => vec![pinned.clone()],
        Some(r) => vec![format!("refs/remotes/origin/{r}"), format!("refs/tags/{r}"), r.clone()],
        None => vec!["HEAD".into()],
    };
    let commit = candidates
        .iter()
        .find_map(|candidate| {
            let target = format!("{candidate}^{{commit}}");
            let args = ["rev-parse", "--verify", "--quiet", "--end-of-options", target.as_str()];
            git(origin, Some(&checkout), args).ok()
        })
        .map(|out| String::from_utf8_lossy(&out).trim().to_string())
        .ok_or_else(|| match &origin.reference {
            Some(r) => format!("{} has no branch, tag or commit `{r}`", origin.repo),
            None => format!("{} has no commits", origin.repo),
        })?;
    if pinned.as_ref().is_some_and(|pinned| *pinned != commit) {
        return Err(format!("{} gave {commit} for the commit {}; nothing was installed", origin.repo, origin.spec()));
    }
    git(origin, Some(&checkout), ["checkout", "--quiet", "--detach", commit.as_str()])?;
    let head = git(origin, Some(&checkout), ["rev-parse", "--verify", "HEAD^{commit}"])?;
    if String::from_utf8_lossy(&head).trim() != commit {
        return Err(format!("the checkout of {} is not at {commit}; nothing was installed", origin.repo));
    }
    fs::remove_dir_all(checkout.join(".git")).map_err(|e| format!("cannot drop the history: {e}"))?;
    let root = match &origin.subdir {
        Some(subdir) => {
            inside(&checkout, subdir).map_err(|e| format!("{} has no folder `{subdir}`: {e}", origin.repo))?
        }
        None => checkout,
    };
    let manifest = Manifest::read(&root)?;
    let registry = Registry::load(&places.registry);
    conflict(&registry, &manifest.id, origin)?;
    let replacing = registry.entry(&manifest.id).ok().map(|entry| entry.enabled);
    Ok(Fetched { origin: origin.clone(), commit, manifest, replacing, staging })
}

fn inside(checkout: &Path, subdir: &str) -> std::io::Result<PathBuf> {
    let mut path = checkout.to_path_buf();
    for part in subdir.split('/') {
        path.push(part);
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_dir() {
            return Err(std::io::Error::other("it is a file or a link, not a folder"));
        }
    }
    Ok(path)
}

fn is_commit(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
}

fn stage(places: &Places) -> std::io::Result<PathBuf> {
    let root = places.installs.join(STAGING);
    private_dir(&places.installs)?;
    private_dir(&root)?;
    for entry in fs::read_dir(&root)?.flatten() {
        let name = entry.file_name();
        let pid = name.to_string_lossy().split('-').next().and_then(|pid| pid.parse::<i32>().ok());
        if pid.is_some_and(|pid| !process::alive(pid)) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    let path = root.join(format!("{}-{}", std::process::id(), STAGED.fetch_add(1, Ordering::Relaxed)));
    let _ = fs::remove_dir_all(&path);
    private_dir(&path)?;
    Ok(path)
}

fn git<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(
    origin: &Origin,
    dir: Option<&Path>,
    args: I,
) -> Result<Vec<u8>, String> {
    let protocol = origin.protocol();
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    command
        .args(["-c", "core.hooksPath=/dev/null", "-c", "transfer.fsckObjects=true", "-c", "protocol.allow=never"])
        .arg("-c")
        .arg(format!("protocol.{protocol}.allow=always"))
        .args(["-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=30", "-c", "core.symlinks=true"])
        .args(args)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env("GIT_ALLOW_PROTOCOL", protocol);
    for key in GIT_ENV {
        command.env_remove(key);
    }
    let out = command.output().map_err(|e| format!("cannot run git: {e}"))?;
    worktree::check(out).map_err(|e| format!("git: {}", text::escape_hidden(&e.to_string())))
}

fn conflict(registry: &Registry, id: &str, origin: &Origin) -> Result<(), String> {
    match registry.entries.iter().find(|entry| entry.id == id).map(|entry| &entry.source) {
        Some(Source::Git { url, subdir, .. }) if *url == origin.url && *subdir == origin.subdir => Ok(()),
        Some(source @ Source::Link { .. }) => Err(format!(
            "a plugin called `{id}` is already {}; `cornercase plugin unlink {id}` first",
            source.describe()
        )),
        Some(source) => Err(format!(
            "a plugin called `{id}` is already {}; `cornercase plugin remove {id}` first",
            source.describe()
        )),
        None => Ok(()),
    }
}

pub(crate) fn check_update(places: &Places, id: &str) -> Result<Update, String> {
    let registry = Registry::load(&places.registry);
    let entry = registry.entry(id)?;
    let Source::Git { repo, url, reference, subdir, commit, .. } = &entry.source else {
        return Err(format!(
            "`{id}` is {}; a linked plugin is read from its folder, not updated",
            entry.source.describe()
        ));
    };
    let origin = Origin { repo: repo.clone(), url: url.clone(), reference: reference.clone(), subdir: subdir.clone() };
    let fetched = fetch(&origin, places)?;
    if fetched.manifest.id != id {
        return Err(format!("{repo} now holds `{}`, not `{id}`; nothing was changed", fetched.manifest.id));
    }
    if fetched.commit == *commit {
        return Ok(Update::UpToDate(commit.clone()));
    }
    Ok(Update::Changed(Box::new(fetched)))
}

#[expect(clippy::needless_pass_by_value, reason = "installing uses up the fetched copy; its folder goes with it")]
pub(crate) fn install(fetched: Fetched, places: &Places) -> Result<Installed, String> {
    let root = fetched.root();
    let id = fetched.manifest.id.clone();
    if let Some(download) = fetched.manifest.download_here() {
        let bytes = http::download(&SERVICE, &download.url).map_err(|e| e.to_string())?;
        archive::verify(&bytes, &download.sha256)?;
        let count = archive::unpack(&bytes, &root)?;
        places.note(&id, "download unpacked", &[("files", &count)]);
    } else {
        build(&fetched, places)?;
    }
    if Manifest::read(&root)? != fetched.manifest {
        return Err(format!("installing changed {}; nothing was installed", manifest::FILE));
    }
    Manifest::load(&root)?;
    let folder = places.installs.join(&id);
    let dest = folder.join(short(&fetched.commit));
    private_dir(&folder).map_err(|e| format!("cannot make {}: {e}", folder.display()))?;
    let mut registry = Registry::open(&places.registry)?;
    conflict(&registry, &id, &fetched.origin)?;
    let previous = registry.entries.iter().find(|entry| entry.id == id).cloned();
    if previous.as_ref().is_none_or(|entry| entry.source.root() != dest) {
        let _ = fs::remove_dir_all(&dest);
        fs::rename(&root, &dest).map_err(|e| format!("cannot move it into {}: {e}", dest.display()))?;
    }
    let source = Source::Git {
        repo: fetched.origin.repo.clone(),
        url: fetched.origin.url.clone(),
        reference: fetched.origin.reference.clone(),
        subdir: fetched.origin.subdir.clone(),
        commit: fetched.commit.clone(),
        path: dest.clone(),
    };
    let enabled = previous.as_ref().is_some_and(|entry| entry.enabled);
    let entry = Entry { id: id.clone(), source, enabled };
    match registry.entries.iter_mut().find(|e| e.id == id) {
        Some(found) => *found = entry,
        None => registry.entries.push(entry),
    }
    registry.save().map_err(|e| format!("cannot save {}: {e}", places.registry.display()))?;
    let kept = previous.as_ref().map(|entry| entry.source.root().to_path_buf());
    if let Ok(folders) = fs::read_dir(&folder) {
        for old in folders.flatten().map(|e| e.path()) {
            if old != dest && Some(&old) != kept.as_ref() {
                let _ = fs::remove_dir_all(old);
            }
        }
    }
    let previous = previous.and_then(|entry| match entry.source {
        Source::Git { commit, .. } => Some(commit),
        Source::Link { .. } => None,
    });
    places.note(&id, "installed", &[("commit", &fetched.commit)]);
    Ok(Installed { manifest: fetched.manifest.clone(), enabled, previous })
}

fn build(fetched: &Fetched, places: &Places) -> Result<(), String> {
    let manifest = &fetched.manifest;
    let entry = Entry { id: manifest.id.clone(), source: Source::Link { path: fetched.root() }, enabled: false };
    let plugin = Plugin { entry, manifest: Ok(manifest.clone()) };
    for step in &manifest.build {
        let shown = shown(&step.command);
        places.note(&manifest.id, "build step started", &[("command", &shown)]);
        let child = places
            .command(&plugin, &step.command, &[], None)
            .and_then(|mut c| c.spawn())
            .map_err(|e| format!("cannot run `{shown}`: {e}"))?;
        let logs = format!("`cornercase plugin logs {}` says more", manifest.id);
        match run::finish(child, BUILD_WITHIN).map_err(|e| e.to_string())? {
            Some(status) if status.success() => {}
            Some(status) => return Err(format!("`{shown}` failed ({status}); {logs}")),
            None => return Err(format!("`{shown}` was stopped after {} min; {logs}", BUILD_WITHIN.as_secs() / 60)),
        }
    }
    Ok(())
}

pub(crate) fn remove(places: &Places, id: &str) -> Result<Removed, String> {
    remove_within(places, id, STOP_WAIT)
}

fn remove_within(places: &Places, id: &str, within: Duration) -> Result<Removed, String> {
    let save =
        |registry: &Registry| registry.save().map_err(|e| format!("cannot save {}: {e}", places.registry.display()));
    let mut registry = Registry::open(&places.registry)?;
    if registry.entry(id)?.enabled {
        registry.set_enabled(id, false)?;
        save(&registry)?;
    }
    let off = registry.entry(id)?.clone();
    let until = Instant::now() + within;
    supervise::exclude_spawns(places, id, until).map_err(|e| {
        format!(
            "{e}; it is turned off and still installed, nothing was deleted, run `cornercase plugin remove {id}` again"
        )
    })?;
    let left = stopped(places, id, until).map_err(|e| {
        format!("cannot tell whether `{id}` still runs: {e}; it is turned off and still installed, nothing was deleted")
    })?;
    if !left.is_empty() {
        let pids = left.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        return Err(format!(
            "`{id}` is turned off but still runs (pid {pids}) after {} s; it is still installed, run `cornercase plugin remove {id}` again",
            within.as_secs().max(1)
        ));
    }
    let mut registry = Registry::open(&places.registry)?;
    if registry.entry(id).ok() != Some(&off) {
        return Err(format!(
            "`{id}` changed while it was being removed; nothing was deleted, run `cornercase plugin remove {id}` again"
        ));
    }
    let installed = off.source.installed();
    registry.unlink(id);
    save(&registry)?;
    if !installed {
        return Ok(Removed::Unlinked);
    }
    let folder = places.installs.join(id);
    match fs::remove_dir_all(&folder) {
        Ok(()) => Ok(Removed::Deleted),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Removed::Deleted),
        Err(e) => Err(format!("removed from the list, but cannot delete {}: {e}", folder.display())),
    }
}

fn stopped(places: &Places, id: &str, until: Instant) -> Result<Vec<i32>, String> {
    loop {
        let left = supervise::running(places, id)?;
        if left.is_empty() || Instant::now() >= until {
            return Ok(left);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests;
