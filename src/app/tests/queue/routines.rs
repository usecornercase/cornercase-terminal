use super::*;
use crate::queue::repeat::Repeat;
use crate::test_util::{clone_of, git, head};

pub(super) struct Routines {
    pub(super) app: App,
    pub(super) rx: Receiver<AppEvent>,
    pub(super) origin: TempDir,
    _clone: TempDir,
    pub(super) config: TempDir,
    _worktrees: TempDir,
}

pub(super) fn routines() -> Routines {
    let origin = git_repo(&[("README", "hello")]);
    let (clone_dir, clone) = clone_of(origin.path());
    let (mut app, rx, config, worktrees) = undrawn_queued_app_in(&clone);
    render(&mut app, AREA);
    Routines { app, rx, origin, _clone: clone_dir, config, _worktrees: worktrees }
}

pub(super) fn routine(app: &App, at: i64, every: &str) -> Task {
    let place = Place { project: app.projects[0].path.clone(), ..Place::default() };
    let repeat = Repeat::parse(every, "UTC", at).expect("repeat");
    Task { repeat: Some(repeat), place, ..task(app, at, None) }
}

pub(super) fn runs(app: &App, routine: u64) -> Vec<queue::Entry> {
    app.panels.queued.queue().runs_of(routine).cloned().collect()
}

fn all_started(app: &App, id: u64, count: usize) -> bool {
    let started = |run: &queue::Entry| matches!(run.state, QueueState::Started { .. });
    let found = runs(app, id);
    found.len() == count && found.iter().all(started)
}

pub(super) fn start_flowing(world: &mut Routines, id: u64, count: usize) {
    let Routines { app, rx, .. } = world;
    wait_until_within("the runs start", Duration::from_secs(60), || {
        drain(app, rx);
        app.refresh(Instant::now());
        all_started(app, id, count)
    });
}

mod runs_in_fresh_worktrees {
    use super::*;

    #[test]
    fn each_occurrence_starts_a_run_from_the_newest_commit_of_origin() {
        let mut world = routines();
        let at = time::now() + 3600;
        let id = world.app.panels.queued.schedule(None, routine(&world.app, at, "2h")).expect("routine");
        std::fs::write(world.origin.path().join("news"), "x").expect("write");
        git(world.origin.path(), &["add", "--all"]);
        git(world.origin.path(), &["commit", "--quiet", "-m", "news"]);
        world.app.refresh_at(Instant::now(), at);
        start_flowing(&mut world, id, 1);
        let run = runs(&world.app, id).remove(0);
        let path = run.task.place.workspace.clone().expect("the run's workspace");
        assert!(git::branch(&path).is_some_and(|branch| branch.starts_with("routine/fix-the-login-")));
        assert_eq!(head(&path), head(world.origin.path()));
        wait_until("the fake gets the prompt", || {
            std::fs::read_to_string(path.join("got")).is_ok_and(|t| !t.is_empty())
        });
        assert_eq!(std::fs::read_to_string(path.join("answered")).expect("trust"), "");
        assert_eq!(world.app.panels.queued.queue().entry(id).expect("routine").task.at, at + 7200);
    }

    #[test]
    fn a_run_starts_while_the_previous_one_still_works_and_branches_never_collide() {
        let mut world = routines();
        let at = time::now() + 3600;
        let id = world.app.panels.queued.schedule(None, routine(&world.app, at, "30m")).expect("routine");
        world.app.run_queued(id, Instant::now()).expect("first");
        world.app.run_queued(id, Instant::now()).expect("second");
        start_flowing(&mut world, id, 2);
        let found = runs(&world.app, id);
        let branches: Vec<_> = found.iter().filter_map(|run| run.task.place.branch.clone()).collect();
        assert_eq!(branches.len(), 2);
        assert_ne!(branches[0], branches[1]);
        assert!(branches.iter().any(|branch| branch.ends_with("-2")));
    }

    #[test]
    fn a_failing_fetch_fails_the_run_and_the_routine_goes_on() {
        let mut world = routines();
        git(&world.app.projects[0].path, &["remote", "set-url", "origin", "/nonexistent/folder"]);
        let at = time::now() + 3600;
        let id = world.app.panels.queued.schedule(None, routine(&world.app, at, "2h")).expect("routine");
        world.app.refresh_at(Instant::now(), at);
        let Routines { app, rx, .. } = &mut world;
        refreshing(app, rx, "the run fails", |app| {
            runs(app, id)
                .iter()
                .any(|run| matches!(&run.state, QueueState::Failed { reason } if reason.contains("could not fetch")))
        });
        let routine = world.app.panels.queued.queue().entry(id).expect("routine");
        assert_eq!((routine.state.clone(), routine.task.at), (QueueState::Pending, at + 7200));
    }
}

mod missing_and_pausing {
    use super::*;

    #[test]
    fn several_missed_occurrences_record_one_missed_run_and_say_so() {
        let mut world = routines();
        world.app.set_queue_area(AREA);
        world.app.clients.events.subscribe(9, Vec::new(), false, true);
        let start = time::now() - 7 * 86_400 - 1000;
        let id = world.app.panels.queued.schedule(None, routine(&world.app, start, "2h")).expect("routine");
        world.app.refresh_at(Instant::now(), time::now());
        let told = messages(&world.app.take_events(), 9);
        assert_eq!(told.iter().filter(|(kind, ..)| kind == "queue-missed").count(), 1, "{told:?}");
        let found = runs(&world.app, id);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].state, QueueState::Missed);
        assert!(world.app.panels.queued.queue().entry(id).expect("routine").task.at > time::now());
        let toast = world.app.messages.toast.as_ref().expect("a notice");
        assert!(toast.message.contains("missed its run"), "{}", toast.message);
    }

    #[test]
    fn start_now_adds_a_run_and_keeps_the_schedule() {
        let mut world = routines();
        let at = time::now() + 3600;
        let id = world.app.panels.queued.schedule(None, routine(&world.app, at, "2h")).expect("routine");
        world.app.run_queued(id, Instant::now()).expect("start now");
        start_flowing(&mut world, id, 1);
        assert_eq!(world.app.panels.queued.queue().entry(id).expect("routine").task.at, at);
    }

    #[test]
    fn a_program_that_is_missing_fails_every_run_at_once() {
        let mut world = routines();
        let at = time::now() + 3600;
        let task = Task { program: "/nonexistent/agent".into(), ..routine(&world.app, at, "day 09:00") };
        let id = world.app.panels.queued.schedule(None, task).expect("routine");
        world.app.refresh_at(Instant::now(), world.app.panels.queued.queue().entry(id).expect("routine").task.at);
        let found = runs(&world.app, id);
        assert!(matches!(&found[0].state, QueueState::Failed { reason } if reason.contains("not installed")));
        let toast = world.app.messages.toast.as_ref().expect("a notice");
        assert!(toast.message.starts_with("routine fix the login failed"), "{}", toast.message);
    }

    #[test]
    fn the_tick_wakes_at_a_routines_time_and_not_while_it_is_paused() {
        let mut world = routines();
        let at = time::now() + 3600;
        let id = world.app.panels.queued.schedule(None, routine(&world.app, at, "2h")).expect("routine");
        assert_eq!(world.app.tick_at(Instant::now(), at - 10), Some(Duration::from_secs(10)));
        world.app.panels.queued.pause(id, true, at - 10).expect("pause");
        assert_eq!(world.app.tick_at(Instant::now(), at - 10), None);
    }
}

mod across_a_clock_change {
    use super::*;

    #[test]
    fn a_daily_routine_stays_at_nine_local_when_the_tick_crosses_the_change() {
        let mut world = routines();
        world.app.set_queue_area(AREA);
        let zone = "Europe/Madrid";
        let eve: jiff::Timestamp = "2026-10-24T07:00:00Z".parse().expect("time");
        let at = eve.as_second();
        let place = Place { project: world.app.projects[0].path.clone(), ..Place::default() };
        let repeat = Repeat::parse("day 09:00", zone, at - 86_400).expect("repeat");
        let mut routine =
            Task { repeat: Some(repeat), place, program: "/nonexistent/agent".into(), ..task(&world.app, at, None) };
        routine.zone = zone.into();
        let id = world.app.panels.queued.schedule(None, routine).expect("routine");
        assert_eq!(world.app.tick_at(Instant::now(), at - 10), Some(Duration::from_secs(10)));
        world.app.refresh_at(Instant::now(), at);
        let next = world.app.panels.queued.queue().entry(id).expect("routine").task.at;
        assert_eq!(next, at + 86_400 + 3600);
        assert_eq!(world.app.tick_at(Instant::now(), at + 10), Some(Duration::from_secs(86_400 + 3600 - 10)));
    }
}

#[test]
fn an_account_removed_between_runs_fails_the_next_run_with_its_reason() {
    use super::accounts::{claude, with_profiles};
    let mut world = routines();
    with_profiles(&mut world.app, &world.config);
    let at = time::now() + 3600;
    let task = Task { agent: "claude".into(), accounts: claude("home"), ..routine(&world.app, at, "2h") };
    let id = world.app.panels.queued.schedule(None, task).expect("routine");
    crate::accounts::remove(&world.config.path().join("accounts"), agents::Known::Claude, "home").expect("remove");
    world.app.relist_accounts();
    world.app.run_queued(id, Instant::now()).expect("start now");
    let found = runs(&world.app, id);
    assert!(
        matches!(&found[0].state, QueueState::Failed { reason } if reason.contains("there is no claude account `home`")),
        "{:?}",
        found[0].state
    );
    assert_eq!(world.app.panels.queued.queue().entry(id).expect("routine").state, QueueState::Pending);
}
