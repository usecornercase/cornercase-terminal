#![cfg(unix)]

use super::issue_list::{checkout, listing, start};
use super::*;
use crate::test_util::{clone_of, commit, git, head};

struct Origin {
    _dir: TempDir,
    behind: String,
    ahead: String,
}

fn origin_ahead_of(repo: &Path) -> Origin {
    let (dir, origin) = clone_of(repo);
    git(repo, &["remote", "add", "origin", &origin.display().to_string()]);
    Origin { behind: head(repo), ahead: commit(&origin, "news", "x"), _dir: dir }
}

struct Dialog {
    app: App,
    rx: Receiver<AppEvent>,
    project: PathBuf,
    ahead: String,
    _dirs: (TempDir, TempDir, TempDir, TempDir),
}

fn dialog_on_a_clone(broken_origin: bool) -> Dialog {
    let origin = git_repo(&[("README", "hi")]);
    let (keep, project) = clone_of(origin.path());
    let ahead = commit(origin.path(), "news", "x");
    if broken_origin {
        git(&project, &["remote", "set-url", "origin", "/nonexistent/folder"]);
    }
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let settings =
        Config { worktrees_dir: worktrees.path().display().to_string(), fresh_branches: true, ..Config::default() };
    let (mut app, rx) = app_in(&project, write_config(config.path(), &settings));
    click_new_workspace(&mut app);
    type_text(&mut app, "feat/login");
    Dialog { app, rx, project, ahead, _dirs: (origin, keep, worktrees, config) }
}

#[test]
fn the_new_workspace_dialog_follows_the_setting() {
    let mut dialog = dialog_on_a_clone(false);

    click(&mut dialog.app, form_button(CREATE_SUBMIT, 0));

    pump_until(&mut dialog.app, &dialog.rx, "the workspace opens", |a| a.projects[0].workspaces.len() == 2);
    assert_eq!(head(&dialog.app.projects[0].workspaces[1].path), dialog.ahead);
}

#[test]
fn the_new_workspace_dialog_shows_a_failed_fetch_and_makes_nothing() {
    let mut dialog = dialog_on_a_clone(true);

    send_key(&mut dialog.app, KeyCode::Enter, KeyModifiers::NONE);

    pump_until(&mut dialog.app, &dialog.rx, "the fetch fails", |a| {
        matches!(&a.overlay, Some(Overlay::NewWorkspace { error: Some(_), .. }))
    });
    let Some(Overlay::NewWorkspace { error: Some(error), .. }) = &dialog.app.overlay else { panic!("no error") };
    assert!(error.contains("could not fetch origin"), "{error}");
    let made = (dialog.app.projects[0].workspaces.len(), branch_exists(&dialog.project, "feat/login"));
    assert_eq!(made, (1, false));
}

#[test]
fn the_issues_dialog_start_follows_the_setting() {
    let mut s = listing();
    let repo = s.app.projects[0].path.clone();
    let origin = origin_ahead_of(&repo);
    s.app.config.fresh_branches = true;

    start(&mut s);

    let made = checkout(&s).canonicalize().expect("checkout");
    assert_eq!((head(&made), head(&repo)), (origin.ahead, origin.behind));
}
