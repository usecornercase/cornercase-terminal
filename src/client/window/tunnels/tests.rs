use std::net::{Ipv4Addr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use super::*;

type Calls = Arc<Mutex<Vec<(String, Forward, u64)>>>;

struct Fake {
    calls: Calls,
    refuse: Option<String>,
}

impl Spawn for Fake {
    fn spawn(&mut self, destination: &str, forward: Forward, attempt: u64) -> Result<Started, String> {
        self.calls.lock().push((destination.into(), forward, attempt));
        self.refuse.clone().map_or(Ok(Started::default()), Err)
    }
}

fn free_port() -> u16 {
    let socket = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind a port");
    socket.local_addr().expect("the port").port()
}

fn tunnels_with(refuse: Option<&str>) -> (Tunnels, Calls) {
    let calls = Calls::default();
    let fake = Fake { calls: calls.clone(), refuse: refuse.map(str::to_string) };
    let mut tunnels = Tunnels::new(Box::new(fake), Openers::default());
    tunnels.sync(Some("vps"));
    (tunnels, calls)
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
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port));

        assert_eq!(
            calls.lock().iter().map(|(to, forward, _)| (to.clone(), *forward)).collect::<Vec<_>>(),
            [("vps".to_string(), Forward { port, local: port })]
        );
        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Opening));
    }

    #[test]
    fn asking_again_while_it_opens_starts_nothing_more() {
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port));
        tunnels.order(TunnelOrder::Open(port));

        assert_eq!(calls.lock().len(), 1);
    }

    #[test]
    fn a_spawn_that_fails_is_down_with_its_reason() {
        let (mut tunnels, _calls) = tunnels_with(Some("no ssh"));
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port));

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Down("no ssh".into())));
    }

    #[test]
    fn port_zero_is_ignored() {
        let (mut tunnels, calls) = tunnels_with(None);

        tunnels.order(TunnelOrder::Open(0));

        assert!(calls.lock().is_empty() && tunnels.holding().tunnels.is_empty());
    }

    #[test]
    fn the_thirty_third_is_down_with_the_limit() {
        let (mut tunnels, _calls) = tunnels_with(None);
        (1..=32).for_each(|port| tunnels.order(TunnelOrder::Open(40_000 + port)));

        tunnels.order(TunnelOrder::Open(41_000));

        let Some(TunnelState::Down(reason)) = state_of(&tunnels, 41_000) else { panic!("it should be down") };
        assert!(reason.contains("at most 32"), "{reason}");
    }

    #[test]
    fn a_long_reason_becomes_one_line_of_at_most_300_characters() {
        let (mut tunnels, _calls) = tunnels_with(Some(&format!("first\n{}", "x".repeat(500))));
        let port = free_port();

        tunnels.order(TunnelOrder::Open(port));

        let Some(TunnelState::Down(reason)) = state_of(&tunnels, port) else { panic!("it should be down") };
        assert!(!reason.contains('\n') && reason.chars().count() == 300, "{reason}");
    }
}

mod watching {
    use super::*;

    fn up(tunnels: &mut Tunnels, calls: &Calls, port: u16) -> Instant {
        let now = Instant::now();
        tunnels.order(TunnelOrder::Open(port));
        assert!(tunnels.watched(attempt_of(calls), Watched::Up, now));
        now
    }

    #[test]
    fn up_is_listed_with_its_local_port() {
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();

        up(&mut tunnels, &calls, port);

        let listed = tunnels.holding().tunnels;
        assert_eq!(listed, [Tunnel { port, local: Some(port), state: TunnelState::Up }]);
    }

    #[test]
    fn ending_before_it_was_up_is_down_and_never_retried() {
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();
        let now = Instant::now();
        tunnels.order(TunnelOrder::Open(port));

        assert!(tunnels.watched(attempt_of(&calls), Watched::Ended("refused".into()), now));

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Down("refused".into())));
        assert!(!tunnels.tick(now + Duration::from_secs(600), true));
        assert_eq!(calls.lock().len(), 1);
        assert_eq!(tunnels.next_wake(true), None);
    }

    #[test]
    fn ending_after_it_was_up_is_retried_after_the_backoff() {
        let (mut tunnels, calls) = tunnels_with(None);
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
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();
        let now = up(&mut tunnels, &calls, port);
        tunnels.watched(attempt_of(&calls), Watched::Ended("dropped".into()), now);

        assert!(!tunnels.tick(now + Duration::from_secs(600), false));
        assert_eq!(tunnels.next_wake(false), None);
        assert_eq!(calls.lock().len(), 1);
    }

    #[test]
    fn a_late_report_of_a_stopped_ssh_is_ignored() {
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port));
        let old = attempt_of(&calls);
        tunnels.order(TunnelOrder::Close(port));
        tunnels.order(TunnelOrder::Open(port));

        assert!(!tunnels.watched(old, Watched::Up, Instant::now()));

        assert_eq!(state_of(&tunnels, port), Some(TunnelState::Opening));
    }
}

mod limits {
    use super::*;

    #[test]
    fn a_port_over_the_cap_starts_nothing_however_often_it_is_asked_or_the_window_links() {
        let (mut tunnels, calls) = tunnels_with(None);
        (1..=32).for_each(|port| tunnels.order(TunnelOrder::Open(40_000 + port)));
        let started = calls.lock().len();

        tunnels.order(TunnelOrder::Open(41_000));
        tunnels.order(TunnelOrder::Open(41_000));
        tunnels.sync(Some("vps"));

        assert_eq!(calls.lock().len(), started);
    }

    #[test]
    fn ports_asked_over_the_cap_do_not_pile_up() {
        let (mut tunnels, _calls) = tunnels_with(None);

        (1..=500).for_each(|port| tunnels.order(TunnelOrder::Open(30_000 + port)));

        assert!(tunnels.holding().tunnels.len() <= 64);
    }

    #[test]
    fn a_retry_that_ends_before_it_is_up_is_retried_after_a_longer_backoff() {
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();
        let now = Instant::now();
        tunnels.order(TunnelOrder::Open(port));
        tunnels.watched(attempt_of(&calls), Watched::Up, now);
        tunnels.watched(attempt_of(&calls), Watched::Ended("dropped".into()), now);
        tunnels.tick(now + remote::backoff(1), true);

        tunnels.watched(attempt_of(&calls), Watched::Ended("refused".into()), now);

        assert_eq!(tunnels.next_wake(true), Some(now + remote::backoff(2)));
    }

    #[test]
    fn a_report_counts_the_orders_the_window_applied() {
        let (mut tunnels, _calls) = tunnels_with(None);

        tunnels.order(TunnelOrder::Open(free_port()));
        tunnels.order(TunnelOrder::Close(1));

        assert_eq!(tunnels.holding().orders, 2);
    }
}

mod linking {
    use super::*;

    #[test]
    fn the_same_machine_again_starts_the_down_tunnels_again() {
        let (mut tunnels, calls) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port));
        tunnels.watched(attempt_of(&calls), Watched::Ended("refused".into()), Instant::now());

        tunnels.sync(Some("vps"));

        assert_eq!((calls.lock().len(), state_of(&tunnels, port)), (2, Some(TunnelState::Opening)));
    }

    #[test]
    fn another_machine_or_this_one_clears_every_tunnel() {
        let (mut tunnels, _calls) = tunnels_with(None);
        tunnels.order(TunnelOrder::Open(free_port()));

        tunnels.sync(Some("gpu"));
        assert_eq!(tunnels.holding().tunnels, Vec::new());

        tunnels.order(TunnelOrder::Open(free_port()));
        tunnels.sync(None);
        assert_eq!(tunnels.holding().tunnels, Vec::new());
    }

    #[test]
    fn closing_removes_the_tunnel() {
        let (mut tunnels, _calls) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port));

        tunnels.order(TunnelOrder::Close(port));

        assert_eq!(state_of(&tunnels, port), None);
    }
}

mod browsing {
    use super::*;

    fn browse(ip: Option<std::net::IpAddr>, port: u16) -> Browse {
        Browse { ip, port }
    }

    fn up_on(local: u16) -> (Tunnels, u16) {
        let (mut tunnels, calls) = tunnels_with(None);
        tunnels.order(TunnelOrder::Open(local));
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
        let (mut tunnels, _calls) = tunnels_with(None);
        let port = free_port();
        tunnels.order(TunnelOrder::Open(port));

        assert!(tunnels.browse(browse(None, port)).is_err());
    }
}
