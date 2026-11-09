use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::agents::activity::Status;
use crate::process;
use crate::script::{self, Line, Sh, Shell};

pub(crate) const NAME: &str = "cornercase-setup";

pub(crate) const TEARDOWN_WITHIN: Duration = Duration::from_mins(5);

const MAX_FAILURE: usize = 160;

const RUN: &str = r#"CORNERCASE_ROOT="$4" CORNERCASE_WORKTREE="$PWD" CORNERCASE_BRANCH="$5" /bin/sh -ec "$1""#;

const GIT_RUN: &str =
    r#"CORNERCASE_ROOT="$4" CORNERCASE_WORKTREE="$6" CORNERCASE_BRANCH="$5" PATH="/usr/bin:$PATH" /bin/sh -ec "$1""#;

const GIT_SHELL: &str = r#"unset PWD SHLVL
[ -n "$7" ] || unset HOME
cornercase_shell=$3
shift 7
"$cornercase_shell" "$@"
"#;

const SH_SHELL: &str = "cornercase_shell=$3\nshift 5\nexec \"$cornercase_shell\" \"$@\"\n";

const REPORT: &str = r#"if [ "$s" = 0 ]; then
  printf '\n\033[32m✓ setup finished\033[0m\n'
else
  printf '\n\033[1;31m✗ setup failed with exit status %s\033[0m\n' "$s"
fi
printf '%s\n' "$s" > "$2"
"#;

const POWERSHELL_WRAPPER: &str = r#"try {
    Set-Location -LiteralPath $cornercase.Worktree
    $env:CORNERCASE_ROOT = $cornercase.Root
    $env:CORNERCASE_WORKTREE = $cornercase.Worktree
    $env:CORNERCASE_BRANCH = $cornercase.Branch
    & $cornercase.Shell -NoProfile -EncodedCommand $cornercase.Script
    $cornercase.Code = $LASTEXITCODE
} finally {
    Remove-Item Env:CORNERCASE_ROOT, Env:CORNERCASE_WORKTREE, Env:CORNERCASE_BRANCH -ErrorAction Ignore
    $cornercase.Escape = [char]27
    if ($cornercase.Code -eq 0) {
        Write-Host ("`n" + $cornercase.Escape + '[32m' + [char]0x2713 + ' setup finished' + $cornercase.Escape + '[0m')
    } else {
        $cornercase.Said = ' setup failed with exit status ' + $cornercase.Code
        Write-Host ("`n" + $cornercase.Escape + '[1;31m' + [char]0x2717 + $cornercase.Said + $cornercase.Escape + '[0m')
    }
    Set-Content -LiteralPath $cornercase.Status -Value $cornercase.Code
    Remove-Variable cornercase
}
"#;

pub(crate) struct Wrapper<'a> {
    pub(crate) script: &'a str,
    pub(crate) status: &'a Path,
    pub(crate) shell: &'a str,
    pub(crate) shell_args: &'a [String],
    pub(crate) root: &'a Path,
    pub(crate) branch: &'a str,
    pub(crate) worktree: &'a Path,
    pub(crate) home: bool,
}

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

pub(crate) fn line(interpreter: &Shell, wrapper: &Wrapper) -> Line {
    match interpreter {
        Shell::Sh(sh) => through_sh(sh, wrapper),
        Shell::PowerShell(program) => powershell(program, wrapper),
    }
}

fn through_sh(sh: &Sh, wrapper: &Wrapper) -> Line {
    let mut args = vec![
        NAME.into(),
        wrapper.script.into(),
        wrapper.status.display().to_string(),
        wrapper.shell.into(),
        wrapper.root.display().to_string(),
        wrapper.branch.into(),
    ];
    if !sh.git {
        args.extend(wrapper.shell_args.iter().cloned());
        return sh.line("-c", &format!("trap : INT\n{RUN}\ns=$?\ntrap - INT\n{REPORT}{SH_SHELL}"), &args);
    }
    args.extend([wrapper.worktree.display().to_string(), if wrapper.home { "1" } else { "" }.into()]);
    args.extend(wrapper.shell_args.iter().cloned());
    sh.line("-c", &format!("trap : INT\n{GIT_RUN}\ns=$?\n{REPORT}{GIT_SHELL}"), &args)
}

fn powershell(program: &str, wrapper: &Wrapper) -> Line {
    let values = [
        ("Shell", program.to_string()),
        ("Script", script::encoded(&script::strict(wrapper.script))),
        ("Status", wrapper.status.display().to_string()),
        ("Root", wrapper.root.display().to_string()),
        ("Worktree", wrapper.worktree.display().to_string()),
        ("Branch", wrapper.branch.to_string()),
    ];
    let fields: Vec<String> = values.iter().map(|(key, value)| format!("{key} = {}", script::literal(value))).collect();
    let text = format!("$cornercase = @{{ Code = 1; {} }}\n{POWERSHELL_WRAPPER}", fields.join("; "));
    Line { program: program.to_string(), args: vec!["-NoExit".into(), "-Command".into(), text], env: Vec::new() }
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
    let mut command = Shell::here().run(script).command();
    command
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
