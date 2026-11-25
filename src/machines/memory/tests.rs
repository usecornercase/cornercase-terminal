use std::time::{Duration, Instant};

use rstest::rstest;
use serde_json::json;

use super::*;
use crate::text::GB;

fn there(name: &str, available: u64, age: u64) -> Candidate {
    Candidate {
        name: Some(name.into()),
        memory: Some(Memory { total: 32 * GB, available }),
        age: Some(Duration::from_secs(age)),
        left: None,
        reserved: 0,
    }
}

fn here(available: u64) -> Candidate {
    Candidate::here(Some(Memory { total: 16 * GB, available }), 0)
}

fn fits(name: Option<&str>, free: u64) -> Picked {
    Picked::Fits(Best { name: name.map(String::from), free })
}

mod picking {
    use super::*;

    #[rstest]
    #[case::the_most_free_memory(
        vec![here(2 * GB), there("torre", 11 * GB, 5), there("mac", 4 * GB, 5)],
        fits(Some("torre"), 11 * GB)
    )]
    #[case::this_computer_on_a_tie(vec![there("torre", 8 * GB, 5), here(8 * GB)], fits(None, 8 * GB))]
    #[case::this_computer_when_it_has_most(vec![here(20 * GB), there("torre", 11 * GB, 5)], fits(None, 20 * GB))]
    #[case::exactly_what_it_needs(vec![here(GB), there("torre", 6 * GB, 5)], fits(Some("torre"), 6 * GB))]
    fn takes_the_machine_with_most_room(#[case] candidates: Vec<Candidate>, #[case] picked: Picked) {
        assert_eq!(pick(6 * GB, &candidates), picked);
    }

    #[test]
    fn subtracts_what_it_reserved() {
        let reserved = Candidate { reserved: 8 * GB, ..there("torre", 11 * GB, 5) };

        assert_eq!(pick(GB, &[here(4 * GB), reserved]), fits(None, 4 * GB));
    }

    #[test]
    fn a_stale_reading_is_left_out() {
        let candidates = [here(GB), there("torre", 30 * GB, 61)];

        assert_eq!(
            pick(6 * GB, &candidates),
            Picked::Nowhere {
                needs: 6 * GB,
                best: Some(Best { name: None, free: GB }),
                left: vec![("torre".into(), Left::Stale)],
            }
        );
    }

    #[rstest]
    #[case::unreachable(Left::Unreachable)]
    #[case::no_project(Left::NoProject)]
    #[case::connecting(Left::Connecting)]
    fn a_machine_it_cannot_use_is_left_out(#[case] why: Left) {
        let candidates = [here(GB), Candidate { left: Some(why.clone()), ..there("torre", 30 * GB, 1) }];

        assert_eq!(
            pick(2 * GB, &candidates),
            Picked::Nowhere {
                needs: 2 * GB,
                best: Some(Best { name: None, free: GB }),
                left: vec![("torre".into(), why)]
            }
        );
    }

    #[test]
    fn an_unknown_memory_is_never_picked() {
        let older = Candidate { memory: None, ..there("old", 0, 1) };

        assert_eq!(
            pick(GB, &[older]),
            Picked::Nowhere { needs: GB, best: None, left: vec![("old".into(), Left::Unknown)] }
        );
    }
}

mod saying_why {
    use super::*;

    #[test]
    fn a_fit_names_the_machine_and_its_room() {
        assert_eq!(fits(Some("torre"), GB * 113 / 10).why("web-shop"), "auto → torre (11.3 GB free)");
    }

    #[test]
    fn a_choice_in_a_dialog_also_says_what_it_needed() {
        let best = Best { name: None, free: GB * 113 / 10 };

        assert_eq!(best.chose(6 * GB), "auto → this computer (11.3 GB free, needs 6 GB)");
    }

    #[test]
    fn a_wait_names_the_best_and_what_is_left_out() {
        let picked = Picked::Nowhere {
            needs: 6 * GB,
            best: Some(Best { name: Some("torre".into()), free: GB * 41 / 10 }),
            left: vec![("mac".into(), Left::Unreachable), ("pi".into(), Left::NoProject)],
        };

        assert_eq!(picked.why("web-shop"), "needs 6 GB · best: torre 4.1 GB · mac unreachable · pi has no web-shop");
        assert_eq!(picked.refusal("web-shop"), "no machine has 6 GB free (best: torre, 4.1 GB)");
    }

    #[test]
    fn a_refusal_with_no_candidate_says_why_each_was_left_out() {
        let picked = Picked::Nowhere { needs: 6 * GB, best: None, left: vec![("torre".into(), Left::Unreachable)] };

        assert_eq!(picked.refusal("web-shop"), "no machine has 6 GB free: needs 6 GB · torre unreachable");
    }
}

mod reservations {
    use super::*;

    #[test]
    fn add_up_per_machine_for_five_minutes() {
        let start = Instant::now();
        let mut reservations = Reservations::default();
        reservations.reserve(Some("Torre"), 3 * GB, start);
        reservations.reserve(Some("torre"), 2 * GB, start + Duration::from_mins(2));
        reservations.reserve(None, GB, start);

        assert_eq!(reservations.on(Some("torre"), start + Duration::from_mins(3)), 5 * GB);
        assert_eq!(reservations.on(None, start + Duration::from_mins(3)), GB);
        assert_eq!(reservations.on(Some("torre"), start + Duration::from_mins(6)), 2 * GB);
        assert_eq!(reservations.on(Some("torre"), start + Duration::from_mins(8)), 0);
    }
}

#[rstest]
#[case::lower("auto", true)]
#[case::upper(" AUTO ", true)]
#[case::a_name("torre", false)]
fn auto_is_a_word_not_a_machine(#[case] text: &str, #[case] auto: bool) {
    assert_eq!(is_auto(text), auto);
}

#[rstest]
#[case::read(json!({"total": 8 * GB, "available": 3 * GB}), Some(Memory { total: 8 * GB, available: 3 * GB }))]
#[case::more_available_than_total(json!({"total": GB, "available": 2 * GB}), Some(Memory { total: GB, available: GB }))]
#[case::no_total(json!({"available": GB}), None)]
fn a_reading_is_known_only_with_a_total(#[case] sent: serde_json::Value, #[case] memory: Option<Memory>) {
    let info: MemoryInfo = serde_json::from_value(sent).expect("a reading");

    assert_eq!(info.memory(), memory);
}
