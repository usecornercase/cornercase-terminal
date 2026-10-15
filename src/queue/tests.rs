use super::*;
use crate::persist;
use crate::test_util::TempDir;

fn task(at: i64) -> Task {
    Task { at, prompt: "fix the login".into(), zone: "UTC".into(), ..Task::default() }
}

mod scheduling {
    use super::*;

    #[test]
    fn starts_every_due_entry_in_time_then_insertion_order() {
        let mut queue = Queue::default();
        let later = queue.add(task(120)).expect("add later");
        let first = queue.add(task(100)).expect("add first");
        let second = queue.add(task(100)).expect("add second");
        assert_eq!(queue.next(90), Some(Duration::from_secs(10)));
        assert_eq!(queue.due(99), [] as [u64; 0]);
        assert_eq!(queue.due(100), [first, second]);
        assert_eq!(queue.entries.iter().map(|entry| entry.id).collect::<Vec<_>>(), [first, second, later]);
    }

    #[test]
    fn marks_late_entries_missed_and_keeps_the_grace_boundary_due() {
        let mut queue = Queue::default();
        let missed = queue.add(task(99)).expect("add missed");
        let grace = queue.add(task(100)).expect("add grace");
        assert_eq!(queue.due(100 + GRACE), [grace]);
        assert_eq!(queue.entry(missed).expect("missed entry").state, State::Missed);
        assert_eq!(queue.pending(), 1);
    }

    #[test]
    fn editing_a_missed_entry_reschedules_it_and_removing_cancels_it() {
        let mut queue = Queue::default();
        let id = queue.add(task(100)).expect("add");
        queue.due(100 + GRACE + 1);
        queue.edit(id, task(1000)).expect("reschedule");
        assert_eq!(queue.entry(id).expect("entry").state, State::Pending);
        queue.remove(id).expect("cancel");
        assert_eq!(queue.next(100), None);
        assert_eq!(queue.due(1000), [] as [u64; 0]);
    }

    #[test]
    fn clears_only_started_entries_and_rejects_editing_them() {
        let mut queue = Queue::default();
        let id = queue.add(task(100)).expect("add");
        queue.entry_mut(id).expect("entry").state = State::Started { pane: 7, server: "old".into() };
        let pending = queue.add(task(200)).expect("add pending");
        assert!(queue.edit(id, task(300)).is_err());
        queue.clear_started();
        assert_eq!(queue.entries[0].id, pending);
    }
}

mod persistence {
    use super::*;

    #[test]
    fn round_trips_states_targets_and_ids_without_reusing_removed_ids() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let mut queue = Queue::default();
        let id = queue.add(task(1000)).expect("add");
        queue.entry_mut(id).expect("entry").state = State::Failed { reason: "missing agent".into() };
        let removed = queue.add(task(2000)).expect("add removed");
        queue.remove(removed).expect("remove");
        persist::save(&path, &queue.saved()).expect("save");
        let mut restored = load(&path, 0);
        assert_eq!(restored.saved(), queue.saved());
        assert!(restored.add(task(3000)).expect("next id") > removed);
    }

    #[test]
    fn restores_missed_entries_without_starting_them() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let mut queue = Queue::default();
        let id = queue.add(task(100)).expect("add");
        persist::save(&path, &queue.saved()).expect("save");
        let mut restored = load(&path, 100 + GRACE + 1);
        assert_eq!(restored.entry(id).expect("entry").state, State::Missed);
        assert_eq!(restored.due(1000), [] as [u64; 0]);
    }

    #[test]
    fn missing_corrupt_and_other_version_files_are_empty() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        assert_eq!(load(&path, 0).entries, []);
        for text in ["{", r#"{"version":99,"next_id":1,"entries":[]}"#] {
            std::fs::write(&path, text).expect("write corrupt file");
            assert_eq!(load(&path, 0).entries, []);
        }
    }
}

#[test]
fn duplicate_or_zero_ids_make_a_saved_queue_corrupt() {
    let dir = TempDir::new();
    let path = dir.path().join("queue.json");
    let mut queue = Queue::default();
    queue.add(task(1000)).expect("add");
    let mut saved = queue.saved();
    saved.entries.push(saved.entries[0].clone());
    persist::save(&path, &saved).expect("save duplicate");
    assert_eq!(load(&path, 0).entries, []);
    saved.entries.pop();
    saved.entries[0].id = 0;
    persist::save(&path, &saved).expect("save zero");
    assert_eq!(load(&path, 0).entries, []);
}
