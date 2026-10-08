use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::error::{Error, Result};
use crate::log;
use crate::process;

pub const INCLUDE_FILE: &str = ".worktreeinclude";
const DEFAULT_SLUG: &str = "worktree";

pub fn slug(branch: &str) -> String {
    let mut slug = String::new();
    for c in branch.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() { DEFAULT_SLUG.into() } else { slug.into() }
}

pub fn checkout_path(worktrees_dir: &Path, repo: &Path, branch: &str) -> PathBuf {
    let name = repo.file_name().unwrap_or(repo.as_os_str());
    worktrees_dir.join(name).join(slug(branch))
}

pub fn create(repo: &Path, branch: &str, path: &Path) -> Result<()> {
    if path.exists() {
        return Err(Error::PathExists(path.to_path_buf()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path = path.as_os_str();
    if branch_exists(repo, branch)? {
        check(git(repo, [OsStr::new("worktree"), "add".as_ref(), path, branch.as_ref()])?)?;
    } else {
        check(git(
            repo,
            [OsStr::new("worktree"), "add".as_ref(), "-b".as_ref(), branch.as_ref(), path, "HEAD".as_ref()],
        )?)?;
    }
    for file in included_files(repo)? {
        if let Err(e) = copy(&repo.join(&file), &Path::new(path).join(&file)) {
            log::warning!("worktree", "failed to copy a file into the worktree", file = file.display(), error = e);
        }
    }
    Ok(())
}

pub fn remove(repo: &Path, path: &Path, force: bool, unlock: bool) -> Result<()> {
    if unlock && !matches!(lock(path), Ok(None)) {
        check(git(repo, [OsStr::new("worktree"), "unlock".as_ref(), path.as_os_str()])?)?;
    }
    let mut args = vec![OsStr::new("worktree"), "remove".as_ref()];
    if force {
        args.push("--force".as_ref());
    }
    args.push(path.as_os_str());
    check(git(repo, args)?).map(drop)
}

pub fn changed(path: &Path) -> Result<bool> {
    check(git(path, ["--no-optional-locks", "status", "--porcelain"])?).map(|out| !out.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub reason: String,
    pub holder: Option<Holder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Running(i32),
    Gone(i32),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub changed: bool,
    pub lock: Option<Lock>,
}

pub fn status(path: &Path) -> Status {
    Status { changed: !matches!(changed(path), Ok(false)), lock: lock(path).ok().flatten() }
}

pub fn lock(path: &Path) -> Result<Option<Lock>> {
    let out = check(git(path, ["rev-parse", "--absolute-git-dir"])?)?;
    let dir = Path::new(OsStr::from_bytes(out.trim_ascii_end())).join("locked");
    let reason = match std::fs::read_to_string(dir) {
        Ok(reason) => reason.trim().to_string(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let holder = pid_in(&reason).map(|pid| if process::alive(pid) { Holder::Running(pid) } else { Holder::Gone(pid) });
    Ok(Some(Lock { reason, holder }))
}

fn pid_in(reason: &str) -> Option<i32> {
    let (_, after) = reason.split_once("pid ")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub fn git<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(repo: &Path, args: I) -> Result<Output> {
    command(repo, args).stdin(Stdio::null()).output().map_err(Error::RunGit)
}

fn command<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(repo: &Path, args: I) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS_REQUIRE", "never");
    cmd
}

pub fn check(out: Output) -> Result<Vec<u8>> {
    if out.status.success() {
        return Ok(out.stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let fatal =
        lines.iter().rev().find_map(|line| line.strip_prefix("fatal: ").or_else(|| line.strip_prefix("error: ")));
    let message = fatal
        .or_else(|| lines.iter().rev().find(|line| !line.starts_with("hint:")).copied())
        .map_or_else(|| format!("git failed with {}", out.status), str::to_string);
    Err(Error::Git(message))
}

fn branch_exists(repo: &Path, branch: &str) -> Result<bool> {
    let out = git(repo, ["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => check(out).map(|_| false),
    }
}

fn included_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let include = repo.join(INCLUDE_FILE);
    if !include.is_file() {
        return Ok(Vec::new());
    }
    let mut exclude_from = OsString::from("--exclude-from=");
    exclude_from.push(&include);
    let matching = check(git(
        repo,
        [OsStr::new("ls-files"), "--others".as_ref(), "--ignored".as_ref(), "-z".as_ref(), &exclude_from],
    )?)?;
    if matching.is_empty() {
        return Ok(Vec::new());
    }
    let ignored = ignored_by_git(repo, matching)?;
    Ok(ignored.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| PathBuf::from(OsStr::from_bytes(p))).collect())
}

fn ignored_by_git(repo: &Path, paths: Vec<u8>) -> Result<Vec<u8>> {
    let mut child = command(repo, ["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::RunGit)?;
    let mut stdin = child.stdin.take().ok_or_else(|| Error::Git("git check-ignore has no stdin".into()))?;
    let writer = std::thread::spawn(move || stdin.write_all(&paths));
    let out = child.wait_with_output().map_err(Error::RunGit)?;
    let _ = writer.join();
    if out.status.code() == Some(1) && out.stdout.is_empty() {
        return Ok(Vec::new());
    }
    check(out)
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to).map(drop)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_util::{TempDir, git, git_repo};

    fn read(path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn current_branch(dir: &Path) -> Option<String> {
        crate::git::branch(dir)
    }

    mod slug {
        use super::*;

        #[rstest]
        #[case::plain("login", "login")]
        #[case::slashes("feat/Login", "feat-login")]
        #[case::runs_of_symbols("fix//the  bug!", "fix-the-bug")]
        #[case::trims_dashes("-x-", "x")]
        #[case::nothing_left("///", "worktree")]
        fn turns_a_branch_into_a_folder_name(#[case] branch: &str, #[case] expected: &str) {
            assert_eq!(slug(branch), expected);
        }
    }

    #[test]
    fn checkout_path_goes_under_the_repo_name() {
        let path = checkout_path(Path::new("/w"), Path::new("/home/ana/projects/cornercase"), "feat/login");
        assert_eq!(path, PathBuf::from("/w/cornercase/feat-login"));
    }

    mod create {
        use super::*;

        #[test]
        fn checks_out_a_new_branch() {
            let repo = git_repo(&[("README", "hi")]);
            let out = TempDir::new();
            let path = out.path().join("repo/login");

            create(repo.path(), "feat/login", &path).expect("create worktree");

            assert_eq!(
                (current_branch(&path).as_deref(), read(&path.join("README")).as_deref()),
                (Some("feat/login"), Some("hi"))
            );
        }

        #[test]
        fn checks_out_an_existing_branch() {
            let repo = git_repo(&[("README", "hi")]);
            git(repo.path(), &["branch", "already-there"]);
            let out = TempDir::new();
            let path = out.path().join("wt");

            create(repo.path(), "already-there", &path).expect("create worktree");

            assert_eq!(current_branch(&path).as_deref(), Some("already-there"));
        }

        #[test]
        fn copies_ignored_files_listed_in_worktreeinclude() {
            let repo = git_repo(&[
                (".gitignore", ".env\n.envs/\nsecret.txt\n"),
                (INCLUDE_FILE, ".env\n.envs/.env.local\n"),
                (".env", "TOKEN=1"),
                (".envs/.env.local", "LOCAL=1"),
                ("secret.txt", "not listed"),
            ]);
            let out = TempDir::new();
            let path = out.path().join("wt");

            create(repo.path(), "copy", &path).expect("create worktree");

            assert_eq!(
                (read(&path.join(".env")), read(&path.join(".envs/.env.local")), read(&path.join("secret.txt"))),
                (Some("TOKEN=1".into()), Some("LOCAL=1".into()), None)
            );
        }

        #[test]
        fn leaves_untracked_files_that_are_not_ignored() {
            let repo = git_repo(&[(INCLUDE_FILE, "notes.md\n")]);
            std::fs::write(repo.path().join("notes.md"), "draft").expect("write untracked file");
            let out = TempDir::new();
            let path = out.path().join("wt");

            create(repo.path(), "untracked", &path).expect("create worktree");

            assert_eq!(read(&path.join("notes.md")), None);
        }

        #[test]
        fn refuses_a_path_that_exists() {
            let repo = git_repo(&[]);
            let out = TempDir::new();

            let err = create(repo.path(), "x", out.path()).expect_err("path exists");

            assert!(matches!(err, Error::PathExists(_)), "{err:?}");
        }

        #[test]
        fn reports_what_git_says() {
            let repo = git_repo(&[]);
            let out = TempDir::new();

            let err = create(repo.path(), "not a branch", &out.path().join("wt")).expect_err("invalid branch");

            assert_eq!(err.to_string(), "'not a branch' is not a valid branch name");
        }
    }

    mod pid_in {
        use super::*;

        #[rstest]
        #[case::claude_code("claude session wt (pid 69184 start Wed Oct  7 09:10:42 2026)", Some(69184))]
        #[case::no_pid("on a usb disk", None)]
        #[case::pid_without_a_number("pid abc", None)]
        #[case::empty("", None)]
        fn finds_the_pid_a_reason_names(#[case] reason: &str, #[case] expected: Option<i32>) {
            assert_eq!(pid_in(reason), expected);
        }
    }

    mod remove {
        use super::*;
        use crate::test_util::{exited_pid, this_pid};

        fn with_worktree() -> (TempDir, TempDir, PathBuf) {
            let repo = git_repo(&[("README", "hi")]);
            let tmp = TempDir::new();
            let path = tmp.path().join("wt");
            create(repo.path(), "wt", &path).expect("create worktree");
            (repo, tmp, path)
        }

        fn locked(repo: &Path, path: &Path, reason: Option<&str>) {
            let path = path.display().to_string();
            match reason {
                Some(reason) => git(repo, &["worktree", "lock", "--reason", reason, &path]),
                None => git(repo, &["worktree", "lock", &path]),
            }
        }

        #[test]
        fn an_unlocked_checkout_has_no_lock() {
            let (_repo, _tmp, path) = with_worktree();

            assert_eq!(lock(&path).expect("read lock"), None);
        }

        #[test]
        fn lock_gives_the_reason() {
            let (repo, _tmp, path) = with_worktree();
            locked(repo.path(), &path, Some("on a usb disk"));

            assert_eq!(lock(&path).expect("read lock"), Some(Lock { reason: "on a usb disk".into(), holder: None }));
        }

        #[test]
        fn lock_without_a_reason_has_an_empty_one() {
            let (repo, _tmp, path) = with_worktree();
            locked(repo.path(), &path, None);

            assert_eq!(lock(&path).expect("read lock"), Some(Lock { reason: String::new(), holder: None }));
        }

        #[test]
        fn a_pid_in_the_reason_that_is_gone_is_a_lock_left_behind() {
            let (repo, _tmp, path) = with_worktree();
            let pid = exited_pid();
            locked(repo.path(), &path, Some(&format!("claude session wt (pid {pid} start Wed Oct  7 09:10:42 2026)")));

            assert_eq!(lock(&path).expect("read lock").and_then(|l| l.holder), Some(Holder::Gone(pid)));
        }

        #[test]
        fn a_pid_in_the_reason_that_runs_holds_the_lock() {
            let (repo, _tmp, path) = with_worktree();
            let pid = this_pid();
            locked(repo.path(), &path, Some(&format!("claude session wt (pid {pid} start …)")));

            assert_eq!(lock(&path).expect("read lock").and_then(|l| l.holder), Some(Holder::Running(pid)));
        }

        #[test]
        fn status_reports_changes_and_the_lock_together() {
            let (repo, _tmp, path) = with_worktree();
            std::fs::write(path.join("notes.txt"), "draft").expect("write file");
            locked(repo.path(), &path, Some("busy"));

            let status = status(&path);

            assert_eq!((status.changed, status.lock.map(|l| l.reason)), (true, Some("busy".into())));
        }

        #[test]
        fn a_locked_checkout_is_refused_without_unlock() {
            let (repo, _tmp, path) = with_worktree();
            locked(repo.path(), &path, Some("busy"));

            let result = remove(repo.path(), &path, true, false);

            assert!(matches!(result, Err(Error::Git(_))) && path.exists(), "{result:?}");
        }

        #[test]
        fn unlock_removes_a_locked_checkout_and_keeps_the_branch() {
            let (repo, _tmp, path) = with_worktree();
            locked(repo.path(), &path, Some("busy"));

            remove(repo.path(), &path, false, true).expect("remove");

            assert_eq!((path.exists(), branch_exists(repo.path(), "wt").expect("show-ref")), (false, true));
        }

        #[test]
        fn unlock_removes_a_checkout_whose_lock_is_already_gone() {
            let (repo, _tmp, path) = with_worktree();

            remove(repo.path(), &path, false, true).expect("remove");

            assert_eq!((path.exists(), branch_exists(repo.path(), "wt").expect("show-ref")), (false, true));
        }

        #[test]
        fn deletes_a_clean_checkout_and_keeps_the_branch() {
            let (repo, _tmp, path) = with_worktree();

            remove(repo.path(), &path, false, false).expect("remove");

            assert_eq!((path.exists(), branch_exists(repo.path(), "wt").expect("show-ref")), (false, true));
        }

        #[test]
        fn refuses_a_checkout_with_changes() {
            let (repo, _tmp, path) = with_worktree();
            std::fs::write(path.join("README"), "changed").expect("edit file");

            let result = remove(repo.path(), &path, false, false);

            assert!(matches!(result, Err(Error::Git(_))) && path.exists(), "{result:?}");
        }

        #[test]
        fn force_deletes_a_checkout_with_changes() {
            let (repo, _tmp, path) = with_worktree();
            std::fs::write(path.join("README"), "changed").expect("edit file");

            remove(repo.path(), &path, true, false).expect("remove");

            assert!(!path.exists());
        }

        #[rstest]
        #[case::nothing(None, false)]
        #[case::an_edited_file(Some("README"), true)]
        #[case::a_new_file(Some("notes.txt"), true)]
        fn changes_are_what_makes_git_refuse(#[case] written: Option<&str>, #[case] expected: bool) {
            let (_repo, _tmp, path) = with_worktree();
            if let Some(name) = written {
                std::fs::write(path.join(name), "changed").expect("write file");
            }

            assert_eq!(changed(&path).expect("git status"), expected);
        }
    }
}
