use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn stamp(secs: u64) -> String {
    Stamp(at(secs)).to_string()
}

fn held(name: &str, changed: Option<u64>, spec: &str) -> Held<String> {
    Held { name: name.into(), changed: changed.map(at), spec: spec.into() }
}

fn sent(plan: Plan) -> Vec<String> {
    plan.send
}

mod a_flow_of_yours {
    use super::*;

    #[test]
    fn is_sent_when_the_machine_has_none_of_its_name() {
        let plan = plan(&[held("Duo", Some(10), "a")], &[]);

        assert_eq!(sent(plan), ["Duo"]);
    }

    #[test]
    fn is_sent_over_an_older_copy() {
        let plan = plan(&[held("Duo", Some(20), "new")], &[held("duo", Some(10), "old")]);

        assert_eq!(sent(plan), ["Duo"]);
    }

    #[test]
    fn is_sent_over_a_copy_saved_before_flows_had_a_time() {
        let plan = plan(&[held("Duo", Some(20), "new")], &[held("Duo", None, "old")]);

        assert_eq!(sent(plan), ["Duo"]);
    }

    #[test]
    fn leaves_a_copy_saved_there_later_alone() {
        let plan = plan(&[held("Duo", Some(10), "old")], &[held("Duo", Some(20), "new")]);

        assert_eq!((plan.send, plan.kept), (Vec::<String>::new(), vec!["Duo".to_string()]));
    }

    #[test]
    fn is_not_sent_again_when_the_machine_has_the_same() {
        let plan = plan(&[held("Duo", Some(10), "a")], &[held("Duo", Some(10), "a")]);

        assert_eq!((plan.send, plan.same), (Vec::<String>::new(), vec!["Duo".to_string()]));
    }

    #[test]
    fn without_a_time_on_either_side_is_sent_only_when_it_differs() {
        let differs = plan(&[held("Duo", None, "a")], &[held("Duo", None, "b")]);
        let same = plan(&[held("Duo", None, "a")], &[held("Duo", None, "a")]);

        assert_eq!((sent(differs), sent(same)), (vec!["Duo".to_string()], Vec::new()));
    }

    #[test]
    fn only_on_the_machine_is_left_alone() {
        let plan = plan(&[], &[held("Theirs", None, "a")]);

        assert_eq!(plan, Plan::default());
    }
}

mod times {
    use super::*;

    #[test]
    fn the_same_copy_with_the_same_time_ahead_is_the_same_not_kept() {
        let plan = plan(&[held("Duo", Some(4_000), "a")], &[held("Duo", Some(4_000), "a")]);

        assert_eq!((plan.same, plan.kept), (vec!["Duo".to_string()], Vec::new()));
    }

    #[test]
    fn a_time_beats_none_and_none_beats_nothing() {
        assert!(newer(Some(at(1)), None));
        assert!(!newer(None, Some(at(1))));
        assert!(!newer(Some(at(1)), Some(at(1))));
    }

    #[test]
    fn only_a_time_past_the_skew_is_in_the_future() {
        let now = at(1_000);
        assert!(!in_the_future(&stamp(1_000 + 60), now));
        assert!(in_the_future(&stamp(1_000 + 3_600), now));
        assert!(!in_the_future("not a time", now));
    }
}
