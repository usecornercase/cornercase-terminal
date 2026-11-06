use super::*;

fn pairings(followers: &[(u64, PairKey, Instant)]) -> Pairings {
    let follower = |(client, key, until): &(u64, PairKey, Instant)| Follower {
        client: *client,
        key: *key,
        until: *until,
        shown: PairShown::default(),
        since: 0,
    };
    Pairings { followers: followers.iter().map(follower).collect() }
}

mod waking {
    use super::*;

    #[test]
    fn nothing_wakes_it_while_nobody_follows() {
        assert_eq!(Pairings::default().wake(Instant::now()), None);
    }

    #[test]
    fn the_nearest_end_does_and_one_already_past_wakes_it_now() {
        let now = Instant::now();
        let soon = now + Duration::from_mins(1);
        let later = now + Duration::from_mins(5);
        let both = pairings(&[(1, PairKey::Browsers, later), (2, PairKey::Browsers, soon)]);

        assert_eq!(both.wake(now), Some(Duration::from_mins(1)));
        assert_eq!(both.wake(soon + Duration::from_mins(1)), Some(Duration::ZERO));
    }
}

mod holding {
    use super::*;

    #[test]
    fn a_key_is_held_by_its_own_followers_and_a_client_follows_one() {
        let telegram = PairKey::Chat(Via::Telegram);
        let held = pairings(&[(1, telegram, Instant::now())]);

        assert!(held.holds(telegram));
        assert!(!held.holds(PairKey::Chat(Via::Discord)));
        assert!(!held.holds(PairKey::Browsers));
        assert!(held.follows(1));
        assert!(!held.follows(2));
    }
}
