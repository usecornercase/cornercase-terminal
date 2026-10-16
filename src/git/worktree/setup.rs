use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::agents::activity::Status;
use crate::process;

pub(crate) const NAME: &str = "cornercase-setup";

pub(crate) const TEARDOWN_WITHIN: Duration = Duration::from_mins(5);

const MAX_FAILURE: usize = 160;

const WRAPPER: &str = r#"trap : INT
CORNERCASE_ROOT="$4" CORNERCASE_WORKTREE="$PWD" CORNERCASE_BRANCH="$5" /bin/sh -ec "$1"
s=$?
trap - INT
if [ "$s" = 0 ]; then
  printf '\n\033[32m✓ setup finished\033[0m\n'
else
  printf '\n\033[1;31m✗ setup failed with exit status %s\033[0m\n' "$s"
fi
printf '%s\n' "$s" > "$2"
exec "$3"
"#;

#[derive(Debug)]
pub(crate) enum Pane {
    Running { status: PathBuf, since: Instant },
    Failed { code: String, unseen: bool, blame: bool },
}

impl Pane {
    pub(crate) fn running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub(crate) fn status(&self) -> Option<Status> {
        matches!(self, Self::Failed { unseen: true, .. }).then_some(Status::Failed)
    }
}

pub(crate) fn args(script: &str, status: &Path, shell: &str, root: &Path, branch: &str) -> Vec<String> {
    vec![
        "-c".into(),
        WRAPPER.into(),
        NAME.into(),
        script.into(),
        status.display().to_string(),
        shell.into(),
        root.display().to_string(),
        branch.into(),
    ]
}

pub(crate) fn status_file(dir: &Path, pane: u64) -> PathBuf {
    dir.join(format!("setup-{pane}.status"))
}

pub(crate) fn read_status(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub(crate) fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("setup-") && name.ends_with(".status") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn teardown(script: &str, worktree: &Path, root: &Path, branch: &str, within: Duration) -> Option<String> {
    if !worktree.is_dir() {
        return None;
    }
    let mut command = Command::new("/bin/sh");
    command
        .args(["-ec", script])
        .current_dir(worktree)
        .env("CORNERCASE_ROOT", root)
        .env("CORNERCASE_WORKTREE", worktree)
        .env("CORNERCASE_BRANCH", branch);
    match process::output_within(&mut command, within) {
        Err(e) => Some(format!("could not start: {e}")),
        Ok(None) => Some(format!("did not finish within {} minutes", within.as_secs().div_ceil(60))),
        Ok(Some(out)) if out.status.success() => None,
        Ok(Some(out)) => {
            let code = out.status.code().map_or_else(|| "a signal".to_string(), |c| format!("exit status {c}"));
            let last = |bytes: &[u8]| {
                String::from_utf8_lossy(bytes).lines().rev().map(str::trim).find(|l| !l.is_empty()).map(String::from)
            };
            let line = last(&out.stderr).or_else(|| last(&out.stdout));
            Some(match line {
                Some(line) => format!("{code}: {}", crate::text::truncate_right(&line, MAX_FAILURE)),
                None => code,
            })
        }
    }
}

#[cfg(test)]
mod tests;
