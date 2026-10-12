use super::*;
use crate::test_util::git;

fn changed_workspace(app: &mut App, rx: &Receiver<AppEvent>) -> (u64, PathBuf) {
    let (id, path) = made(app, rx);
    std::fs::write(path.join("notes.txt"), "draft").expect("write file");
    (id, path)
}

fn locked_workspace(app: &mut App, rx: &Receiver<AppEvent>, repo: &Path) -> (u64, PathBuf) {
    let (id, path) = made(app, rx);
    lock(repo, &path);
    (id, path)
}

fn removing_workspace(app: &mut App, rx: &Receiver<AppEvent>) -> u64 {
    let (id, _) = made(app, rx);
    app.start_removal(app.projects[0].id, id, false, false, None);
    id
}

fn repo() -> (App, Receiver<AppEvent>, TempDir, TempDir, TempDir) {
    let repo = git_repo(&[("README", "hi")]);
    let (worktrees, config, config_path) = with_worktrees_config();
    let (app, rx) = app_in(repo.path(), config_path);
    (app, rx, repo, worktrees, config)
}

fn worktree(name: &str) -> Command {
    Command::NewWorkspace(wire::NewWorkspace {
        name: name.into(),
        worktree: true,
        ..wire::NewWorkspace::default()
    })
}

#[test]
fn a_worktree_opens_with_a_tab_and_leaves_the_window_as_it_is() {
    let (mut app, rx, _repo, worktrees, _config) = repo();
    let before = app.focus();

    ask(&mut app, None, worktree("feat/x"));
    let made = done(answered(&mut app, &rx, "git makes the worktree"));

    let workspace =
        app.projects[0].workspaces.iter().find(|w| Some(w.id) == made.ids.workspace).expect("it");
    assert_eq!((workspace.worktree, workspace.tabs.len(), app.focus()), (true, 1, before));
    assert!(workspace.path.starts_with(worktrees.path()), "{}", workspace.path.display());
}

fn made(app: &mut App, rx: &Receiver<AppEvent>) -> (u64, PathBuf) {
    ask(app, None, worktree("feat/x"));
    let id = done(answered(app, rx, "git makes the worktree")).ids.workspace.expect("the workspace");
    let (p, w) = app.workspace_position(id).expect("it");
    (id, app.projects[p].workspaces[w].path.clone())
}

fn remove(id: u64) -> Command {
    Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: false })
}

#[test]
fn removing_it_deletes_the_checkout() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = made(&mut app, &rx);

    ask(&mut app, None, remove(id));
    done(answered(&mut app, &rx, "git removes the worktree"));

    pump_until(&mut app, &rx, "the workspace goes", |a| a.workspace_position(id).is_none());
    assert!(!path.exists());
}

#[test]
fn a_worktree_already_being_removed_is_refused() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let id = removing_workspace(&mut app, &rx);

    let message = error(now(&mut app, None, remove(id)));

    assert!(message.contains("is being removed"), "{message}");
}

#[test]
fn no_tab_opens_in_a_worktree_being_removed() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let id = removing_workspace(&mut app, &rx);

    let new = wire::NewTab { workspace: Some(id), ..wire::NewTab::default() };
    let message = error(now(&mut app, None, Command::NewTab(new)));

    assert!(message.contains("is being removed"), "{message}");
}

#[test]
fn a_removal_that_starts_while_git_status_runs_is_not_started_twice() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, _) = made(&mut app, &rx);
    ask(&mut app, None, remove(id));

    app.start_removal(app.projects[0].id, id, false, false, None);

    let message = error(answered(&mut app, &rx, "git status answers"));
    assert!(message.contains("is being removed"), "{message}");
}

#[test]
fn a_worktree_with_changes_is_refused_before_anything_stops() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = changed_workspace(&mut app, &rx);

    ask(&mut app, None, remove(id));
    let message = error(answered(&mut app, &rx, "git status answers"));

    let (p, w) = app.workspace_position(id).expect("it stays");
    let workspace = &app.projects[p].workspaces[w];
    assert!(message.contains("--force"), "{message}");
    assert_eq!((workspace.removing(), workspace.tabs.len(), path.exists()), (false, 1, true));
}

fn force_remove(id: u64) -> Command {
    Command::Close(wire::Close { item: Item::Workspace(id), remove_worktree: true, force: true })
}

fn lock(repo: &Path, path: &Path) {
    git(repo, &["worktree", "lock", "--reason", "on a usb disk", &path.display().to_string()]);
}

fn untouched(app: &App, id: u64, path: &Path) -> bool {
    let (p, w) = app.workspace_position(id).expect("it stays");
    let workspace = &app.projects[p].workspaces[w];
    workspace.open() && !workspace.tabs.is_empty() && path.exists()
}

#[test]
fn a_locked_worktree_is_refused_before_anything_stops() {
    let (mut app, rx, repo, _worktrees, _config) = repo();
    let (id, path) = locked_workspace(&mut app, &rx, repo.path());

    ask(&mut app, None, remove(id));
    let message = error(answered(&mut app, &rx, "git status answers"));

    assert!(
        message.contains("is locked (on a usb disk)") && message.contains("git worktree unlock"),
        "{message}"
    );
    assert!(untouched(&app, id, &path));
}

#[test]
fn force_does_not_unlock_a_locked_worktree() {
    let (mut app, rx, repo, _worktrees, _config) = repo();
    let (id, path) = locked_workspace(&mut app, &rx, repo.path());

    ask(&mut app, None, force_remove(id));
    let message = error(answered(&mut app, &rx, "git status answers"));

    assert!(message.contains("is locked"), "{message}");
    assert!(untouched(&app, id, &path));
}

#[test]
fn force_removes_a_worktree_with_changes() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = changed_workspace(&mut app, &rx);

    ask(&mut app, None, force_remove(id));
    done(answered(&mut app, &rx, "git removes the worktree"));

    assert!(!path.exists());
}

#[test]
fn only_a_repository_root_has_worktrees() {
    let (mut app, _rx, _dirs) = app_with(1);

    let message = error(now(&mut app, None, worktree("feat/x")));

    assert!(message.contains("is not the root of a git repository"), "{message}");
}
