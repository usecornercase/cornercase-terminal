use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use parking_lot::Mutex;
use rstest::rstest;

use super::*;

const WALL: i64 = 1_793_000_000;

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

mod a_wake {
    use super::*;

    #[rstest]
    #[case::a_tick(1, Duration::from_millis(500), false)]
    #[case::a_busy_loop(6, secs(6), false)]
    #[case::four_seconds_away(4, Duration::ZERO, false)]
    #[case::five_seconds_away(5, Duration::ZERO, true)]
    #[case::a_night(36_000, Duration::from_millis(500), true)]
    #[case::the_wall_clock_set_back(-3_600, Duration::from_millis(500), false)]
    fn is_wall_time_running_five_seconds_ahead_of_monotonic_time(
        #[case] wall: i64,
        #[case] monotonic: Duration,
        #[case] woke: bool,
    ) {
        let before = Instant::now();

        assert_eq!(slept(WALL, before, WALL + wall, before + monotonic), woke);
    }

    #[test]
    fn is_seen_once_between_two_refreshes() {
        let start = Instant::now();
        let mut sleep = Sleep::default();

        let seen = [
            sleep.woke(WALL, start),
            sleep.woke(WALL + 3_600, start + secs(1)),
            sleep.woke(WALL + 3_601, start + secs(2)),
        ];

        assert_eq!(seen, [false, true, false]);
    }
}

fn route(v4: Option<[u8; 4]>, v6: Option<u16>) -> Route {
    Route {
        v4: v4.map(|octets| IpAddr::V4(Ipv4Addr::from(octets))),
        v6: v6.map(|last| IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, last))),
    }
}

const HOME: Option<[u8; 4]> = Some([192, 168, 1, 20]);
const CAFE: Option<[u8; 4]> = Some([10, 0, 0, 7]);

mod a_route {
    use super::*;

    #[rstest]
    #[case::a_new_address(route(HOME, None), route(CAFE, None), true)]
    #[case::the_same_address(route(HOME, Some(1)), route(HOME, Some(1)), false)]
    #[case::from_no_route_to_one(route(None, None), route(HOME, None), true)]
    #[case::lost(route(HOME, None), route(None, None), true)]
    #[case::a_new_ipv6_address_alone(route(HOME, Some(1)), route(HOME, Some(2)), true)]
    fn changes_when_a_source_address_does(#[case] before: Route, #[case] now: Route, #[case] moved: bool) {
        assert_eq!(changed(Some(&before), &now), moved);
    }

    #[test]
    fn read_for_the_first_time_is_no_change() {
        assert!(!changed(None, &route(HOME, None)));
    }
}

struct Probe {
    routes: Arc<Mutex<VecDeque<Route>>>,
}

impl Probe {
    fn of(routes: &[Route]) -> (Network, Self) {
        let routes = Arc::new(Mutex::new(routes.iter().copied().collect::<VecDeque<_>>()));
        let given = Arc::clone(&routes);
        (Network::new(Box::new(move || given.lock().pop_front().unwrap_or_default())), Self { routes })
    }

    fn left(&self) -> usize {
        self.routes.lock().len()
    }
}

mod the_network {
    use super::*;

    #[test]
    fn is_read_every_two_seconds_at_most() {
        let start = Instant::now();
        let (mut network, probe) = Probe::of(&[route(HOME, None); 3]);

        network.moved(start);
        network.moved(start + secs(1));
        assert_eq!(probe.left(), 2);
        network.moved(start + secs(2));
        assert_eq!(probe.left(), 1);
    }

    #[test]
    fn moves_when_the_default_route_s_address_changes() {
        let start = Instant::now();
        let (mut network, _probe) = Probe::of(&[route(HOME, None), route(HOME, None), route(CAFE, None)]);

        let moves = [network.moved(start), network.moved(start + secs(2)), network.moved(start + secs(4))];

        assert_eq!(moves, [false, false, true]);
    }

    #[test]
    fn starts_over_after_a_rest() {
        let start = Instant::now();
        let (mut network, probe) = Probe::of(&[route(HOME, None), route(CAFE, None)]);

        network.moved(start);
        network.rest();

        assert!(!network.moved(start + secs(1)));
        assert_eq!(probe.left(), 0);
    }
}
