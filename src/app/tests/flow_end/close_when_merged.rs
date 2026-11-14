use rstest::rstest;

use super::*;
use crate::agents::launch::Launch;
use crate::flows::run::{MERGED_GRACE, Sent};
use crate::project::split::Dir;

pub(super) fn opted(end: End) -> Repo {
    let mut repo = with_run(end);
    repo.app.runs.first_mut().close_when_merged = true;
    repo
}

pub(super) fn pull(repo: &mut Repo, state: &str) {
    answer(repo, &VIEW.replace("\"OPEN\"", state), 1);
}

pub(super) fn workspace_id(app: &App) -> u64 {
    app.runs.all()[0].workspace
}

fn closes_at(repo: &mut Repo, t0: Instant) -> bool {
    repo.app.close_merged_runs(t0);
    repo.app.close_merged_runs(t0 + MERGED_GRACE);
    repo.app.runs.all().is_empty()
}

#[test]
fn a_merged_run_with_nothing_at_work_closes_its_agents_after_three_minutes() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let (id, t0) = (workspace_id(&repo.app), Instant::now());
    let run = repo.app.runs.all()[0].id;
    let path = repo.app.projects[0].workspaces[1].path.clone();

    let plan = repo.app.agents_close_plan(id).expect("a plan");
    let orchestrator = repo.app.runs.all()[0].pane_of(ORCHESTRATOR).expect("the orchestrator");
    repo.app.clients.events.subscribe(1, Vec::new(), false, true);
    repo.app.clients.events.subscribe(2, vec![orchestrator], false, true);
    repo.app.clients.events.subscribe(3, Vec::new(), false, false);
    let _ = repo.app.take_events();

    repo.app.close_merged_runs(t0);
    assert_eq!(repo.app.runs.all().len(), 1);
    repo.app.close_merged_runs(t0 + MERGED_GRACE);

    let said = repo.app.messages.toast.as_ref().expect("a toast").message.clone();
    let events = repo.app.take_events();
    let ids = crate::control::Ids { project: Some(repo.app.projects[0].id), workspace: Some(id), ..Default::default() };
    let label = repo.app.projects[0].workspaces[1].label();
    let words = format!("pull request merged in {label}: {} agents closed", plan.agents.len());
    assert_eq!(said, format!("{words} · ▶ on the workspace reopens them"));
    let told = vec![("flow-merged".to_string(), words, plan.agents.clone(), ids)];
    assert_eq!((messages(&events, 1), messages(&events, 2)), (told.clone(), told));
    let silent = messages(&events, 3);
    assert!(silent.is_empty(), "{silent:?}");
    assert!(repo.app.runs.all().is_empty() && repo.app.agents.closed.has_run(id));
    let runs = repo.app.flow_runs_state().runs;
    assert_eq!((runs.len(), runs[0].run, runs[0].state.as_deref()), (1, run, Some("closed")));
    assert!(!runs[0].close_when_merged);
    let toast = repo.app.messages.toast.as_ref().expect("a toast");
    assert_eq!(toast.view().button, Some(ui::ToastButton::Reopen));
    pump_until(&mut repo.app, &repo.rx, "the tabs go", |a| a.projects[0].workspaces[1].tabs.is_empty());
    assert!(path.is_dir());
}

#[test]
fn closing_a_run_by_hand_tells_programs_nothing() {
    let mut repo = opted(End::PullRequest);
    repo.app.clients.events.subscribe(1, Vec::new(), false, true);
    let plan = repo.app.agents_close_plan(workspace_id(&repo.app)).expect("a plan");
    let _ = repo.app.take_events();

    repo.app.close_agents_with_toast(&plan);

    let told = messages(&repo.app.take_events(), 1);
    assert!(told.is_empty(), "{told:?}");
    assert!(repo.app.messages.toast.is_some());
}

#[test]
fn without_the_opt_in_a_merged_run_stays_open() {
    let mut repo = with_run(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");

    assert!(!closes_at(&mut repo, Instant::now()));
}

#[rstest]
#[case::open("\"OPEN\"")]
#[case::closed("\"CLOSED\"")]
fn a_pull_request_that_is_open_or_closed_never_closes_it(#[case] state: &str) {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, state);

    assert!(!closes_at(&mut repo, Instant::now()));
}

#[test]
fn an_agent_at_work_during_the_three_minutes_starts_them_over() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let t0 = Instant::now();
    let later = |seconds: u64| t0 + Duration::from_secs(seconds);

    repo.app.close_merged_runs(t0);
    orchestrator(&mut repo.app).agent.update(Some(Activity::Working), true, later(100));
    repo.app.close_merged_runs(later(100));
    repo.app.close_merged_runs(later(180));
    assert_eq!(repo.app.runs.all().len(), 1);

    orchestrator(&mut repo.app).agent.update(Some(Activity::Idle), true, later(200));
    repo.app.close_merged_runs(later(200));
    repo.app.close_merged_runs(later(379));
    assert_eq!(repo.app.runs.all().len(), 1);
    repo.app.close_merged_runs(later(380));
    assert!(repo.app.runs.all().is_empty());
}

#[test]
fn a_shell_in_one_of_the_teams_tabs_keeps_them_open() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let member = repo.app.runs.all()[0].members[1].pane;
    repo.app.split_pane(member, Dir::Right, AREA, false).expect("a split");

    assert!(!closes_at(&mut repo, Instant::now()));
}

#[test]
fn a_launch_in_flight_keeps_them_open() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let member = repo.app.runs.all()[0].members[1].pane;
    let now = Instant::now();
    repo.app.agents.launches.add(Launch::resume(member, "sh".into(), true, now, now + Duration::from_secs(600)));

    assert!(!closes_at(&mut repo, now));
}

#[test]
fn a_flow_that_leaves_the_commits_never_closes() {
    let mut repo = opted(End::Commits);
    pull(&mut repo, "\"MERGED\"");

    assert!(!closes_at(&mut repo, Instant::now()));
}

#[test]
fn a_manual_close_keeps_the_opt_in_and_drops_the_countdown() {
    let mut repo = opted(End::PullRequest);
    repo.app.runs.first_mut().free_builds = true;
    pull(&mut repo, "\"MERGED\"");
    let id = workspace_id(&repo.app);
    repo.app.close_merged_runs(Instant::now());
    assert!(repo.app.runs.all()[0].merged_since.is_some());

    let plan = repo.app.agents_close_plan(id).expect("a plan");
    repo.app.close_agents(&plan);

    let kept = &repo.app.agents.closed.of(id).and_then(|kept| kept.run.as_ref()).expect("a kept run").run;
    assert_eq!((kept.close_when_merged, kept.free_builds, kept.merged_since), (true, true, None));
}

#[test]
fn status_and_the_feed_say_the_run_closes_when_merged() {
    let mut repo = with_run(End::PullRequest);
    let before = repo.app.refreshed();
    repo.app.runs.first_mut().close_when_merged = true;

    assert_ne!(repo.app.refreshed(), before);
    assert!(repo.app.flow_runs_state().runs[0].close_when_merged);
    let report = repo.app.report(None);
    let run = report.projects[0].workspaces[1].run.as_ref().expect("a run");
    assert!(run.close_when_merged);
}

#[test]
fn typing_in_an_agent_during_the_countdown_starts_it_over() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let (t0, member) = (Instant::now(), repo.app.runs.all()[0].members[1].pane);
    let later = |seconds: u64| t0 + Duration::from_secs(seconds);

    repo.app.close_merged_runs(t0);
    let term = repo.app.projects.pane_by_mut(member).expect("the pane");
    let pid = term.foreground_pid();
    term.typing.text("one more thing", pid, later(170));
    repo.app.close_merged_runs(later(170));
    repo.app.close_merged_runs(later(180));
    repo.app.close_merged_runs(later(229));
    repo.app.close_merged_runs(later(230));
    repo.app.close_merged_runs(later(409));
    assert_eq!(repo.app.runs.all().len(), 1);
    repo.app.close_merged_runs(later(410));
    assert!(repo.app.runs.all().is_empty());
}

#[test]
fn a_wake_of_the_orchestrator_that_is_due_keeps_them_open() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let member = repo.app.runs.all()[0].members[1].pane;
    let t0 = Instant::now();
    repo.app.runs.first_mut().sent.push(Sent { pane: member, at: t0, finished: true, failed: false, back: false });

    assert!(!closes_at(&mut repo, t0));
}

#[test]
fn an_agent_of_another_kind_opened_in_the_workspace_that_cannot_resume_keeps_them_open() {
    let mut repo = opted(End::PullRequest);
    pull(&mut repo, "\"MERGED\"");
    let id = workspace_id(&repo.app);
    let at = workspace_at(&repo.app, 0, 1);
    let tab = repo.app.push_tab(at, AREA, None).expect("a tab").index();
    let pane = repo.app.projects[0].workspaces[1].tabs[tab].panes[0].id;
    let term = repo.app.projects.pane_by_mut(pane).expect("the pane");
    term.agent.follow(Some("opencode"));
    term.agent.update(Some(Activity::Idle), true, Instant::now());
    assert!(repo.app.agents_close_plan(id).expect("a plan").fresh.contains(&pane));

    assert!(!closes_at(&mut repo, Instant::now()));
}

#[test]
fn reopening_after_the_merge_keeps_the_team_open() {
    let mut repo = opted(End::PullRequest);
    repo.app.runs.first_mut().free_builds = true;
    let id = workspace_id(&repo.app);
    let plan = repo.app.agents_close_plan(id).expect("a plan");
    repo.app.close_agents(&plan);
    pump_until(&mut repo.app, &repo.rx, "the tabs go", |a| a.projects[0].workspaces[1].tabs.is_empty());
    pull(&mut repo, "\"MERGED\"");
    for kind in agents::kinds(&repo.app.config) {
        repo.app.config.agent_commands.insert(kind, "/bin/sh".into());
    }

    repo.app.reopen_agents(id, AREA).expect("reopened");

    assert!(!repo.app.runs.all()[0].close_when_merged);
    assert!(!repo.app.runs.all()[0].free_builds);
}
