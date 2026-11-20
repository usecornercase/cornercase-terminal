use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::test_util::TempDir;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn stamp(secs: u64) -> String {
    Stamp(at(secs)).to_string()
}

fn held(name: &str, changed: Option<u64>, spec: &str) -> Held<String> {
    Held { name: name.into(), changed: changed.map(at), spec: spec.into() }
}

fn ours(flows: Vec<Held<String>>) -> Ours<String> {
    Ours { flows, hidden: Vec::new(), events: Vec::new(), synced: Some(at(1)), retry: Vec::new() }
}

fn theirs(flows: Vec<Held<String>>) -> Theirs<String> {
    Theirs { flows, hidden: Vec::new(), builtins: vec!["Quick".into(), "Arena".into()] }
}

fn changed(events: Vec<Event>) -> Ours<String> {
    Ours { events, ..ours(Vec::new()) }
}

fn retried(events: Vec<Event>, failed: Event) -> Ours<String> {
    Ours { synced: Some(at(40)), retry: vec![failed], ..changed(events) }
}

fn event(name: &str, change: Change, secs: u64) -> Event {
    Event { name: name.into(), change, at: stamp(secs) }
}

mod a_flow_of_yours {
    use super::*;

    #[test]
    fn is_sent_when_the_machine_has_none_of_its_name() {
        let plan = plan(&ours(vec![held("Duo", Some(10), "a")]), &theirs(Vec::new()));

        assert_eq!(plan.ops, [Op::Send("Duo".into())]);
    }

    #[test]
    fn is_sent_over_an_older_copy() {
        let plan = plan(&ours(vec![held("Duo", Some(20), "new")]), &theirs(vec![held("duo", Some(10), "old")]));

        assert_eq!(plan.ops, [Op::Send("Duo".into())]);
    }

    #[test]
    fn is_sent_over_a_copy_saved_before_flows_had_a_time() {
        let plan = plan(&ours(vec![held("Duo", Some(20), "new")]), &theirs(vec![held("Duo", None, "old")]));

        assert_eq!(plan.ops, [Op::Send("Duo".into())]);
    }

    #[test]
    fn leaves_a_copy_saved_there_later_alone() {
        let plan = plan(&ours(vec![held("Duo", Some(10), "old")]), &theirs(vec![held("Duo", Some(20), "new")]));

        assert_eq!((plan.ops, plan.kept), (Vec::new(), vec!["Duo".to_string()]));
    }

    #[test]
    fn is_not_sent_again_when_the_machine_has_the_same() {
        let plan = plan(&ours(vec![held("Duo", Some(10), "a")]), &theirs(vec![held("Duo", Some(10), "a")]));

        assert_eq!((plan.ops, plan.same), (Vec::new(), vec!["Duo".to_string()]));
    }

    #[test]
    fn without_a_time_on_either_side_is_sent_only_when_it_differs() {
        let differs = plan(&ours(vec![held("Duo", None, "a")]), &theirs(vec![held("Duo", None, "b")]));
        let same = plan(&ours(vec![held("Duo", None, "a")]), &theirs(vec![held("Duo", None, "a")]));

        assert_eq!((differs.ops, same.ops), (vec![Op::Send("Duo".into())], Vec::new()));
    }

    #[test]
    fn only_on_the_machine_is_never_deleted() {
        let plan = plan(&ours(Vec::new()), &theirs(vec![held("Theirs", None, "a")]));

        assert_eq!(plan, Plan::default());
    }
}

mod a_deleted_flow {
    use super::*;

    #[test]
    fn is_deleted_there_when_its_copy_is_older() {
        let ours = changed(vec![event("Duo", Change::Deleted, 30)]);

        let plan = plan(&ours, &theirs(vec![held("Duo", Some(20), "a")]));

        assert_eq!(plan.ops, [Op::Delete("Duo".into())]);
    }

    #[test]
    fn stays_there_when_it_was_saved_there_after() {
        let ours = changed(vec![event("Duo", Change::Deleted, 30)]);

        let plan = plan(&ours, &theirs(vec![held("Duo", Some(40), "a")]));

        assert_eq!((plan.ops, plan.kept), (Vec::new(), vec!["Duo".to_string()]));
    }

    #[test]
    fn of_an_edited_built_in_restores_it_there() {
        let ours = changed(vec![event("Quick", Change::Deleted, 30)]);

        let plan = plan(&ours, &theirs(vec![held("Quick", Some(20), "a")]));

        assert_eq!(plan.ops, [Op::Restore("Quick".into())]);
    }

    #[test]
    fn is_not_sent_again_after_a_sync_that_followed_it() {
        let mut ours = changed(vec![event("Duo", Change::Deleted, 30)]);
        ours.synced = Some(at(40));

        let plan = plan(&ours, &theirs(vec![held("Duo", None, "a")]));

        assert_eq!(plan.ops, []);
    }

    #[test]
    fn is_never_sent_to_a_machine_that_never_synced() {
        let mut ours = changed(vec![event("Duo", Change::Deleted, 30)]);
        ours.synced = None;

        let plan = plan(&ours, &theirs(vec![held("Duo", None, "a")]));

        assert_eq!(plan.ops, []);
    }

    #[test]
    fn saved_again_here_is_sent_rather_than_deleted() {
        let mut ours = ours(vec![held("Duo", Some(40), "a")]);
        ours.events = vec![event("Duo", Change::Deleted, 30)];

        let plan = plan(&ours, &theirs(vec![held("Duo", Some(20), "old")]));

        assert_eq!(plan.ops, [Op::Send("Duo".into())]);
    }
}

mod a_built_in {
    use super::*;

    #[test]
    fn deleted_here_is_deleted_there() {
        let ours = changed(vec![event("Arena", Change::Hidden, 30)]);

        let plan = plan(&ours, &theirs(Vec::new()));

        assert_eq!(plan.ops, [Op::Hide("Arena".into())]);
    }

    #[test]
    fn already_deleted_there_is_left_alone() {
        let ours = changed(vec![event("Arena", Change::Hidden, 30)]);
        let mut theirs = theirs(Vec::new());
        theirs.hidden = vec!["arena".into()];

        assert_eq!(plan(&ours, &theirs).ops, []);
    }

    #[test]
    fn restored_here_is_restored_where_it_is_deleted() {
        let ours = changed(vec![event("Arena", Change::Restored, 30)]);
        let mut theirs = theirs(Vec::new());
        theirs.hidden = vec!["Arena".into()];

        assert_eq!(plan(&ours, &theirs).ops, [Op::Restore("Arena".into())]);
    }

    #[test]
    fn deleted_before_a_first_sync_is_deleted_there_too() {
        let mut ours = ours(Vec::new());
        ours.hidden = vec!["Arena".into(), "Gone".into()];
        ours.synced = None;

        assert_eq!(plan(&ours, &theirs(Vec::new())).ops, [Op::Hide("Arena".into())]);
    }

    #[test]
    fn deleted_before_a_first_sync_keeps_an_edit_saved_there() {
        let mut ours = ours(Vec::new());
        ours.hidden = vec!["Arena".into()];
        ours.synced = None;

        let plan = plan(&ours, &theirs(vec![held("Arena", Some(5), "theirs")]));

        assert_eq!((plan.ops, plan.kept), (Vec::new(), vec!["Arena".to_string()]));
    }
}

mod the_journal {
    use super::*;

    #[test]
    fn keeps_one_change_per_name_and_survives_a_read() {
        let dir = TempDir::new();
        let path = dir.path().join("flow-sync.json");
        let mut journal = Journal::default();
        journal.record("Duo", Change::Deleted, &stamp(10));
        journal.record(" duo ", Change::Hidden, &stamp(20));
        journal.synced.insert("vps.lan".into(), stamp(30));
        journal.write(&path).expect("write the journal");

        let read = Journal::read(&path);

        assert_eq!(read.events, [Event { name: "duo".into(), change: Change::Hidden, at: stamp(20) }]);
        assert_eq!(read.synced("vps.lan"), Some(at(30)));
    }

    #[test]
    fn forgets_a_name_saved_again() {
        let mut journal = Journal::default();
        journal.record("Duo", Change::Deleted, &stamp(10));

        assert!(journal.forget("DUO"));
        assert_eq!(journal.events, []);
    }

    #[test]
    fn keeps_only_the_latest_changes() {
        let mut journal = Journal::default();
        for n in 0..300 {
            journal.record(&format!("flow {n}"), Change::Deleted, &stamp(n));
        }

        assert_eq!(journal.events.len(), KEPT);
        assert_eq!(journal.events[0].name, "flow 44");
    }

    #[test]
    fn that_cannot_be_read_is_empty() {
        let dir = TempDir::new();
        let path = dir.path().join("flow-sync.json");
        std::fs::write(&path, "not json").expect("write");

        assert_eq!(Journal::read(&path), Journal::default());
    }
}

mod a_failed_change {
    use super::*;

    #[test]
    fn is_sent_again_though_a_later_round_moved_past_it() {
        let deleted = event("Duo", Change::Deleted, 30);
        let ours = retried(vec![deleted.clone()], deleted);

        let plan = plan(&ours, &theirs(vec![held("Duo", Some(20), "a")]));

        assert_eq!(plan.ops, [Op::Delete("Duo".into())]);
    }

    #[test]
    fn is_dropped_once_the_name_changed_again() {
        let ours = retried(vec![event("Arena", Change::Restored, 50)], event("Arena", Change::Hidden, 30));
        let mut theirs = theirs(Vec::new());
        theirs.hidden = vec!["Arena".into()];

        assert_eq!(plan(&ours, &theirs).ops, [Op::Restore("Arena".into())]);
    }

    #[test]
    fn is_kept_per_machine_until_a_round_has_none() {
        let mut journal = Journal::default();
        journal.round_ended("vps.lan", at(10), vec![event("Duo", Change::Deleted, 5)]);
        assert_eq!(journal.retry.get("vps.lan").map(Vec::len), Some(1));

        journal.round_ended("vps.lan", at(20), Vec::new());

        assert_eq!((journal.retry.len(), journal.synced("vps.lan")), (0, Some(at(20))));
    }
}

mod times {
    use super::*;

    #[test]
    fn a_time_in_the_future_is_sent_as_now() {
        assert_eq!(not_after(Some(at(50)), at(40)), Some(at(40)));
        assert_eq!(not_after(Some(at(30)), at(40)), Some(at(30)));
    }

    #[test]
    fn only_a_time_past_the_skew_is_in_the_future() {
        let now = at(1_000);
        assert!(!in_the_future(&stamp(1_000 + 60), now));
        assert!(in_the_future(&stamp(1_000 + 3_600), now));
        assert!(!in_the_future("not a time", now));
    }
}
