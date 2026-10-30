use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;

use super::*;
use crate::app::machines::{MachineEvent, Update};
use crate::control;
use crate::error::Behind;
use crate::host::network::Route;
use crate::machines::link::fake::{answer, line};
use crate::machines::link::{Connect, Event, Line, Report};
use crate::machines::wake::Network;
use crate::machines::{Machine, State, machine};
use crate::protocol::{self, ClientMessage};
use crate::remote::Killer;

const CLOCK: i64 = 1_793_000_000;

#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<String>>>);

impl Seen {
    fn note(&self, what: String) {
        self.0.lock().push(what);
    }

    fn count(&self, what: &str) -> usize {
        self.0.lock().iter().filter(|seen| *seen == what).count()
    }
}

#[derive(Clone)]
enum There {
    Missing,
    Older,
    Server(String),
}

struct Fake {
    destination: String,
    there: There,
    seen: Seen,
}

impl Connect for Fake {
    fn connect(&self, _: &Killer) -> Result<Line> {
        self.seen.note(format!("connect {}", self.destination));
        match &self.there {
            There::Missing => Err(Error::RemoteMissing { host: self.destination.clone(), option: "" }),
            There::Older => Err(Error::RemoteBehind(Box::new(Behind {
                host: self.destination.clone(),
                there: "0.0.1".into(),
                update: String::new(),
            }))),
            There::Server(boot) => Ok(serving(boot)),
        }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.seen.note(format!("end {}", self.destination));
    }
}

fn serving(boot: &str) -> Line {
    let (line, mut theirs) = line(version::CURRENT, None);
    let boot = boot.to_string();
    std::thread::spawn(move || {
        answer(&mut theirs, &boot, version::CURRENT);
        while matches!(protocol::recv::<ClientMessage>(&mut theirs), Ok(Some(_))) {}
    });
    line
}

struct Rig {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
    seen: Seen,
    start: Instant,
}

impl Rig {
    fn new(machines: Vec<Machine>, there: There) -> Self {
        let dir = TempDir::new();
        let (mut app, rx) = new_app(write_config(dir.path(), &Config { machines, ..Config::default() }));
        let seen = Seen::default();
        let given = seen.clone();
        app.machines.set_connector(Box::new(move |destination| {
            given.note(format!("start {destination}"));
            Box::new(Fake { destination: destination.into(), there: there.clone(), seen: given.clone() })
        }));
        app.machines.set_network(Network::new(Box::new(Route::default)));
        let start = Instant::now();
        app.refresh_at(start, CLOCK);
        Self { app, rx, dir, seen, start }
    }

    fn edit(&mut self, machines: Vec<Machine>, after: Duration) {
        write_config(self.dir.path(), &Config { machines, ..Config::default() });
        self.refresh(after);
    }

    fn refresh(&mut self, after: Duration) {
        self.app.refresh_at(self.start + after, CLOCK + i64::try_from(after.as_secs()).expect("seconds"));
    }

    fn until(&mut self, what: &str, mut done: impl FnMut(&App) -> bool) {
        let (app, rx) = (&mut self.app, &self.rx);
        wait_until(what, || {
            drain(app, rx);
            done(app)
        });
    }

    fn seen(&self, what: &str) -> usize {
        self.seen.count(what)
    }

    fn links(&self) -> Vec<u64> {
        self.app.machines.held().iter().map(|held| held.link.id()).collect()
    }

    fn state(&self, name: &str) -> Option<State> {
        self.app.machines.held().iter().find(|held| held.name == name).map(|held| held.status.state)
    }

    fn gated_updates(&mut self) -> (Sender<()>, Arc<Mutex<Vec<String>>>) {
        let (go, gate) = mpsc::channel::<()>();
        let (gate, asked) = (Mutex::new(gate), Arc::new(Mutex::new(Vec::new())));
        let given = Arc::clone(&asked);
        self.app.machines.set_updater(Arc::new(move |destination: &str| {
            given.lock().push(destination.to_string());
            let _ = gate.lock().recv();
            Ok(())
        }));
        (go, asked)
    }

    fn saved(&self) -> Vec<Machine> {
        config::load(&self.dir.path().join("config.json")).machines
    }

    fn reason(&self, name: &str) -> Option<String> {
        let held = self.app.machines.held().iter().find(|held| held.name == name)?;
        self.app.machines.reason(held)
    }
}

mod links {
    use super::*;

    #[test]
    fn a_machine_added_outside_gets_one_link_on_the_next_watch() {
        let mut rig = Rig::new(Vec::new(), There::Missing);
        assert_eq!(rig.links(), Vec::<u64>::new());

        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(2));
        rig.refresh(Duration::from_secs(3));

        assert_eq!((rig.links().len(), rig.seen("start vps.lan")), (1, 1));
    }

    #[test]
    fn a_new_name_icon_or_folder_keeps_the_same_link() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        let links = rig.links();

        let renamed = Machine { icon: "◆".into(), folder: "/srv".into(), ..machine("box", "vps.lan") };
        rig.edit(vec![renamed], Duration::from_secs(2));

        assert_eq!(rig.links(), links);
        assert_eq!(rig.app.machines.held()[0].name, "box");
        assert_eq!(rig.seen("start vps.lan"), 1);
    }

    #[test]
    fn a_new_destination_gets_a_new_link_and_ends_the_old_one() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        let links = rig.links();

        rig.edit(vec![machine("vps", "new.lan")], Duration::from_secs(2));

        assert_ne!(rig.links(), links);
        assert_eq!(rig.seen("start new.lan"), 1);
        wait_until("the old link ends", || rig.seen("end vps.lan") == 1);
    }

    #[test]
    fn a_removed_machine_has_its_link_dropped() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);

        rig.edit(Vec::new(), Duration::from_secs(2));

        assert_eq!(rig.links(), Vec::<u64>::new());
        wait_until("its link ends", || rig.seen("end vps.lan") == 1);
    }

    #[test]
    fn an_entry_with_a_problem_gets_no_link() {
        let rig = Rig::new(vec![machine("my vps", "vps.lan"), machine("pi", "")], There::Missing);

        assert_eq!(rig.links(), Vec::<u64>::new());
        assert_eq!(rig.seen("start vps.lan"), 0);
    }

    #[test]
    fn stopping_the_server_ends_every_link() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")], There::Missing);

        rig.app.stop_background();

        assert_eq!(rig.links(), Vec::<u64>::new());
        wait_until("both links end", || rig.seen("end vps.lan") == 1 && rig.seen("end pi.lan") == 1);
    }
}

mod states {
    use super::*;

    #[test]
    fn start_connecting_then_follow_their_link() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        assert_eq!(rig.state("vps"), Some(State::Connecting));

        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);

        assert_eq!(rig.reason("vps").as_deref(), Some("cornercase was not found on vps: install it there"));
    }

    #[test]
    fn a_machine_whose_server_answers_this_one_s_boot_is_this_computer() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Server(control::server_token().into()));

        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);

        assert_eq!(rig.reason("vps").as_deref(), Some("that is this computer"));
    }

    #[test]
    fn of_two_machines_on_one_server_only_one_is_linked_and_the_other_names_it() {
        let both = vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")];
        let mut rig = Rig::new(both, There::Server("shared-1".into()));

        rig.until("both links report", |app| {
            app.machines.held().iter().all(|held| held.status.state != State::Connecting)
        });

        let linked: Vec<&str> =
            ["vps", "pi"].into_iter().filter(|name| rig.state(name) == Some(State::Linked)).collect();
        let [linked] = linked[..] else { panic!("one machine is linked, not {linked:?}") };
        let other = if linked == "vps" { "pi" } else { "vps" };
        assert_eq!(rig.state(other), Some(State::Unreachable));
        assert_eq!(rig.reason(other), Some(format!("the same server as {linked}")));
    }

    #[test]
    fn a_linked_machine_has_its_versions_and_no_reason() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Server("vps-1".into()));

        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Linked);

        let status = &rig.app.machines.held()[0].status;
        assert_eq!(
            (status.version.as_deref(), status.server_version.as_deref()),
            (Some(version::CURRENT), Some(version::CURRENT))
        );
        assert_eq!(rig.reason("vps"), None);
    }

    #[test]
    fn a_report_from_a_link_no_longer_held_is_ignored() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        let gone =
            Report { link: 99, event: Event::Linked { release: "9.9.9".into(), server_version: "9.9.9".into() } };

        rig.app.machine_event(MachineEvent::Link(gone));

        assert_eq!(rig.state("vps"), Some(State::Connecting));
    }
}

mod pokes {
    use super::*;

    fn tried_once() -> Rig {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the first attempt fails", |app| app.machines.held()[0].status.state == State::Unreachable);
        assert_eq!(rig.seen("connect vps.lan"), 1);
        rig
    }

    #[test]
    fn a_wake_retries_every_link_at_once() {
        let mut rig = tried_once();

        rig.app.refresh_at(rig.start + Duration::from_millis(500), CLOCK + 3_600);

        wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
    }

    #[test]
    fn a_new_network_retries_every_link_at_once() {
        let mut rig = tried_once();
        let home = Route { v4: Some("192.168.1.20".parse().expect("an address")), v6: None };
        let mut routes = VecDeque::from([Route::default(), home]);
        rig.app.machines.set_network(Network::new(Box::new(move || routes.pop_front().unwrap_or(home))));

        rig.refresh(Duration::from_secs(1));
        assert_eq!(rig.seen("connect vps.lan"), 1);
        rig.refresh(Duration::from_secs(3));

        wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
    }

    #[test]
    fn an_edit_retries_a_machine_that_is_down() {
        let mut rig = tried_once();

        rig.edit(vec![Machine { folder: "/srv".into(), ..machine("vps", "vps.lan") }], Duration::from_secs(2));

        wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
        assert_eq!(rig.seen("start vps.lan"), 1);
    }
}

mod updates {
    use super::*;

    fn older() -> Rig {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")], There::Older);
        rig.until("the links report", |app| app.machines.held().iter().all(|held| held.status.state == State::Older));
        rig
    }

    #[test]
    fn run_once_at_a_time_on_the_machine_then_retry_its_link() {
        let mut rig = older();
        let (go, asked) = rig.gated_updates();

        assert!(matches!(rig.app.update_machine("vps"), Update::Started(_)));
        assert!(rig.app.machines.held()[0].status.updating);
        assert_eq!(rig.app.update_machine("vps"), Update::Running);
        go.send(()).expect("let the update finish");

        rig.until("the update ends", |app| !app.machines.held()[0].status.updating);
        wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
        assert_eq!(*asked.lock(), ["vps.lan"]);
    }

    #[test]
    fn that_fail_say_what_was_printed_there() {
        let mut rig = older();
        rig.app.machines.set_updater(Arc::new(|_: &str| {
            Err(Error::Ssh { host: "vps.lan".into(), reason: "brew update && brew upgrade cornercase".into() })
        }));

        rig.app.update_machine("pi");

        rig.until("the update ends", |app| !app.machines.held()[1].status.updating);
        assert_eq!(rig.reason("pi").as_deref(), Some("brew update && brew upgrade cornercase"));
        assert_eq!(rig.state("pi"), Some(State::Older));
    }

    #[test]
    fn are_only_for_an_older_machine_that_exists() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);

        assert_eq!(rig.app.update_machine("vps"), Update::NotOlder);
        assert_eq!(rig.app.update_machine("pi"), Update::Unknown);
    }
}

mod fakes {
    use super::*;

    #[test]
    fn an_app_test_that_fakes_no_machine_never_runs_ssh() {
        let dir = TempDir::new();
        let config = Config { machines: vec![machine("pi", "pi.lan")], ..Config::default() };
        let (mut app, rx) = new_app(write_config(dir.path(), &config));
        app.machines.set_network(Network::new(Box::new(Route::default)));
        app.refresh_at(Instant::now(), CLOCK);

        wait_until("the link reports", || {
            drain(&mut app, &rx);
            app.machines.held().first().is_some_and(|held| held.status.state == State::Unreachable)
        });

        assert_eq!(app.machines.reason(&app.machines.held()[0]).as_deref(), Some(NO_SSH));
    }
}

mod requests;
mod settings;
