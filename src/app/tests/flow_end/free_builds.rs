use std::path::PathBuf;
use std::process::Command;

use super::close_when_merged::{opted, pull, workspace_id};
use super::*;
use crate::app::tests::control_requests::build_folders::with_builds;
use crate::app::tests::pull_requests::{gh_answering, repo_on};
use crate::flows::run::{FREE_SETTLE, MERGED_GRACE, RoleWorktree};
use crate::test_util::Running;

struct Team {
    repo: Repo,
    own: PathBuf,
    role: PathBuf,
    id: u64,
}

fn team(free: bool) -> Team {
    let mut repo = repo_on(&["feat/x", "feat/x--coder"], |dir| gh_answering(dir, "cat \"$0.json\""), |_| {});
    run_in(&mut repo.app, (0, 1), flow(End::PullRequest));
    repo.app.projects[0].active = 1;
    let own = repo.app.projects[0].workspaces[1].path.clone();
    let role = repo.app.projects[0].workspaces.iter().map(|w| w.path.clone()).find(|p| p.ends_with("feat-x--coder"));
    let role = role.expect("the role's worktree");
    let seat = RoleWorktree { role: "coder A".into(), path: role.clone(), branch: "feat/x--coder".into() };
    repo.app.projects[0].workspaces[1].role_worktrees.push(seat);
    for path in [&own, &role] {
        with_builds(path);
        std::fs::write(path.join("mine.txt"), "mine\n").expect("an untracked file");
    }
    let run = repo.app.runs.first_mut();
    (run.close_when_merged, run.free_builds) = (true, free);
    pull(&mut repo, "\"MERGED\"");
    let id = workspace_id(&repo.app);
    Team { repo, own, role, id }
}

fn merged(app: &mut App) -> Vec<Heard> {
    messages(&app.take_events(), 1)
}

fn merged_with_failed(app: &mut App) -> Vec<(Heard, bool)> {
    messages_with_failed(&app.take_events(), 1)
}

fn toast_icon(team: &Team) -> ui::ToastIcon {
    team.repo.app.messages.toast.as_ref().expect("a toast").icon
}

fn closed_at(team: &mut Team) -> Instant {
    let app = &mut team.repo.app;
    app.clients.events.subscribe(1, Vec::new(), false, true);
    let _ = app.take_events();
    let t0 = Instant::now();
    app.close_merged_runs(t0);
    app.close_merged_runs(t0 + MERGED_GRACE);
    t0 + MERGED_GRACE
}

fn tabs_gone(team: &mut Team) {
    let repo = &mut team.repo;
    pump_until(&mut repo.app, &repo.rx, "the tabs go", |a| a.projects[0].workspaces[1].tabs.is_empty());
}

fn freed(team: &mut Team, closed: Instant, said: &str) {
    tabs_gone(team);
    team.repo.app.close_merged_runs(closed + FREE_SETTLE);
    let repo = &mut team.repo;
    pump_until(&mut repo.app, &repo.rx, said, |a| {
        a.messages.toast.as_ref().is_some_and(|toast| toast.message.contains(said))
    });
}

fn branch_exists(team: &Team, branch: &str) -> bool {
    let repo = &team.own;
    let out = Command::new("git").arg("-C").arg(repo).args(["rev-parse", "--verify", "--quiet", branch]).output();
    out.is_ok_and(|out| out.status.success())
}

fn the_close_only_says_it_is_freeing(team: &mut Team) {
    let toast = team.repo.app.messages.toast.as_ref().expect("a toast").message.clone();
    assert!(toast.contains("freeing the builds"), "{toast}");
    assert_eq!(merged(&mut team.repo.app).len(), 0);
    assert!(team.own.join("node_modules").exists() && team.role.join("node_modules").exists());
    let kept = &team.repo.app.agents.closed.of(team.id).and_then(|k| k.run.as_ref()).expect("kept").run;
    assert_eq!((kept.close_when_merged, kept.free_builds), (false, false));
}

fn only_the_builds_went(team: &Team) {
    assert!(!team.own.join("node_modules").exists() && !team.role.join("node_modules").exists());
    for path in [&team.own, &team.role] {
        assert!(path.join("mine.txt").exists() && path.join(".git").exists());
    }
    assert!(branch_exists(team, "feat/x") && branch_exists(team, "feat/x--coder"));
}

#[test]
fn a_merged_run_that_frees_its_builds_frees_its_worktree_and_its_roles_and_says_how_much() {
    let mut team = team(true);
    let closed = closed_at(&mut team);
    the_close_only_says_it_is_freeing(&mut team);

    team.repo.app.close_merged_runs(closed + FREE_SETTLE);
    assert!(!team.repo.app.freeing_path(&team.own));
    assert!(team.own.join("node_modules").exists());

    freed(&mut team, closed, "of builds freed");

    only_the_builds_went(&team);
    let said = team.repo.app.messages.toast.as_ref().expect("a toast").message.clone();
    assert!(said.contains("agents closed, ") && said.contains("of builds freed · "), "{said}");
    assert_eq!(toast_icon(&team), ui::ToastIcon::Check);
    let told = merged_with_failed(&mut team.repo.app);
    assert_eq!(told.len(), 1, "{told:?}");
    let ((kind, words, ..), failed) = &told[0];
    assert!(kind == "flow-merged" && words.ends_with("of builds freed") && !failed, "{told:?}");
    assert_eq!(said, format!("{words} · ▶ on the workspace reopens them"));
    assert!(!team.repo.app.freeing_path(&team.own) && !team.repo.app.freeing_path(&team.role));
}

#[test]
fn without_free_builds_a_merged_run_frees_nothing_and_says_what_it_said() {
    let mut team = team(false);
    let closed = closed_at(&mut team);

    let told = merged(&mut team.repo.app);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(!told[0].1.contains("builds"), "{}", told[0].1);
    tabs_gone(&mut team);
    team.repo.app.close_merged_runs(closed + FREE_SETTLE);

    assert!(team.own.join("node_modules").exists() && team.role.join("node_modules").exists());
    assert_eq!(merged(&mut team.repo.app).len(), 0);
}

#[test]
fn a_program_in_the_worktree_keeps_its_builds_and_the_message_says_why() {
    let mut team = team(true);
    let path = team.own.clone();
    let sleeper = Running::spawn(Command::new("/bin/sleep").arg("30").current_dir(&path));
    wait_until("sleep works in the folder", || process::cwd(sleeper.pid()).as_deref() == Some(path.as_path()));
    let closed = closed_at(&mut team);

    freed(&mut team, closed, "of builds freed");

    assert!(team.own.join("node_modules").exists());
    assert!(!team.role.join("node_modules").exists());
    assert_eq!(toast_icon(&team), ui::ToastIcon::Check);
    let told = merged_with_failed(&mut team.repo.app);
    assert!(told[0].0.1.contains("the worktree kept") && told[0].0.1.contains("sleep runs in it"), "{told:?}");
    assert!(!told[0].1, "{told:?}");
}

#[test]
fn a_worktree_that_cannot_be_freed_marks_the_message_failed() {
    let mut team = team(true);
    let gone = team.role.join("gone");
    let seats = &mut team.repo.app.projects[0].workspaces[1].role_worktrees;
    seats.iter_mut().find(|seat| seat.role == "coder A").expect("the seat").path = gone;
    let closed = closed_at(&mut team);

    freed(&mut team, closed, "could not free coder A's worktree");

    assert_eq!(toast_icon(&team), ui::ToastIcon::Bug);
    let said = team.repo.app.messages.toast.as_ref().expect("a toast").message.clone();
    let told = merged_with_failed(&mut team.repo.app);
    assert_eq!(told.len(), 1, "{told:?}");
    let ((kind, words, ..), failed) = &told[0];
    assert!(kind == "flow-merged" && *failed && !words.contains('▶'), "{told:?}");
    assert!(words.contains("of builds freed; could not free coder A's worktree: "), "{words}");
    assert_eq!(said, format!("{words} · ▶ on the workspace reopens them"));
}

#[test]
fn status_and_the_feed_say_the_run_frees_its_builds() {
    let mut repo = with_run(End::PullRequest);
    let before = repo.app.refreshed();
    repo.app.runs.first_mut().free_builds = true;

    assert_ne!(repo.app.refreshed(), before);
    assert!(repo.app.flow_runs_state().runs[0].free_builds);
    let report = repo.app.report(None);
    assert!(report.projects[0].workspaces[1].run.as_ref().expect("a run").free_builds);
}

#[test]
fn both_choices_come_back_after_a_restart_live_or_closed_by_hand() {
    let mut repo = opted(End::PullRequest);
    repo.app.runs.first_mut().free_builds = true;
    let id = workspace_id(&repo.app);
    let saved = repo.app.state();
    let (mut again, _rx) = new_app(no_config());
    assert!(again.restore(&saved, AREA));
    assert!(again.runs.all()[0].close_when_merged && again.runs.all()[0].free_builds);

    let plan = repo.app.agents_close_plan(id).expect("a plan");
    repo.app.close_agents(&plan);
    let saved = repo.app.state();
    let (mut again, _rx) = new_app(no_config());
    assert!(again.restore(&saved, AREA));
    let kept = &again.agents.closed.of(id).and_then(|k| k.run.as_ref()).expect("kept").run;
    assert!(kept.close_when_merged && kept.free_builds);
}
