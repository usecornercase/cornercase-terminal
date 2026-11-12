use std::collections::HashMap;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use super::*;
use crate::remote::tunnels::{self, Kept};
use crate::test_util::TempDir;

type Calls = Arc<Mutex<Vec<(String, Forward, u64)>>>;
type Owners = Arc<Mutex<HashMap<i32, Owner>>>;

const FIRST_PID: i32 = 7_000_000;

struct Fake {
    calls: Calls,
    owners: Owners,
    refuse: Option<String>,
    base: i32,
}

impl Spawn for Fake {
    fn spawn(&mut self, destination: &str, forward: Forward, attempt: u64) -> Result<Started, String> {
        let mut calls = self.calls.lock();
        calls.push((destination.into(), forward, attempt));
        let pid = self.base + i32::try_from(calls.len()).expect("a few calls");
        self.refuse.clone().map_or(Ok(Started { _running: None, pid: Some(pid) }), Err)
    }

    fn owner(&self, pid: i32, _destination: &str, _forward: Forward) -> Owner {
        self.owners.lock().get(&pid).copied().unwrap_or(Owner::Gone)
    }
}

struct Rig {
    tunnels: Tunnels,
    calls: Calls,
    owners: Owners,
    dir: TempDir,
}

impl Rig {
    fn file(&self) -> PathBuf {
        self.dir.path().join("tunnels.json")
    }

    fn kept(&self) -> Vec<Kept> {
        tunnels::read(&self.file(), "vps")
    }

    fn write(&self, entries: &[Kept]) {
        for entry in entries {
            tunnels::update(&self.file(), "vps", entry.port, |_| Some(*entry)).expect("written");
        }
    }
}

fn rig_before(refuse: Option<&str>, entries: &[Kept]) -> Rig {
    let (calls, owners, dir) = (Calls::default(), Owners::default(), TempDir::new());
    let fake =
        Fake { calls: calls.clone(), owners: owners.clone(), refuse: refuse.map(str::to_string), base: FIRST_PID };
    let file = dir.path().join("tunnels.json");
    let tunnels = Tunnels::new(Box::new(fake), Openers::default(), file);
    let mut rig = Rig { tunnels, calls, owners, dir };
    rig.write(entries);
    rig.tunnels.sync(Some("vps"), Instant::now());
    rig
}

fn rig() -> Rig {
    rig_before(None, &[])
}

fn free_port() -> u16 {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind a port");
    socket.local_addr().expect("the port").port()
}

fn tunnels_with(refuse: Option<&str>) -> (Tunnels, Calls, TempDir) {
    let rig = rig_before(refuse, &[]);
    (rig.tunnels, rig.calls, rig.dir)
}

fn state_of(tunnels: &Tunnels, port: u16) -> Option<TunnelState> {
    tunnels.holding().tunnels.into_iter().find(|tunnel| tunnel.port == port).map(|tunnel| tunnel.state)
}

fn attempt_of(calls: &Calls) -> u64 {
    calls.lock().last().expect("a start").2
}

mod opening {
    use super::*;

    #[test]
    fn starts_one_ssh_on_the_same_port_when_it_is_free_and_reports_opening() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port), Instant::now());

        assert_eq!(
            calls.lock().iter().map(|(to, forward, _)| (to.clone(), *forward)).collect::<Vec<_>>(),
            [("vps".to_string(), Forward { port, local: port })]
        );
        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Opening));
    }

    #[test]
    fn asking_again_while_it_opens_starts_nothing_more() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port), Instant::now());
        tunnels.order(TunnelOrder::Open(port), Instant::now());

        assert_eq!(calls.lock().len(), 1);
    }

    #[test]
    fn a_spawn_that_fails_is_down_with_its_reason() {
        let (mut tunnels, _calls, _dir) = tunnels_with(Some("no ssh"));
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port), Instant::now());

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Down("no ssh".into())));
    }

    #[test]
    fn port_zero_is_ignored() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);

        tunnels.order(TunnelOrder::Open(0), Instant::now());

        assert!(calls.lock().is_empty() && tunnels.holding().tunnels.is_empty());
    }

    #[test]
    fn the_thirty_third_is_down_with_the_limit() {
        let (mut tunnels, _calls, _dir) = tunnels_with(None);
        (1..=32).for_each(|port| tunnels.order(TunnelOrder::Open(40_000 + port), Instant::now()));

        tunnels.order(TunnelOrder::Open(41_000), Instant::now());

        let Some(TunnelState::Down(reason)) = state_of(&tunnels, 41_000) else { panic!("it should be down") };
        assert!(reason.contains("at most 32"), "{reason}");
    }

    #[test]
    fn a_long_reason_becomes_one_line_of_at_most_300_characters() {
        let (mut tunnels, _calls, _dir) = tunnels_with(Some(&format!("first\n{}", "x".repeat(500))));
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port), Instant::now());

        let Some(TunnelState::Down(reason)) = state_of(&tunnels, port) else { panic!("it should be down") };
        assert!(!reason.contains('\n') && reason.chars().count() == 300, "{reason}");
    }
}

mod watching {
    use super::*;

    fn up(tunnels: &mut Tunnels, calls: &Calls, port: u16) -> Instant {
        let now = Instant::now();
        tunnels.order(TunnelOrder::Open(port), Instant::now());
        assert!(tunnels.watched(attempt_of(calls), Watched::Up, now));
        now
    }

    #[test]
    fn up_is_listed_with_its_local_port() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();

        up(&mut tunnels, &calls, port);

        let listed = tunnels.holding().tunnels;
        assert_eq!(listed, [Tunnel { port, local: Some(port), state: TunnelState::Up }]);
    }

    #[test]
    fn ending_before_it_was_up_is_down_and_never_retried() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();
        let now = Instant::now();
        tunnels.order(TunnelOrder::Open(port), Instant::now());

        assert!(tunnels.watched(attempt_of(&calls), Watched::Ended("refused".into()), now));
        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Opening));
        assert!(tunnels.tick(now + CHECK_EVERY, true));

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Down("refused".into())));
        assert!(!tunnels.tick(now + Duration::from_secs(600), true));
        assert_eq!(calls.lock().len(), 1);
        assert_eq!(tunnels.next_wake(true), None);
    }

    #[test]
    fn ending_after_it_was_up_is_retried_after_the_backoff() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();
        let now = up(&mut tunnels, &calls, port);

        assert!(tunnels.watched(attempt_of(&calls), Watched::Ended("dropped".into()), now));

        assert_eq!(tunnels.next_wake(true), Some(now + remote::backoff(1)));
        assert!(!tunnels.tick(now, true));
        assert!(tunnels.tick(now + remote::backoff(1), true));
        assert_eq!(calls.lock().len(), 2);
        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Opening));
    }

    #[test]
    fn nothing_starts_while_the_window_is_not_linked() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();
        let now = up(&mut tunnels, &calls, port);
        tunnels.watched(attempt_of(&calls), Watched::Ended("dropped".into()), now);

        assert!(!tunnels.tick(now + Duration::from_secs(600), false));
        assert_eq!(tunnels.next_wake(false), None);
        assert_eq!(calls.lock().len(), 1);
    }

    #[test]
    fn a_late_report_of_a_stopped_ssh_is_ignored() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port), Instant::now());
        let old = attempt_of(&calls);
        tunnels.order(TunnelOrder::Close(port), Instant::now());
        tunnels.order(TunnelOrder::Open(port), Instant::now());

        assert!(!tunnels.watched(old, Watched::Up, Instant::now()));

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Opening));
    }
}

mod limits {
    use super::*;

    #[test]
    fn a_port_over_the_cap_starts_nothing_however_often_it_is_asked_or_the_window_links() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        (1..=32).for_each(|port| tunnels.order(TunnelOrder::Open(40_000 + port), Instant::now()));
        let started = calls.lock().len();

        tunnels.order(TunnelOrder::Open(41_000), Instant::now());
        tunnels.order(TunnelOrder::Open(41_000), Instant::now());
        tunnels.sync(Some("vps"), Instant::now());

        assert_eq!(calls.lock().len(), started);
    }

    #[test]
    fn ports_asked_over_the_cap_do_not_pile_up() {
        let (mut tunnels, _calls, _dir) = tunnels_with(None);

        (1..=500).for_each(|port| tunnels.order(TunnelOrder::Open(30_000 + port), Instant::now()));

        assert!(tunnels.holding().tunnels.len() <= 64);
    }

    #[test]
    fn a_retry_that_ends_before_it_is_up_is_retried_after_a_longer_backoff() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();
        let now = Instant::now();
        tunnels.order(TunnelOrder::Open(port), Instant::now());
        tunnels.watched(attempt_of(&calls), Watched::Up, now);
        tunnels.watched(attempt_of(&calls), Watched::Ended("dropped".into()), now);
        tunnels.tick(now + remote::backoff(1), true);

        tunnels.watched(attempt_of(&calls), Watched::Ended("refused".into()), now);

        assert_eq!(tunnels.next_wake(true), Some(now + remote::backoff(2)));
    }

    #[test]
    fn a_report_counts_the_orders_the_window_applied() {
        let (mut tunnels, _calls, _dir) = tunnels_with(None);

        tunnels.order(TunnelOrder::Open(free_port()), Instant::now());
        tunnels.order(TunnelOrder::Close(1), Instant::now());

        assert_eq!(tunnels.holding().orders, 2);
    }
}

mod linking {
    use super::*;

    #[test]
    fn the_same_machine_again_starts_the_down_tunnels_again() {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port), Instant::now());
        let now = Instant::now();
        tunnels.watched(attempt_of(&calls), Watched::Ended("refused".into()), now);
        tunnels.tick(now + CHECK_EVERY, true);

        tunnels.sync(Some("vps"), Instant::now());

        assert_eq!((calls.lock().len(), state_of(&tunnels, port)), (2, Some(TunnelState::Opening)));
    }

    #[test]
    fn another_machine_or_this_one_clears_every_tunnel() {
        let (mut tunnels, _calls, _dir) = tunnels_with(None);
        tunnels.order(TunnelOrder::Open(free_port()), Instant::now());

        tunnels.sync(Some("gpu"), Instant::now());
        assert_eq!(tunnels.holding().tunnels, Vec::new());

        tunnels.order(TunnelOrder::Open(free_port()), Instant::now());
        tunnels.sync(None, Instant::now());
        assert_eq!(tunnels.holding().tunnels, Vec::new());
    }

    #[test]
    fn closing_keeps_the_tunnel_listed_as_closed() {
        let (mut tunnels, _calls, _dir) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port), Instant::now());

        tunnels.order(TunnelOrder::Close(port), Instant::now());

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Closed));
    }
}

mod browsing {
    use super::*;

    fn browse(ip: Option<std::net::IpAddr>, port: u16) -> Browse {
        Browse { ip, port }
    }

    fn up_on(local: u16) -> (Tunnels, u16) {
        let (mut tunnels, calls, _dir) = tunnels_with(None);
        tunnels.order(TunnelOrder::Open(local), Instant::now());
        tunnels.watched(attempt_of(&calls), Watched::Up, Instant::now());
        (tunnels, local)
    }

    #[test]
    fn maps_a_tunnelled_port_to_its_local_one() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listen");
        let port = held.local_addr().expect("the port").port();
        let (tunnels, _) = up_on(port);
        let local = tunnels.holding().tunnels[0].local.expect("a local port");

        assert_ne!(local, port);
        assert_eq!(tunnels.browse(browse(None, port)), Ok(browse(None, local)));
        assert_eq!(tunnels.browse(browse(Some(Ipv4Addr::LOCALHOST.into()), port)), Ok(browse(None, local)));
    }

    #[test]
    fn refuses_a_port_it_does_not_tunnel_and_a_lan_address() {
        let (tunnels, port) = up_on(free_port());

        assert!(tunnels.browse(browse(None, port.wrapping_add(1))).is_err());
        assert!(tunnels.browse(browse(Some(Ipv4Addr::new(192, 168, 1, 5).into()), port)).is_err());
    }

    #[test]
    fn refuses_a_port_that_is_not_up() {
        let (mut tunnels, _calls, _dir) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port), Instant::now());

        assert!(tunnels.browse(browse(None, port)).is_err());
    }
}

fn entry(port: u16, local: Option<u16>, pid: Option<i32>, closed: bool) -> Kept {
    Kept { port, local, pid, closed }
}

fn open(rig: &mut Rig, port: u16) {
    rig.tunnels.order(TunnelOrder::Open(port), Instant::now());
}

mod remembering {
    use super::*;

    #[test]
    fn opening_writes_the_port_the_local_port_and_the_pid() {
        let mut rig = rig();
        let port = free_port();

        open(&mut rig, port);

        assert_eq!(rig.kept(), [entry(port, Some(port), Some(FIRST_PID + 1), false)]);
    }

    #[test]
    fn closing_keeps_the_entry_as_closed_without_a_pid() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);

        rig.tunnels.order(TunnelOrder::Close(port), Instant::now());

        assert_eq!(rig.kept(), [entry(port, Some(port), None, true)]);
        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Closed));
    }

    #[test]
    fn opening_a_closed_one_starts_it_again_and_clears_the_flag() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);
        rig.tunnels.order(TunnelOrder::Close(port), Instant::now());

        open(&mut rig, port);

        assert_eq!(rig.calls.lock().len(), 2);
        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Opening));
        assert!(!rig.kept()[0].closed);
    }

    #[test]
    fn forgetting_removes_it_from_the_report_and_from_the_file() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);

        rig.tunnels.order(TunnelOrder::Forget(port), Instant::now());

        assert_eq!(state_of(&rig.tunnels, port), None);
        assert_eq!(rig.kept(), Vec::<Kept>::new());
    }

    #[test]
    fn closing_a_port_that_is_not_held_writes_nothing() {
        let mut rig = rig();

        rig.tunnels.order(TunnelOrder::Close(5173), Instant::now());

        assert!(!rig.file().exists());
    }
}

mod linking_to_a_machine_that_was_remembered {
    use super::*;

    #[test]
    fn starts_the_open_entries_on_their_local_port_and_lists_the_closed_ones_without_starting_them() {
        let (port, local) = (free_port(), free_port());
        let rig = rig_before(None, &[entry(port, Some(local), None, false), entry(3000, None, None, true)]);

        let calls = rig.calls.lock().clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, Forward { port, local: port });
        assert_eq!(state_of(&rig.tunnels, 3000), Some(TunnelState::Closed));
        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Opening));
    }

    #[test]
    fn falls_back_on_the_remembered_port_when_the_remote_port_is_taken_here() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listen");
        let port = held.local_addr().expect("the port").port();
        let local = free_port();

        let rig = rig_before(None, &[entry(port, Some(local), None, false)]);

        assert_eq!(rig.calls.lock()[0].1, Forward { port, local });
    }

    #[test]
    fn ignores_the_entries_of_another_machine() {
        let rig = rig();
        tunnels::update(&rig.file(), "other", 4000, |_| Some(entry(4000, None, None, false))).expect("written");
        let mut tunnels = rig.tunnels;

        tunnels.sync(Some("vps"), Instant::now());

        assert_eq!(tunnels.holding().tunnels, Vec::new());
    }

    #[test]
    fn a_closed_flag_written_by_another_window_stops_a_running_tunnel_and_a_new_entry_starts() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);
        let other = free_port();
        rig.write(&[entry(port, Some(port), None, true), entry(other, None, None, false)]);

        rig.tunnels.sync(Some("vps"), Instant::now());

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Closed));
        assert_eq!(state_of(&rig.tunnels, other), Some(TunnelState::Opening));
    }

    #[test]
    fn a_held_port_missing_from_the_file_is_kept() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);
        std::fs::remove_file(rig.file()).expect("remove");

        rig.tunnels.sync(Some("vps"), Instant::now());

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Opening));
    }
}

mod sharing {
    use super::*;

    fn pid(n: i32) -> i32 {
        FIRST_PID * 2 + n
    }

    fn elsewhere(owner: Owner) -> (Rig, u16, Instant) {
        let port = free_port();
        let rig = rig_before(None, &[]);
        rig.owners.lock().insert(pid(1), owner);
        rig.write(&[entry(port, Some(port), Some(pid(1)), false)]);
        let now = Instant::now();
        let mut rig = rig;
        rig.tunnels.held.clear();
        rig.tunnels.sync(Some("vps"), now);
        (rig, port, now)
    }

    #[test]
    fn an_owner_that_listens_makes_it_up_on_the_recorded_port_with_no_ssh() {
        let (rig, port, _) = elsewhere(Owner::Listening);

        assert_eq!(rig.tunnels.holding().tunnels, [Tunnel { port, local: Some(port), state: TunnelState::Up }]);
        assert!(rig.calls.lock().is_empty());
    }

    #[test]
    fn an_owner_that_has_not_listened_yet_is_opening_with_no_ssh() {
        let (rig, port, _) = elsewhere(Owner::Starting);

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Opening));
        assert!(rig.calls.lock().is_empty());
    }

    #[test]
    fn a_dead_owner_starts_its_own_ssh_and_rewrites_the_pid() {
        let (rig, port, _) = elsewhere(Owner::Gone);

        assert_eq!(rig.calls.lock().len(), 1);
        assert_eq!(rig.kept(), [entry(port, Some(port), Some(FIRST_PID + 1), false)]);
    }

    #[test]
    fn the_owner_is_looked_at_again_and_taken_over_when_it_goes() {
        let (mut rig, port, now) = elsewhere(Owner::Listening);
        rig.owners.lock().insert(pid(1), Owner::Gone);

        assert_eq!(rig.tunnels.next_wake(true), Some(now + CHECK_EVERY));
        assert!(rig.tunnels.tick(now + CHECK_EVERY, true));

        assert_eq!(rig.calls.lock().len(), 1);
        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Opening));
        assert_eq!(rig.kept()[0].pid, Some(FIRST_PID + 1));
    }

    #[test]
    fn an_owner_that_starts_to_listen_turns_opening_into_up() {
        let (mut rig, port, now) = elsewhere(Owner::Starting);
        rig.owners.lock().insert(pid(1), Owner::Listening);

        assert!(rig.tunnels.tick(now + CHECK_EVERY, true));

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Up));
        assert_eq!(rig.tunnels.next_wake(true), Some(now + CHECK_EVERY * 2));
    }

    #[test]
    fn nothing_is_taken_over_while_the_window_is_not_linked() {
        let (mut rig, _, now) = elsewhere(Owner::Listening);
        rig.owners.lock().insert(pid(1), Owner::Gone);

        assert!(!rig.tunnels.tick(now + CHECK_EVERY, false));

        assert!(rig.calls.lock().is_empty());
    }

    #[test]
    fn a_take_over_that_finds_the_entry_closed_stays_closed() {
        let (mut rig, port, now) = elsewhere(Owner::Listening);
        rig.owners.lock().insert(pid(1), Owner::Gone);
        rig.write(&[entry(port, Some(port), None, true)]);

        rig.tunnels.tick(now + CHECK_EVERY, true);

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Closed));
        assert!(rig.calls.lock().is_empty());
    }

    #[test]
    fn a_take_over_that_finds_the_entry_forgotten_removes_the_port() {
        let (mut rig, port, now) = elsewhere(Owner::Listening);
        rig.owners.lock().insert(pid(1), Owner::Gone);
        tunnels::forget(&rig.file(), "vps", port).expect("forgotten");

        rig.tunnels.tick(now + CHECK_EVERY, true);

        assert_eq!(state_of(&rig.tunnels, port), None);
        assert!(rig.calls.lock().is_empty());
    }

    #[test]
    fn a_take_over_that_finds_another_window_s_new_owner_watches_it() {
        let (mut rig, port, now) = elsewhere(Owner::Listening);
        rig.owners.lock().insert(pid(1), Owner::Gone);
        rig.owners.lock().insert(pid(2), Owner::Listening);
        rig.write(&[entry(port, Some(port), Some(pid(2)), false)]);

        rig.tunnels.tick(now + CHECK_EVERY, true);

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Up));
        assert!(rig.calls.lock().is_empty());
    }

    #[test]
    fn our_ssh_failing_before_it_was_up_adopts_the_owner_that_won_the_port() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);
        rig.owners.lock().insert(pid(3), Owner::Listening);
        rig.write(&[entry(port, Some(port), Some(pid(3)), false)]);
        let attempt = attempt_of(&rig.calls);

        assert!(rig.tunnels.watched(attempt, Watched::Ended("Address already in use".into()), Instant::now()));

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Up));
        assert_eq!(rig.calls.lock().len(), 1);
    }

    #[test]
    fn ports_held_elsewhere_do_not_count_toward_the_cap() {
        let (mut rig, _, _) = elsewhere(Owner::Listening);
        (1..=32).for_each(|port| open(&mut rig, 40_000 + port));

        assert_eq!(rig.calls.lock().len(), 32);
        let Some(TunnelState::Opening) = state_of(&rig.tunnels, 40_032) else { panic!("it should open") };
    }

    #[test]
    fn browsing_maps_a_port_held_elsewhere_to_its_local_one() {
        let (rig, port, _) = elsewhere(Owner::Listening);

        let mapped = rig.tunnels.browse(Browse { ip: None, port });

        assert_eq!(mapped, Ok(Browse { ip: None, port }));
    }
}

mod late_reports {
    use super::*;

    #[test]
    fn from_an_ssh_closed_here_are_ignored() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);
        let old = attempt_of(&rig.calls);
        rig.tunnels.order(TunnelOrder::Close(port), Instant::now());

        assert!(!rig.tunnels.watched(old, Watched::Up, Instant::now()));
        assert!(!rig.tunnels.watched(old, Watched::Ended("gone".into()), Instant::now()));

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Closed));
    }

    #[test]
    fn from_an_ssh_given_up_for_another_window_s_are_ignored() {
        let mut rig = rig();
        let port = free_port();
        open(&mut rig, port);
        let old = attempt_of(&rig.calls);
        rig.owners.lock().insert(FIRST_PID * 2, Owner::Listening);
        rig.write(&[entry(port, Some(port), Some(FIRST_PID * 2), false)]);
        rig.tunnels.converge(port, Instant::now());

        assert!(!rig.tunnels.watched(old, Watched::Ended("stopped".into()), Instant::now()));

        assert_eq!(state_of(&rig.tunnels, port), Some(TunnelState::Up));
    }
}

mod converging {
    use super::*;

    struct Pair {
        a: Rig,
        b: Tunnels,
        b_calls: Calls,
        port: u16,
    }

    fn pair() -> Pair {
        let a = rig();
        let b_calls = Calls::default();
        let fake = Fake { calls: b_calls.clone(), owners: a.owners.clone(), refuse: None, base: FIRST_PID * 3 };
        let mut b = Tunnels::new(Box::new(fake), Openers::default(), a.file());
        b.sync(Some("vps"), Instant::now());
        Pair { a, b, b_calls, port: free_port() }
    }

    fn both_spawned(pair: &mut Pair) -> (i32, i32) {
        open(&mut pair.a, pair.port);
        let first = FIRST_PID + 1;
        pair.a.write(&[entry(pair.port, None, None, false)]);
        pair.b.order(TunnelOrder::Open(pair.port), Instant::now());
        let second = FIRST_PID * 3 + 1;
        pair.a.owners.lock().insert(first, Owner::Listening);
        pair.a.owners.lock().insert(second, Owner::Listening);
        (first, second)
    }

    #[test]
    fn two_windows_that_both_came_up_keep_one_ssh_between_them() {
        let mut pair = pair();
        let (_, second) = both_spawned(&mut pair);
        let attempt = attempt_of(&pair.a.calls);

        pair.a.tunnels.watched(attempt, Watched::Up, Instant::now());
        pair.b.watched(attempt_of(&pair.b_calls), Watched::Up, Instant::now());

        assert_eq!(pair.a.tunnels.held[&pair.port].elsewhere, Some(second));
        assert!(pair.a.tunnels.held[&pair.port].started.is_none());
        assert_eq!(state_of(&pair.a.tunnels, pair.port), Some(TunnelState::Up));
        assert!(pair.b.held[&pair.port].started.is_some());
        assert_eq!(pair.a.kept()[0].pid, Some(second));
    }

    #[test]
    fn the_loser_of_a_bind_waits_for_the_winner_instead_of_settling_down() {
        let mut pair = pair();
        let (first, second) = both_spawned(&mut pair);
        let (attempt_a, attempt_b) = (attempt_of(&pair.a.calls), attempt_of(&pair.b_calls));
        pair.a.owners.lock().insert(second, Owner::Gone);
        let now = Instant::now();

        pair.b.watched(attempt_b, Watched::Ended("Address already in use".into()), now);
        assert_eq!(state_of_held(&pair.b, pair.port), Some(TunnelState::Opening));
        pair.a.tunnels.watched(attempt_a, Watched::Up, now);
        assert_eq!(pair.a.kept()[0].pid, Some(first));
        pair.b.tick(now + CHECK_EVERY, true);

        assert_eq!(state_of_held(&pair.b, pair.port), Some(TunnelState::Up));
        assert_eq!(pair.b.held[&pair.port].elsewhere, Some(first));
    }

    fn state_of_held(tunnels: &Tunnels, port: u16) -> Option<TunnelState> {
        state_of(tunnels, port)
    }

    #[test]
    fn a_take_over_that_fails_before_it_is_up_is_retried() {
        let mut rig = rig();
        let port = free_port();
        let pid = FIRST_PID * 2 + 1;
        rig.owners.lock().insert(pid, Owner::Listening);
        rig.write(&[entry(port, Some(port), Some(pid), false)]);
        let now = Instant::now();
        rig.tunnels.sync(Some("vps"), now);
        rig.owners.lock().insert(pid, Owner::Gone);
        rig.tunnels.tick(now + CHECK_EVERY, true);
        let attempt = attempt_of(&rig.calls);

        rig.tunnels.watched(attempt, Watched::Ended("refused".into()), now + CHECK_EVERY);

        let wake = rig.tunnels.next_wake(true).expect("a retry is planned");
        assert!(rig.tunnels.tick(wake, true));
        assert_eq!(rig.calls.lock().len(), 2);
    }
}
