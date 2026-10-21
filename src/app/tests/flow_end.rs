use std::time::Duration;

use super::flow_panel::{TALL, run_in, spot};
use super::pull_requests::{Repo, VIEW, repo_with};
use super::*;
use crate::agents::activity::Activity;
use crate::flows::{End, Flow, ORCHESTRATOR, builtin};
use crate::pull_request::{Ci, State};
use crate::test_util::git;

fn flow(end: End) -> Flow {
    let mut flow = builtin::flows(&agents::Known::ALL, Some(agents::Known::Claude)).remove(0);
    flow.end = end;
    flow
}

fn with_run(end: End) -> Repo {
    let mut repo = repo_with("cat \"$0.json\"", |_| {});
    run_in(&mut repo.app, (0, 1), flow(end));
    repo.app.projects[0].active = 1;
    repo
}

fn answer(repo: &mut Repo, json: &str, step: u64) {
    std::fs::write(repo.tmp.path().join("gh.json"), json).expect("the answer");
    repo.app.ask_pull_requests(Instant::now() + Duration::from_mins(10 * step));
    pump_until(&mut repo.app, &repo.rx, "gh answers", |a| a.git.pulls.asking.is_empty());
}

fn end(app: &App) -> ui::flow::Ending {
    app.flow_view(Instant::now(), None).and_then(|view| view.diagram.live).expect("a live run").end
}

fn orchestrator(app: &mut App) -> &mut Term {
    let pane = app.runs.all()[0].pane_of(ORCHESTRATOR).expect("the orchestrator");
    app.projects.pane_by_mut(pane).expect("its pane")
}

fn mark(app: &mut App) {
    orchestrator(app).agent.mark(true);
    app.latch_run_ends();
}

#[test]
fn the_end_lights_once_the_pull_request_appears_and_its_tag_follows_the_checks() {
    let mut repo = with_run(End::PullRequest);
    repo.app.ask_pull_requests(Instant::now());
    pump_until(&mut repo.app, &repo.rx, "gh fails", |a| a.git.pulls.asking.is_empty());
    assert_eq!((end(&repo.app).reached, end(&repo.app).pull), (false, None));

    answer(&mut repo, VIEW, 1);
    let found = end(&repo.app);
    assert!(found.reached);
    let pull = found.pull.expect("its mark");
    assert_eq!((pull.number, pull.ci), (482, Some(Ci::Failing)));
    repo.app.show_flow();
    spot(&mut repo.app, "#482✗");

    answer(&mut repo, &VIEW.replace("\"FAILURE\"", "\"SUCCESS\""), 2);
    assert_eq!(end(&repo.app).pull.expect("its mark").ci, Some(Ci::Pending));
    spot(&mut repo.app, "#482•");

    answer(&mut repo, &VIEW.replace("\"OPEN\"", "\"MERGED\""), 3);
    assert_eq!(end(&repo.app).pull.map(|pull| pull.state), Some(State::Merged));
    assert!(end(&repo.app).reached);

    answer(&mut repo, &VIEW.replace("\"OPEN\"", "\"CLOSED\""), 4);
    let closed = end(&repo.app);
    assert_eq!((closed.reached, closed.pull.map(|pull| pull.state)), (false, Some(State::Closed)));
}

#[test]
fn the_end_stays_lit_once_the_orchestrator_marked_it_even_when_it_works_again_without_a_pull_request() {
    let mut repo = with_run(End::PullRequest);
    let now = Instant::now();
    orchestrator(&mut repo.app).agent.update(Some(Activity::Idle), true, now);

    mark(&mut repo.app);

    assert_eq!(repo.app.runs.all()[0].ended.as_deref(), Some("feat/x"));
    orchestrator(&mut repo.app).agent.update(Some(Activity::Working), true, now);
    assert!(!orchestrator(&mut repo.app).agent.pending());
    repo.app.config.pull_requests = false;
    repo.app.ask_pull_requests(now);
    let ended = end(&repo.app);
    assert_eq!((ended.reached, ended.pull), (true, None));
}

#[test]
fn a_flow_that_leaves_the_commits_ends_on_the_mark_alone_and_shows_no_tag() {
    let mut repo = with_run(End::Commits);
    answer(&mut repo, VIEW, 1);
    let before = end(&repo.app);
    assert_eq!((before.reached, before.pull), (false, None));

    mark(&mut repo.app);

    let after = end(&repo.app);
    assert_eq!((after.reached, after.pull), (true, None));
}

#[test]
fn another_branch_forgets_the_end_and_a_detached_head_keeps_it() {
    let mut repo = with_run(End::Commits);
    mark(&mut repo.app);
    let path = repo.app.projects[0].workspaces[1].path.clone();

    git(&path, &["checkout", "--quiet", "--detach"]);
    repo.app.forget_moved_run_ends();
    assert!(repo.app.runs.all()[0].ended.is_some());

    git(&path, &["checkout", "--quiet", "-b", "feat/y"]);
    repo.app.forget_moved_run_ends();
    assert_eq!(repo.app.runs.all()[0].ended, None);
    assert!(!orchestrator(&mut repo.app).agent.pending());
    repo.app.latch_run_ends();
    assert_eq!(repo.app.runs.all()[0].ended, None);
}

#[test]
fn a_click_on_the_tag_opens_the_pull_request_dialog() {
    let mut repo = with_run(End::PullRequest);
    answer(&mut repo, VIEW, 1);
    repo.app.show_flow();

    let pos = spot(&mut repo.app, "#482✗");
    click_in(&mut repo.app, pos, TALL);

    let Some(Overlay::PullRequest { workspace, .. }) = repo.app.overlay else { panic!("the dialog is not open") };
    assert_eq!(workspace, repo.worktree());
}

#[test]
fn scripts_see_whether_the_run_reached_its_end() {
    let mut repo = with_run(End::PullRequest);
    let run_of = |app: &App| app.report(None).projects[0].workspaces[1].run.clone().expect("a run");

    let before = run_of(&repo.app);
    assert_eq!((before.end.as_str(), before.reached), ("pull_request", false));

    answer(&mut repo, VIEW, 1);
    assert!(run_of(&repo.app).reached);
    assert_eq!(repo.app.report(None).projects[0].workspaces[0].run, None);
}

#[test]
fn the_latch_changes_what_watchers_see() {
    let mut repo = with_run(End::Commits);
    let before = repo.app.refreshed();

    mark(&mut repo.app);

    assert_ne!(repo.app.refreshed(), before);
}
