use rstest::rstest;

use super::super::close_agent::{agent, tabs};
use super::super::worktrees_dialog::{Setup, alive, commit_in, path_of, with};
use super::*;
use crate::control::arrange::AgentClosing;
use crate::test_util::git;

fn close_agent(pane: u64, check: bool) -> Command {
    Command::CloseAgent(wire::CloseAgent { pane, check, ..wire::CloseAgent::default() })
}

fn closing(response: Response) -> AgentClosing {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("an agent closing"),
        Response::Error(message) => panic!("close-agent was refused: {message}"),
    }
}

fn in_worktree() -> (Setup, u64, PathBuf) {
    let mut s = with(&["wt"]);
    let pane = agent(&mut s.app, 1, 0);
    let path = path_of(&s, "wt");
    (s, pane, path)
}

fn asked(s: &mut Setup, command: Command, what: &str) -> Response {
    ask(&mut s.app, None, command);
    let mut got = Vec::new();
    wait_until(what, || {
        drain(&mut s.app, &s.rx);
        got = answers(&mut s.app);
        !got.is_empty()
    });
    got.remove(0)
}

fn untouched(s: &Setup, path: &Path) -> bool {
    alive(&s.app, path) && path.exists()
}

fn gone(s: &mut Setup, path: &Path) -> bool {
    pump_until(&mut s.app, &s.rx, "the workspace goes", |a| a.projects[0].workspaces.len() == 1);
    !path.exists()
}

#[test]
fn check_stops_nothing() {
    let (mut s, pane, path) = in_worktree();
    tabs(&mut s.app, 1, 1);
    std::fs::write(path.join("notes.txt"), "draft").expect("write file");

    let found = closing(asked(&mut s, close_agent(pane, true), "git checks the worktree"));

    let removes = found.removes.as_ref().expect("the workspace goes");
    assert_eq!((found.pane, found.agent.as_deref(), found.closed), (pane, Some("claude"), false));
    assert_eq!(
        (removes.path.as_path(), removes.changed, found.tabs.len(), found.stops.len()),
        (path.as_path(), true, 2, 2)
    );
    assert_eq!(
        found.branches.iter().map(|b| (b.branch.as_deref(), b.deletes)).collect::<Vec<_>>(),
        [(Some("wt"), false)]
    );
    assert!(untouched(&s, &path));
}

#[rstest]
#[case::a_shell(false, "conflict")]
#[case::a_pane_that_is_not_there(true, "not-found")]
fn a_shell_pane_is_refused(#[case] missing: bool, #[case] code: &str) {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = if missing { 999 } else { app.projects[0].workspaces[0].tabs[0].panes[0].id };

    let refused = refused(&mut app, "close-agent", &json!({"pane": pane}));

    assert_eq!(refused.code, code, "{}", refused.message);
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
}

#[test]
fn a_pane_without_a_worktree_closes_its_tab_and_keeps_the_others() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 1);
    let pane = agent(&mut app, 0, 1);
    let other = app.projects[0].workspaces[0].tabs[0].id;

    let closed = closing(now(&mut app, None, close_agent(pane, false)));

    assert_eq!((closed.removes, closed.closed, closed.stops.len()), (None, true, 1));
    pump_until(&mut app, &rx, "the agent's tab goes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
    assert_eq!(app.projects[0].workspaces[0].tabs[0].id, other);
}

#[test]
fn changes_need_force() {
    let (mut s, pane, path) = in_worktree();
    std::fs::write(path.join("notes.txt"), "draft").expect("write file");

    let message = error(asked(&mut s, close_agent(pane, false), "git checks the worktree"));
    assert!(message.contains("not committed") && message.contains("--force"), "{message}");
    assert!(untouched(&s, &path));

    let forced = Command::CloseAgent(wire::CloseAgent { pane, force: true, ..wire::CloseAgent::default() });
    let closed = closing(asked(&mut s, forced, "the workspace is removed"));
    assert!(closed.closed && gone(&mut s, &path));
}

#[test]
fn a_lock_needs_unlock() {
    let (mut s, pane, path) = in_worktree();
    git(s.repo.path(), &["worktree", "lock", "--reason", "on a usb disk", path.to_str().expect("utf-8")]);

    let message = error(asked(&mut s, close_agent(pane, false), "git checks the worktree"));
    assert!(message.contains("is locked (on a usb disk)") && message.contains("--unlock"), "{message}");
    assert!(untouched(&s, &path));

    let unlocked = Command::CloseAgent(wire::CloseAgent { pane, unlock: true, ..wire::CloseAgent::default() });
    let closed = closing(asked(&mut s, unlocked, "the workspace is removed"));
    assert!(closed.closed && gone(&mut s, &path));
}

#[rstest]
#[case::kept(false)]
#[case::deleted(true)]
fn delete_branch_deletes_only_when_asked(#[case] delete_branch: bool) {
    let (mut s, pane, path) = in_worktree();

    let command = Command::CloseAgent(wire::CloseAgent { pane, delete_branch, ..wire::CloseAgent::default() });
    let closed = closing(asked(&mut s, command, "the workspace is removed"));

    let fates: Vec<_> = closed.branches.iter().map(|b| (b.branch.as_deref(), b.deletes, b.deleted)).collect();
    assert_eq!(fates, [(Some("wt"), delete_branch, delete_branch)]);
    assert!(gone(&mut s, &path));
    assert_eq!(branch_exists(s.repo.path(), "wt"), !delete_branch);
}

#[test]
fn a_branch_whose_commits_would_be_lost_needs_force() {
    let (mut s, pane, path) = in_worktree();
    commit_in(&path);
    let asking = |force| {
        Command::CloseAgent(wire::CloseAgent { pane, delete_branch: true, force, ..wire::CloseAgent::default() })
    };

    let message = error(asked(&mut s, asking(false), "git checks the worktree"));
    assert!(
        message.contains("the branch wt has commits in no remote branch") && message.contains("--force"),
        "{message}"
    );
    assert!(untouched(&s, &path) && branch_exists(s.repo.path(), "wt"));

    let closed = closing(asked(&mut s, asking(true), "the workspace is removed"));
    assert!(closed.closed && closed.branches[0].lost && gone(&mut s, &path));
    assert!(!branch_exists(s.repo.path(), "wt"));
}

#[test]
fn a_removal_by_request_shows_no_toast() {
    let (mut s, pane, path) = in_worktree();

    closing(asked(&mut s, close_agent(pane, false), "the workspace is removed"));

    assert!(gone(&mut s, &path));
    assert_eq!(s.app.messages.toast.as_ref().map(|toast| toast.message.clone()), None);
}

fn check_lands(s: &mut Setup) {
    wait_until("the check lands", || {
        let mut landed = false;
        while let Ok(event) = s.rx.try_recv() {
            landed |= matches!(event, AppEvent::Worktree(WorktreeEvent::AgentChecked { .. }));
            s.app.handle_event(event, AREA).expect("handle event");
        }
        landed
    });
}

#[test]
fn a_check_is_dropped_when_its_client_leaves() {
    let (mut s, pane, path) = in_worktree();
    ask(&mut s.app, None, close_agent(pane, true));

    s.app.forget(CLIENT);
    let waiting = s.app.clients.requests_waiting();
    check_lands(&mut s);

    assert_eq!((waiting, answers(&mut s.app).len()), (0, 0));
    assert!(untouched(&s, &path) && s.app.projects[0].workspaces.len() == 2);
}

#[test]
fn a_close_goes_on_when_its_client_leaves() {
    let (mut s, pane, path) = in_worktree();
    ask(&mut s.app, None, close_agent(pane, false));

    s.app.forget(CLIENT);
    let kept = s.app.clients.requests_waiting();
    check_lands(&mut s);

    assert_eq!(kept, 1);
    assert!(gone(&mut s, &path));
    wait_until("the request settles", || {
        drain(&mut s.app, &s.rx);
        s.app.clients.requests_waiting() == 0
    });
    assert_eq!(answers(&mut s.app).len(), 0);
}

#[test]
fn a_target_that_changed_while_git_checked_is_a_conflict() {
    let (mut s, pane, path) = in_worktree();
    ask(&mut s.app, None, close_agent(pane, false));
    s.app.projects[0].workspaces[1].worktree = false;

    let mut got = Vec::new();
    wait_until("the check answers", || {
        drain(&mut s.app, &s.rx);
        got = answers(&mut s.app);
        !got.is_empty()
    });

    let message = error(got.remove(0));
    assert!(message.contains("changed while git checked it"), "{message}");
    assert!(path.exists() && s.app.projects[0].workspaces.len() == 2);
}
