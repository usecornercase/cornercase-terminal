use super::routines::{Routines, routine, routines, runs, start_flowing};
use super::*;
use crate::agents::activity;
use crate::project::Hierarchy;
use crate::test_util::git as run_git;

struct Finished {
    world: Routines,
    run: u64,
    pane: u64,
    workspace: u64,
    path: PathBuf,
    since: Instant,
    origin: Instant,
}

const STAYS: &str = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\nprintf '\\033[2J\\033[H'\n\
                     printf 'agent ready> '\nread line\nwhile read -r more; do :; done\n";

fn started(cleanup: bool) -> Finished {
    let mut world = routines();
    let at = time::now() + 3600;
    crate::test_util::write_executable(&world.config.path().join("fake-agent"), STAYS);
    let task = Task { cleanup, ..routine(&world.app, at, "2h") };
    let id = world.app.panels.queued.schedule(None, task).expect("routine");
    let since = Instant::now();
    world.app.run_queued(id, since).expect("start now");
    start_flowing(&mut world, id, 1);
    let run = runs(&world.app, id).remove(0);
    let QueueState::Started { pane, .. } = run.state else { panic!("started") };
    let path = run.task.place.workspace.clone().expect("workspace");
    let workspace = run.task.place.workspace_id.expect("workspace id");
    let origin = world.app.agents.watched().unwrap_or(since).max(since) + Duration::from_secs(1);
    Finished { world, run: run.id, pane, workspace, path, since, origin }
}

impl Finished {
    fn report(&mut self, state: &str) {
        let term = self.world.app.projects.pane_by_mut(self.pane).expect("pane");
        term.feed(format!("\x1b]7501;state={state}\x1b\\").as_bytes());
    }

    fn at(&mut self, seconds: u64) {
        let now = self.origin + Duration::from_secs(seconds);
        self.world.app.watch_agents(now);
        self.world.app.drive_cleanups(now);
    }

    fn state(&self) -> QueueState {
        self.world.app.panels.queued.queue().entry(self.run).expect("run").state.clone()
    }

    fn wait_for(&mut self, what: &str, state: impl Fn(&QueueState) -> bool) {
        let Routines { app, rx, .. } = &mut self.world;
        let run = self.run;
        wait_until_within(what, Duration::from_secs(30), || {
            drain(app, rx);
            state(&app.panels.queued.queue().entry(run).expect("run").state)
        });
    }

    fn done_at(&mut self, seconds: u64) {
        self.report("working");
        self.at(seconds - 1);
        self.report("done");
        self.at(seconds);
        let term = self.world.app.projects.pane_by(self.pane).expect("pane");
        assert_eq!(term.agent.status(), Some(activity::Status::Done));
        assert!(term.agent.worked_since(self.since));
    }
}

#[test]
fn done_for_the_grace_removes_the_workspace_its_folder_and_its_branch() {
    let mut run = started(true);
    let branch = git::branch(&run.path).expect("branch");
    run.done_at(2);
    run.at(2 + 179);
    assert!(matches!(run.state(), QueueState::Started { .. }));
    run.at(2 + 181);
    run.wait_for("the run is removed", |state| *state == QueueState::Removed);
    assert!(!run.path.exists());
    let repo = run.world.app.projects[0].path.clone();
    wait_until_within("its branch goes", Duration::from_secs(30), || !git::has_branch(&repo, &branch));
    assert!(run.world.app.projects.workspace_by(run.workspace).is_none());
}

#[test]
fn working_again_during_the_grace_starts_the_countdown_over() {
    let mut run = started(true);
    run.done_at(2);
    run.report("working");
    run.at(120);
    run.report("done");
    run.at(121);
    run.at(2 + 200);
    assert!(matches!(run.state(), QueueState::Started { .. }));
    assert!(run.path.exists());
}

#[rstest::rstest]
#[case::waiting("blocked:kind=permission")]
#[case::looked_at("idle")]
#[case::failed("error")]
fn a_run_that_is_not_done_never_removes_its_workspace(#[case] state: &str) {
    let mut run = started(true);
    run.report("working");
    run.at(1);
    run.report(state);
    run.at(2);
    run.at(2 + 1000);
    run.at(2 + 2000);
    assert!(matches!(run.state(), QueueState::Started { .. }));
    assert!(run.path.exists());
}

#[test]
fn an_uncommitted_file_keeps_the_workspace_and_says_so() {
    let mut run = started(true);
    std::fs::write(run.path.join("notes.txt"), "work in progress").expect("write");
    run.done_at(2);
    run.at(2 + 181);
    run.wait_for("the run is kept", |state| matches!(state, QueueState::Kept { .. }));
    let QueueState::Kept { reason, .. } = run.state() else { panic!("kept") };
    assert!(reason.contains("1 uncommitted"), "{reason}");
    assert!(run.path.exists());
    let toast = run.world.app.messages.toast.as_ref().expect("notice");
    assert!(toast.message.starts_with("routine fix the login left changes in "), "{}", toast.message);
}

#[test]
fn a_commit_that_was_not_pushed_keeps_the_workspace() {
    let mut run = started(true);
    std::fs::write(run.path.join("done.txt"), "finished").expect("write");
    run_git(&run.path, &["add", "--all"]);
    run_git(&run.path, &["commit", "--quiet", "-m", "work"]);
    run.done_at(2);
    run.at(2 + 181);
    run.wait_for("the run is kept", |state| matches!(state, QueueState::Kept { .. }));
    let QueueState::Kept { reason, .. } = run.state() else { panic!("kept") };
    assert!(reason.contains("1 unpushed"), "{reason}");
    assert!(run.path.exists());
}

#[test]
fn removing_the_run_from_the_list_during_the_grace_keeps_the_workspace() {
    let mut run = started(true);
    run.done_at(2);
    run.world.app.panels.queued.remove(run.run).expect("remove the run");
    run.at(2 + 181);
    run.at(2 + 400);
    assert!(run.path.exists());
    assert!(run.world.app.projects.workspace_by(run.workspace).is_some());
}

#[test]
fn a_routine_without_cleanup_keeps_every_workspace() {
    let mut run = started(false);
    run.done_at(2);
    run.at(2 + 1000);
    assert!(matches!(run.state(), QueueState::Started { .. }));
    assert!(run.path.exists());
}
