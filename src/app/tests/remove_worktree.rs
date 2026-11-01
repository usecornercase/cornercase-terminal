use super::*;
use crate::config::ProjectSettings;
use crate::test_util::{exited_pid, git};

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    path: PathBuf,
    _worktrees: TempDir,
    _config: TempDir,
}

fn with_worktree() -> Setup {
    let mut s = opened();
    ask(&mut s);
    s
}

fn opened() -> Setup {
    let repo = git_repo(&[("README", "hi")]);
    let (worktrees, config, config_path) = with_worktrees_config();
    let (mut app, rx) = app_in(repo.path(), config_path);
    click_new_workspace(&mut app);
    submit_text(&mut app, "wt");
    pump_until(&mut app, &rx, "the workspace opens", |a| a.projects[0].workspaces.len() == 2);
    let path = app.projects[0].workspaces[1].path.clone();
    Setup { app, rx, repo, path, _worktrees: worktrees, _config: config }
}

fn ask(s: &mut Setup) {
    let close = row_close(&s.app, WorkspaceRow::Workspace(1));
    click(&mut s.app, close);
}

fn checked(s: &mut Setup) {
    pump_until(&mut s.app, &s.rx, "git status answers", |a| {
        matches!(a.overlay, Some(Overlay::RemoveWorkspace { check: Check::Clean | Check::Changed, .. }))
    });
}

fn gone(s: &mut Setup) {
    pump_until(&mut s.app, &s.rx, "the workspace goes away", |a| a.projects[0].workspaces.len() == 1);
}

#[test]
fn asks_first() {
    let s = with_worktree();
    assert!(matches!(s.app.overlay, Some(Overlay::RemoveWorkspace { .. })));
}

#[test]
fn cancel_keeps_it() {
    let mut s = with_worktree();

    click(&mut s.app, form_button(REMOVE_SUBMIT, 1));

    assert_eq!((s.app.overlay.is_none(), s.app.projects[0].workspaces.len(), s.path.exists()), (true, 2, true));
}

#[test]
fn confirming_closes_the_dialog_at_once_and_marks_the_row() {
    let mut s = with_worktree();
    checked(&mut s);

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    assert_eq!((s.app.overlay.is_none(), s.app.removing(0, 1), s.app.projects[0].active), (true, true, 0));
}

#[test]
fn the_row_goes_once_git_is_done_and_the_branch_stays() {
    let mut s = with_worktree();
    checked(&mut s);

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    gone(&mut s);
    assert_eq!((toast(&s.app), s.path.exists(), branch_exists(s.repo.path(), "wt")), (Some("removed wt"), false, true));
}

#[test]
fn a_failed_teardown_still_removes_it_and_points_at_the_log() {
    let mut s = with_worktree();
    let teardown = ProjectSettings { teardown: "echo no route; exit 1".into(), ..Default::default() };
    s.app.config.projects.insert(s.repo.path().display().to_string(), teardown);
    checked(&mut s);

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    gone(&mut s);
    let toast = s.app.messages.toast.as_ref().expect("a toast");
    assert_eq!(
        (toast.message.as_str(), toast.icon, s.path.exists()),
        ("removed wt, cleanup failed in part, see server.log", ui::ToastIcon::Bug, false)
    );
}

#[test]
fn confirming_before_git_status_answers_waits_for_it() {
    let mut s = with_worktree();

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
    let waits = matches!(s.app.overlay, Some(Overlay::RemoveWorkspace { check: Check::Confirmed, .. }));

    gone(&mut s);
    assert!(waits && !s.path.exists());
}

#[test]
fn a_row_being_removed_does_not_react_to_clicks() {
    let mut s = with_worktree();
    checked(&mut s);
    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    click_row(&mut s.app, WorkspaceRow::Workspace(1));

    assert_eq!(s.app.projects[0].active, 0);
}

#[test]
fn a_row_being_removed_opens_no_menu() {
    let mut s = with_worktree();
    checked(&mut s);
    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    right_click_row(&mut s.app, WorkspaceRow::Workspace(1));

    assert!(s.app.overlay.is_none());
}

#[test]
fn the_row_stays_until_git_answers_even_once_the_folder_is_gone() {
    let mut s = with_worktree();
    checked(&mut s);
    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
    wait_until("git deletes the folder", || !s.path.exists());
    let shells: Vec<u64> = s.app.projects[0].workspaces[1].terms().map(|t| t.id).collect();
    for id in shells {
        s.app.remove(id);
    }

    s.app.sync_worktrees();

    assert!(s.app.removing(0, 1));
}

#[test]
fn a_removal_git_refuses_for_a_worktree_it_forgot_still_lets_the_row_go() {
    let mut s = with_worktree();
    checked(&mut s);
    let name = s.path.file_name().expect("folder name").to_owned();
    std::fs::remove_dir_all(s.repo.path().join(".git/worktrees").join(name)).expect("drop the registration");

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
    pump_until(&mut s.app, &s.rx, "git refuses", |a| a.projects[0].workspaces.get(1).is_none_or(Workspace::open));
    pump_until(&mut s.app, &s.rx, "the shells stop", |a| {
        a.projects[0].workspaces.get(1).is_none_or(|w| w.tabs.is_empty())
    });
    s.app.sync_worktrees();

    assert_eq!(s.app.projects[0].workspaces.len(), 1);
}

#[test]
fn the_row_keeps_its_name_while_git_deletes_the_folder() {
    let mut s = with_worktree();
    checked(&mut s);
    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    wait_until("git deletes the folder", || !s.path.exists());

    assert_eq!(s.app.projects[0].workspaces[1].label(), "wt");
}

#[test]
fn a_row_being_removed_is_not_a_search_result() {
    let mut s = with_worktree();
    checked(&mut s);
    let id = s.app.projects[0].workspaces[1].id;

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    let found = s.app.search_candidates().into_iter().map(|c| c.goto);
    assert!(!found.into_iter().any(|g| matches!(g, Goto::Place { workspace: Some(w), .. } if w == id)));
}

#[test]
fn changes_make_the_dialog_offer_remove_anyway_from_the_start() {
    let mut s = opened();
    std::fs::write(s.path.join("notes.txt"), "draft").expect("write file");
    ask(&mut s);

    checked(&mut s);
    assert_eq!(
        (form_error(&s.app).is_some(), s.app.overlay.as_ref().map(Overlay::submit_label)),
        (true, Some(FORCE_REMOVE_SUBMIT))
    );

    click(&mut s.app, form_button(FORCE_REMOVE_SUBMIT, 0));

    gone(&mut s);
    assert!(!s.path.exists());
}

#[test]
fn a_git_status_that_fails_counts_as_changes() {
    let mut s = opened();
    std::fs::write(s.path.join(".git"), "not a gitfile").expect("break the checkout");
    ask(&mut s);

    checked(&mut s);

    assert_eq!(s.app.overlay.as_ref().map(Overlay::submit_label), Some(FORCE_REMOVE_SUBMIT));
}

fn lock(s: &Setup, reason: &str) {
    git(s.repo.path(), &["worktree", "lock", "--reason", reason, &s.path.display().to_string()]);
}

fn note(app: &App) -> Option<String> {
    match app.overlay_view(app.overlay.as_ref()?, AREA)? {
        ui::Overlay::Confirm(ui::Confirm { note: Some(ui::Note::Error(text)), .. }) => Some(text),
        _ => None,
    }
}

fn submit(app: &App) -> Option<&'static str> {
    app.overlay.as_ref().map(Overlay::submit_label)
}

#[test]
fn a_lock_makes_the_dialog_say_so_before_anything_stops() {
    let mut s = opened();
    lock(&s, "on a usb disk");
    ask(&mut s);

    checked(&mut s);

    let message = confirmation(&s.app).expect("a confirmation");
    assert!(message.contains("is locked: on a usb disk"), "{message}");
    assert_eq!((submit(&s.app), s.app.projects[0].workspaces[1].tabs.is_empty()), (Some(UNLOCK_SUBMIT), false));
}

#[test]
fn a_lock_left_by_a_process_that_is_gone_says_so() {
    let mut s = opened();
    lock(&s, &format!("claude session wt (pid {} start Wed Oct  7 09:10:42 2026)", exited_pid()));
    ask(&mut s);

    checked(&mut s);

    assert!(note(&s.app).is_some_and(|n| n.contains("is gone; the lock was left behind")), "{:?}", note(&s.app));
}

#[test]
fn unlock_and_remove_deletes_a_locked_worktree_and_keeps_the_branch() {
    let mut s = opened();
    lock(&s, "busy");
    ask(&mut s);
    checked(&mut s);

    click(&mut s.app, form_button(UNLOCK_SUBMIT, 0));

    gone(&mut s);
    assert_eq!((s.path.exists(), branch_exists(s.repo.path(), "wt")), (false, true));
}

#[test]
fn unlock_and_remove_deletes_a_locked_worktree_with_changes() {
    let mut s = opened();
    std::fs::write(s.path.join("notes.txt"), "draft").expect("write file");
    lock(&s, "busy");
    ask(&mut s);
    checked(&mut s);
    assert!(note(&s.app).is_some_and(|n| n.contains("uncommitted")), "{:?}", note(&s.app));

    click(&mut s.app, form_button(UNLOCK_SUBMIT, 0));

    gone(&mut s);
    assert!(!s.path.exists());
}

#[test]
fn confirming_before_git_status_answers_stops_at_a_lock() {
    let mut s = opened();
    lock(&s, "busy");
    ask(&mut s);

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));
    checked(&mut s);

    assert_eq!(
        (submit(&s.app), s.app.removing(0, 1), s.app.projects[0].workspaces[1].tabs.is_empty()),
        (Some(UNLOCK_SUBMIT), false, false)
    );
}

#[test]
fn a_refused_removal_brings_the_row_back_and_says_why() {
    let mut s = opened();
    ask(&mut s);
    checked(&mut s);
    std::fs::write(s.path.join("notes.txt"), "draft").expect("write file");

    click(&mut s.app, form_button(REMOVE_SUBMIT, 0));

    pump_until(&mut s.app, &s.rx, "git refuses", |a| !a.projects[0].workspaces[1].removing());
    assert!(toast(&s.app).is_some_and(|t| t.contains("modified or untracked")), "{:?}", toast(&s.app));
    pump_until(&mut s.app, &s.rx, "its shells stopped", |a| a.projects[0].workspaces[1].tabs.is_empty());
    ask(&mut s);
    checked(&mut s);
    assert!(s.path.exists() && matches!(s.app.overlay, Some(Overlay::RemoveWorkspace { .. })));
}
