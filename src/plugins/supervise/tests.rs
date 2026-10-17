use super::*;
use crate::plugins::Registry;
use crate::plugins::manifest::FILE;
use crate::process;
use crate::test_util::{TempDir, wait_until, wait_until_within, write_executable};

const SERVICE: &str = "id = \"acme.notes\"\nname = \"Notes\"\nversion = \"1\"\n\
                       [[service]]\nid = \"sync\"\ncommand = [\"./serve\"]\n";

struct Fixture {
    home: TempDir,
    root: TempDir,
    places: Places,
}

impl Fixture {
    fn new(enabled: bool) -> Self {
        let (home, root) = (TempDir::new(), TempDir::new());
        fs::write(root.path().join(FILE), SERVICE).expect("manifest");
        write_executable(&root.path().join("serve"), "#!/bin/sh\necho $$ >> pids\nexec sleep 30\n");
        let places = Places::new(&home.path().join("config.json"), &home.path().join("session.json"));
        let mut registry = Registry::load(&places.registry);
        registry.link(root.path()).expect("link");
        registry.set_enabled("acme.notes", enabled).expect("switch");
        registry.save().expect("save");
        Self { home, root, places }
    }

    fn pids(&self) -> Vec<i32> {
        fs::read_to_string(self.root.path().join("pids"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse().ok())
            .collect()
    }

    fn switch(&self, on: bool) {
        let mut registry = Registry::load(&self.places.registry);
        registry.set_enabled("acme.notes", on).expect("switch");
        registry.save().expect("save");
    }

    fn running(&self) -> i32 {
        wait_until("the service starts", || self.pids().last().is_some_and(|pid| process::alive(*pid)));
        *self.pids().last().expect("a pid")
    }
}

#[test]
fn a_service_that_ends_starts_again() {
    let fixture = Fixture::new(true);
    let supervisor = Supervisor::start(Kind::Service, fixture.places.clone(), Vec::new());
    let first = fixture.running();
    let _ = rustix::process::kill_process(rustix::process::Pid::from_raw(first).expect("pid"), Signal::KILL);
    wait_until_within("it starts again", Duration::from_secs(10), || fixture.pids().len() == 2);
    assert!(supervisor.note("acme.notes").is_some());
    let log = fs::read_to_string(fixture.places.log("acme.notes")).expect("log");
    assert_eq!(log.matches("service started").count(), 2, "{log}");
    drop((supervisor, fixture.home));
}

#[test]
fn turning_a_plugin_off_stops_its_service() {
    let fixture = Fixture::new(true);
    let _supervisor = Supervisor::start(Kind::Service, fixture.places.clone(), Vec::new());
    let pid = fixture.running();
    fixture.switch(false);
    wait_until("the service stops", || !process::alive(pid));
}

#[test]
fn stopping_the_supervisor_stops_every_service() {
    let fixture = Fixture::new(true);
    let supervisor = Supervisor::start(Kind::Service, fixture.places.clone(), Vec::new());
    let pid = fixture.running();
    drop(supervisor);
    assert!(!process::alive(pid));
}

#[test]
fn nothing_runs_until_the_plugin_is_turned_on() {
    let fixture = Fixture::new(false);
    let supervisor = Supervisor::start(Kind::Service, fixture.places.clone(), Vec::new());
    wait_until("the supervisor looked twice", || supervisor.steps() >= 2);
    assert_eq!(fixture.pids(), Vec::<i32>::new());
    fixture.switch(true);
    fixture.running();
}

#[test]
fn window_processes_are_not_services() {
    let fixture = Fixture::new(true);
    let windows = Supervisor::start(Kind::Window, fixture.places.clone(), Vec::new());
    wait_until("the supervisor looked twice", || windows.steps() >= 2);
    drop(windows);
    assert_eq!(fixture.pids(), Vec::<i32>::new());
}

#[rstest::rstest]
#[case::first(1, 1)]
#[case::second(2, 2)]
#[case::fifth(5, 16)]
#[case::at_most_a_minute(12, 60)]
fn waits_longer_after_each_failure(#[case] failures: u32, #[case] seconds: u64) {
    assert_eq!(backoff(failures), Duration::from_secs(seconds));
}

#[test]
fn a_process_that_cannot_be_recorded_is_stopped() {
    let fixture = Fixture::new(true);
    let state = fixture.places.state("acme.notes");
    fs::create_dir_all(&state).expect("state folder");
    fs::write(state.join(RUNNING_DIR), "not a folder").expect("running as a file");
    let supervisor = Supervisor::start(Kind::Service, fixture.places.clone(), Vec::new());
    let log = || fs::read_to_string(fixture.places.log("acme.notes")).unwrap_or_default();
    wait_until("it is stopped again", || log().contains("service stopped") && log().contains("error="));
    let log = log();
    let pid: i32 = log
        .split("service started")
        .nth(1)
        .and_then(|rest| rest.split("pid=").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|pid| pid.parse().ok())
        .expect("the started pid");
    wait_until("the process is gone", || !process::alive(pid));
    drop((supervisor, fixture.home));
}

#[test]
fn a_plugin_turned_off_on_disk_is_not_started_from_a_stale_list() {
    let fixture = Fixture::new(true);
    let plugin = Registry::load(&fixture.places.registry).plugins().remove(0);
    let command = plugin.runs().expect("runs").service[0].command.clone();
    fixture.switch(false);
    let mut state = State {
        kind: Kind::Service,
        places: fixture.places.clone(),
        env: Vec::new(),
        notes: Notes::default(),
        running: BTreeMap::new(),
        waiting: BTreeMap::new(),
    };
    let key = Key { plugin: "acme.notes".into(), entry: "sync".into() };
    state.spawn(&key, &Spec { plugin, command }, Instant::now());
    assert!(state.running.is_empty() && fixture.pids().is_empty());
}

#[test]
fn a_spawn_is_skipped_while_removal_holds_the_lock() {
    let fixture = Fixture::new(true);
    let plugin = Registry::load(&fixture.places.registry).plugins().remove(0);
    let command = plugin.runs().expect("runs").service[0].command.clone();
    let held = lock_file(&fixture.places, "acme.notes").expect("lock file");
    flock(&held, FlockOperation::NonBlockingLockExclusive).expect("exclusive");
    let mut state = State {
        kind: Kind::Service,
        places: fixture.places.clone(),
        env: Vec::new(),
        notes: Notes::default(),
        running: BTreeMap::new(),
        waiting: BTreeMap::new(),
    };
    let key = Key { plugin: "acme.notes".into(), entry: "sync".into() };
    state.spawn(&key, &Spec { plugin, command }, Instant::now());
    assert!(state.running.is_empty() && fixture.pids().is_empty());
}
