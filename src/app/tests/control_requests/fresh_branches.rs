#![cfg(unix)]

use super::*;
use crate::test_util::{clone_of, commit, git, head};

struct Clones {
    app: App,
    rx: Receiver<AppEvent>,
    clone: PathBuf,
    behind: String,
    ahead: String,
    _dirs: (TempDir, TempDir, TempDir, TempDir),
}

fn behind(setting: bool) -> Clones {
    let origin = git_repo(&[("README", "hi")]);
    let (keep, clone) = clone_of(origin.path());
    let behind = head(&clone);
    let ahead = commit(origin.path(), "news", "x");
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let settings = Config {
        worktrees_dir: worktrees.path().display().to_string(),
        agent_commands: [("mine".to_string(), "/bin/true".to_string())].into(),
        fresh_branches: setting,
        ..Config::default()
    };
    let (app, rx) = app_in(&clone, write_config(config.path(), &settings));
    Clones { app, rx, clone, behind, ahead, _dirs: (origin, keep, worktrees, config) }
}

fn new_workspace(fresh: Option<bool>) -> Command {
    Command::NewWorkspace(wire::NewWorkspace { name: "feat/x".into(), worktree: true, fresh, ..Default::default() })
}

impl Clones {
    fn made(&mut self, command: Command) -> String {
        ask(&mut self.app, None, command);
        let made = done(answered(&mut self.app, &self.rx, "git makes the worktree"));
        let id = made.ids.workspace.expect("the workspace");
        let workspace = self.app.projects[0].workspaces.iter().find(|w| w.id == id).expect("it");
        head(&workspace.path)
    }

    fn untouched(&self) {
        assert_eq!(head(&self.clone), self.behind);
    }
}

#[test]
fn a_new_workspace_with_fresh_starts_from_the_newest_commit_of_origin() {
    let mut world = behind(false);

    let started = world.made(new_workspace(Some(true)));

    assert_eq!(started, world.ahead);
    world.untouched();
}

#[test]
fn without_fresh_a_new_branch_starts_from_head_as_before() {
    let mut world = behind(false);

    let started = world.made(new_workspace(None));

    assert_eq!(started, world.behind);
}

#[test]
fn the_setting_makes_a_request_without_the_field_fresh() {
    let mut world = behind(true);

    let started = world.made(new_workspace(None));

    assert_eq!(started, world.ahead);
    world.untouched();
}

#[test]
fn no_fresh_overrides_the_setting() {
    let mut world = behind(true);

    let started = world.made(new_workspace(Some(false)));

    assert_eq!(started, world.behind);
}

#[test]
fn a_failing_fetch_fails_the_request_and_leaves_nothing() {
    let mut world = behind(false);
    git(&world.clone, &["remote", "set-url", "origin", "/nonexistent/folder"]);

    ask(&mut world.app, None, new_workspace(Some(true)));
    let failed = error(answered(&mut world.app, &world.rx, "the fetch fails"));

    assert!(failed.contains("could not fetch origin"), "{failed}");
    assert_eq!(world.app.projects[0].workspaces.len(), 1);
    assert!(!branch_exists(&world.clone, "feat/x"));
}

#[test]
fn an_existing_branch_is_checked_out_without_a_fetch() {
    let mut world = behind(false);
    git(&world.clone, &["branch", "feat/x"]);
    git(&world.clone, &["remote", "set-url", "origin", "/nonexistent/folder"]);

    let started = world.made(new_workspace(Some(true)));

    assert_eq!(started, world.behind);
}

#[test]
fn a_start_with_fresh_makes_its_worktree_from_origin() {
    let mut world = behind(false);
    let start = wire::Start {
        agent: Some("mine".into()),
        worktree: Some("feat/x".into()),
        fresh: Some(true),
        placed: true,
        ..wire::Start::default()
    };

    let started = world.made(Command::Start(start));

    assert_eq!(started, world.ahead);
}
