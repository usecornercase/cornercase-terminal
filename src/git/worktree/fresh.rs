use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use parking_lot::Mutex;

use super::{check, create_from, git, git_within};
use crate::error::{Error, Result};

pub(crate) const FETCH_WITHIN: Duration = Duration::from_secs(120);

const FETCH_TRIES: u32 = 5;
const RETRY_AFTER: Duration = Duration::from_millis(300);

static FETCHING: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Mutex::default);

fn one_at_a_time(repo: &Path) -> Arc<Mutex<()>> {
    Arc::clone(FETCHING.lock().entry(repo.to_path_buf()).or_default())
}

fn remote_name(repo: &Path) -> Option<String> {
    let remotes = crate::git::remote_urls(repo);
    remotes.iter().find(|(name, _)| name == "origin").or_else(|| remotes.first()).map(|(name, _)| name.clone())
}

fn fetch_once(repo: &Path, remote: &str) -> Result<()> {
    let out = git_within(repo, ["fetch", "--quiet", remote], FETCH_WITHIN)?
        .ok_or_else(|| Error::Git(format!("could not fetch {remote}: it took longer than {FETCH_WITHIN:?}")))?;
    check(out).map(|_| ()).map_err(|error| Error::Git(format!("could not fetch {remote}: {error}")))
}

fn collided(error: &Error) -> bool {
    let text = error.to_string();
    ["incorrect old value", "cannot lock ref", "Unable to create"].iter().any(|clash| text.contains(clash))
}

fn fetched(repo: &Path, remote: &str) -> Result<()> {
    let mut tries = 1;
    loop {
        match fetch_once(repo, remote) {
            Err(error) if collided(&error) && tries < FETCH_TRIES => {
                std::thread::sleep(RETRY_AFTER * tries);
                tries += 1;
            }
            other => return other,
        }
    }
}

fn has_ref(repo: &Path, name: &str) -> Result<bool> {
    Ok(git(repo, ["show-ref", "--verify", "--quiet", name])?.status.success())
}

fn remote_default(repo: &Path, remote: &str) -> Result<String> {
    let head = format!("refs/remotes/{remote}/HEAD");
    let out = git(repo, ["symbolic-ref", "--quiet", &head])?;
    if out.status.success() {
        let target = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if has_ref(repo, &target)? {
            return Ok(target);
        }
    }
    for branch in ["main", "master"] {
        let name = format!("refs/remotes/{remote}/{branch}");
        if has_ref(repo, &name)? {
            return Ok(name);
        }
    }
    Err(Error::Git(format!("cannot tell the default branch of {remote}: run `git remote set-head {remote} --auto`")))
}

fn local_default(repo: &Path) -> Result<String> {
    for branch in ["main", "master"] {
        let name = format!("refs/heads/{branch}");
        if has_ref(repo, &name)? {
            return Ok(name);
        }
    }
    Ok("HEAD".into())
}

fn resolved(repo: &Path) -> Result<String> {
    match remote_name(repo) {
        Some(remote) => {
            fetched(repo, &remote)?;
            remote_default(repo, &remote)
        }
        None => local_default(repo),
    }
}

pub(crate) fn create(repo: &Path, branch: &str, path: &Path) -> Result<()> {
    let turn = one_at_a_time(repo);
    let _held = turn.lock();
    create_from(repo, branch, path, &resolved(repo)?)
}

#[cfg(test)]
mod tests;
