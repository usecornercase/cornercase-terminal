use std::path::{Path, PathBuf};

use crate::worktree;

pub fn check(repo: &Path, workspaces: &[(u64, PathBuf)], fetch: bool) -> Vec<(u64, u32)> {
    if fetch {
        let _ = worktree::git(repo, ["fetch", "--all", "--quiet"]);
    }
    workspaces.iter().map(|(id, path)| (*id, behind(path).unwrap_or(0))).collect()
}

pub fn behind(dir: &Path) -> Option<u32> {
    let out = worktree::git(dir, ["rev-list", "--count", "HEAD..@{upstream}"]).ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, git, git_repo};

    struct Clone {
        remote: TempDir,
        _tmp: TempDir,
        path: PathBuf,
    }

    fn clone() -> Clone {
        let remote = git_repo(&[("README", "hi")]);
        let (tmp, path) = crate::test_util::clone_of(remote.path());
        Clone { remote, _tmp: tmp, path }
    }

    fn commit(dir: &Path) {
        git(dir, &["commit", "--quiet", "--allow-empty", "-m", "more"]);
    }

    #[test]
    fn counts_the_commits_on_the_remote_after_a_fetch() {
        let c = clone();
        commit(c.remote.path());
        commit(c.remote.path());

        assert_eq!(check(&c.path, &[(1, c.path.clone())], true), [(1, 2)]);
    }

    #[test]
    fn does_not_see_new_commits_without_a_fetch() {
        let c = clone();
        commit(c.remote.path());

        assert_eq!(check(&c.path, &[(1, c.path.clone())], false), [(1, 0)]);
    }

    #[test]
    fn is_zero_after_a_pull() {
        let c = clone();
        commit(c.remote.path());
        check(&c.path, &[], true);

        git(&c.path, &["pull", "--quiet", "--ff-only"]);

        assert_eq!(behind(&c.path), Some(0));
    }

    #[test]
    fn one_fetch_serves_every_worktree_of_the_repo() {
        let c = clone();
        let tmp = TempDir::new();
        let wt = tmp.path().join("wt");
        git(&c.path, &["worktree", "add", "--quiet", "--track", "-b", "wt", &wt.display().to_string(), "origin/main"]);
        commit(c.remote.path());

        assert_eq!(check(&c.path, &[(1, c.path.clone()), (2, wt)], true), [(1, 1), (2, 1)]);
    }

    #[test]
    fn a_branch_without_upstream_has_nothing_to_pull() {
        let repo = git_repo(&[]);

        assert_eq!((behind(repo.path()), check(repo.path(), &[(1, repo.path().into())], true)), (None, vec![(1, 0)]));
    }

    #[test]
    fn a_folder_outside_git_has_nothing_to_pull() {
        let tmp = TempDir::new();

        assert_eq!(check(tmp.path(), &[(1, tmp.path().into())], true), [(1, 0)]);
    }
}
