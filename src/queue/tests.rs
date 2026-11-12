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

mod accounts {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn an_entry_without_accounts_is_written_as_before() {
        let text = serde_json::to_string(&task(100)).expect("json");
        assert!(!text.contains("accounts"), "{text}");
    }

    #[test]
    fn accounts_and_their_absence_round_trip() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let chosen = BTreeMap::from([("claude".to_string(), "home".to_string()), ("codex".into(), String::new())]);
        let mut queue = Queue::default();
        queue.add(task(100)).expect("without");
        queue.add(Task { accounts: chosen.clone(), ..task(200) }).expect("with");
        persist::save(&path, &queue.saved()).expect("save");
        let read = load(&path, 0);
        let accounts: Vec<_> = read.entries.iter().map(|entry| entry.task.accounts.clone()).collect();
        assert_eq!(accounts, [BTreeMap::new(), chosen]);
    }
}

mod fresh {
    use super::*;

    #[test]
    fn an_entry_without_a_choice_is_written_as_before() {
        let text = serde_json::to_string(&task(100)).expect("json");
        assert!(!text.contains("fresh"), "{text}");
    }

    #[test]
    fn a_choice_round_trips_and_leaves_the_version_alone() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let mut queue = Queue::default();
        queue.add(Task { fresh: Some(true), ..task(100) }).expect("chosen");
        queue.add(Task { fresh: Some(false), ..task(200) }).expect("head");
        queue.add(task(300)).expect("left to the setting");
        assert_eq!(queue.saved().version, 1);
        persist::save(&path, &queue.saved()).expect("save");
        let read = load(&path, 0);
        let chosen: Vec<_> = read.entries.iter().map(|entry| entry.task.fresh).collect();
        assert_eq!(chosen, [Some(true), Some(false), None]);
    }
}

mod routines {
    use super::*;
    use crate::queue::repeat::Repeat;

    const START: i64 = 1_760_000_000;

    fn routine(at: i64, every: &str) -> Task {
        Task { repeat: Some(Repeat::parse(every, "UTC", START).expect("repeat")), label: "morning".into(), ..task(at) }
    }

    fn runs(queue: &Queue, id: u64) -> Vec<(i64, State)> {
        let mut runs: Vec<_> = queue.runs_of(id).map(|run| (run.task.at, run.state.clone())).collect();
        runs.sort_by_key(|(at, _)| *at);
        runs
    }

    #[test]
    fn a_due_routine_adds_one_pending_run_and_moves_on() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        assert_eq!(queue.fire(START + 5), [] as [u64; 0]);
        assert_eq!(runs(&queue, id), [(START, State::Pending)]);
        assert_eq!(queue.entry(id).expect("routine").task.at, START + 7200);
        assert_eq!(queue.entry(id).expect("routine").state, State::Pending);
        let run = queue.runs_of(id).next().cloned().expect("run");
        assert_eq!((run.task.routine, run.task.repeat), (Some(id), None));
        assert_eq!(queue.due(START + 5), [run.id]);
    }

    #[test]
    fn a_week_down_records_one_missed_run_and_lands_after_now() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        let now = START + 7 * 86_400 + 3000;
        let missed = queue.fire(now);
        assert_eq!(missed.len(), 1);
        assert_eq!(runs(&queue, id), [(START, State::Missed)]);
        assert!(queue.entry(id).expect("routine").task.at > now);
    }

    #[test]
    fn down_for_a_gap_but_within_grace_runs_the_last_occurrence() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        queue.fire(START + 3 * 7200 + 60);
        assert_eq!(runs(&queue, id), [(START, State::Missed), (START + 3 * 7200, State::Pending)]);
        assert_eq!(queue.entry(id).expect("routine").task.at, START + 4 * 7200);
    }

    #[test]
    fn a_paused_routine_never_fires_and_resumes_on_its_grid() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        queue.pause(id, true, START - 10).expect("pause");
        assert_eq!(queue.next(START), None);
        assert_eq!(queue.fire(START + 90_000), [] as [u64; 0]);
        assert_eq!(queue.runs_of(id).count(), 0);
        let now = START + 7200 + 100;
        queue.pause(id, false, now).expect("resume");
        assert_eq!(queue.entry(id).expect("routine").task.at, START + 14_400);
        assert!(Listed::from(queue.entry(id).expect("routine")).next.is_some());
    }

    #[test]
    fn start_now_leaves_the_schedule_alone() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START + 5000, "2h")).expect("add");
        let run = queue.run_now(id, START).expect("run");
        assert_eq!(queue.entry(run).expect("run").task.at, START);
        assert_eq!(queue.entry(id).expect("routine").task.at, START + 5000);
        assert!(queue.run_now(run, START).is_err());
    }

    #[test]
    fn only_the_last_ten_runs_stay_and_never_a_starting_one() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "30m")).expect("add");
        let first = queue.run_now(id, START).expect("first");
        queue.entry_mut(first).expect("first").state = State::Starting;
        for step in 1..=14 {
            let run = queue.run_now(id, START + step).expect("run");
            queue.entry_mut(run).expect("run").state = State::Removed;
        }
        assert_eq!(queue.runs_of(id).count(), RUNS_KEPT);
        assert!(queue.entry(first).is_some());
    }

    #[test]
    fn removing_a_routine_removes_its_runs_and_restore_brings_them_back() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        let one_off = queue.add(task(START)).expect("one-off");
        queue.run_now(id, START).expect("run");
        let gone = queue.remove(id).expect("remove");
        assert_eq!(gone.len(), 2);
        assert_eq!(queue.entries.iter().map(|entry| entry.id).collect::<Vec<_>>(), [one_off]);
        queue.restore(gone);
        assert_eq!(queue.entries.len(), 3);
    }

    #[test]
    fn a_routine_cannot_be_removed_while_one_of_its_runs_starts() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        let run = queue.run_now(id, START).expect("run");
        queue.entry_mut(run).expect("run").state = State::Starting;
        assert!(queue.remove(id).is_err());
    }

    #[test]
    fn clear_started_keeps_routines_and_starting_runs() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        let started = queue.run_now(id, START).expect("started");
        queue.entry_mut(started).expect("run").state = State::Started { pane: 1, server: "s".into() };
        let removed = queue.run_now(id, START + 1).expect("removed");
        queue.entry_mut(removed).expect("run").state = State::Removed;
        let starting = queue.run_now(id, START + 2).expect("starting");
        queue.entry_mut(starting).expect("run").state = State::Starting;
        let cleared = queue.clear_started();
        assert_eq!(cleared.len(), 2);
        assert!(queue.entry(id).is_some() && queue.entry(starting).is_some());
    }

    #[test]
    fn a_run_cannot_be_edited_nor_a_one_off_turned_into_a_routine() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("add");
        let run = queue.run_now(id, START).expect("run");
        assert!(queue.edit(run, task(START)).expect_err("run").contains("routine"));
        let one_off = queue.add(task(START)).expect("one-off");
        assert!(queue.edit(one_off, routine(START, "2h")).is_err());
        assert!(queue.edit(id, task(START)).is_err());
        assert!(queue.edit(id, routine(START + 1, "3h")).is_ok());
    }

    #[test]
    fn the_file_is_version_one_without_routines_and_two_with_them() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let mut queue = Queue::default();
        queue.add(task(100)).expect("one-off");
        assert_eq!(queue.saved().version, 1);
        let id = queue.add(routine(START, "2h")).expect("routine");
        queue.run_now(id, START).expect("run");
        assert_eq!(queue.saved().version, 2);
        persist::save(&path, &queue.saved()).expect("save");
        let read = load(&path, 0);
        assert_eq!(read.saved(), queue.saved());
        assert!(read.entry(id).expect("routine").is_routine());
    }

    #[test]
    fn a_routine_is_never_marked_missed_by_due_and_load_does_not_fire() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("routine");
        persist::save(&path, &queue.saved()).expect("save");
        let mut read = load(&path, START + 10 * 86_400);
        assert_eq!(read.entry(id).expect("routine").state, State::Pending);
        assert_eq!(read.runs_of(id).count(), 0);
        assert_eq!(read.due(START + 10 * 86_400), [] as [u64; 0]);
    }

    #[test]
    fn only_an_unpaused_routine_lists_its_next_time() {
        let mut queue = Queue::default();
        let id = queue.add(routine(START, "2h")).expect("routine");
        let one_off = queue.add(task(START)).expect("one-off");
        assert_eq!(Listed::from(queue.entry(id).expect("routine")).next, Some(START));
        assert_eq!(Listed::from(queue.entry(one_off).expect("entry")).next, None);
        queue.pause(id, true, START - 1).expect("pause");
        assert_eq!(Listed::from(queue.entry(id).expect("routine")).next, None);
        let text = serde_json::to_value(Listed::from(queue.entry(id).expect("routine"))).expect("json");
        assert_eq!(text["repeat"], "2h");
        assert_eq!(text["paused"], true);
    }
}

mod trimming {
    use super::*;
    use crate::queue::repeat::Repeat;

    #[test]
    fn a_started_run_whose_cleanup_still_counts_is_not_dropped_but_one_of_an_old_server_is() {
        let mut queue = Queue::default();
        let repeat = Repeat::parse("30m", "UTC", 0).expect("repeat");
        let id = queue.add(Task { repeat: Some(repeat), cleanup: true, ..task(100) }).expect("routine");
        let mine = State::Started { pane: 1, server: crate::control::server_token().into() };
        let first = queue.run_now(id, 1).expect("first");
        queue.entry_mut(first).expect("run").state = mine;
        let old = queue.run_now(id, 2).expect("old");
        queue.entry_mut(old).expect("run").state = State::Started { pane: 2, server: "gone".into() };
        for at in 3..=14 {
            let run = queue.run_now(id, at).expect("run");
            queue.entry_mut(run).expect("run").state = State::Removed;
        }
        assert!(queue.entry(first).is_some());
        assert!(queue.entry(old).is_none());
    }
}

mod capped_trimming {
    use super::*;
    use crate::queue::repeat::Repeat;

    #[test]
    fn runs_that_keep_their_place_stop_at_twice_the_history() {
        let mut queue = Queue::default();
        let repeat = Repeat::parse("30m", "UTC", 0).expect("repeat");
        let id = queue.add(Task { repeat: Some(repeat), cleanup: true, ..task(100) }).expect("routine");
        for at in 1..=40 {
            let run = queue.run_now(id, at).expect("run");
            queue.entry_mut(run).expect("run").state =
                State::Started { pane: 1, server: crate::control::server_token().into() };
        }
        assert_eq!(queue.runs_of(id).count(), RUNS_KEPT * 2);
    }
}

mod results {
    use super::*;
    use crate::queue::outcome::{Outcome, Pull};

    fn finished(queue: &mut Queue) -> u64 {
        let id = queue.add(task(100)).expect("add");
        let entry = queue.entry_mut(id).expect("entry");
        entry.state = State::Started { pane: 3, server: "s".into() };
        entry.started = Some(150);
        entry.result = Some(Outcome {
            at: 200,
            text: "Reviewed it.".into(),
            agent: Some("claude".into()),
            pull_request: Some(Pull { number: 4, title: "Fix".into(), ..Pull::default() }),
            ..Outcome::default()
        });
        id
    }

    #[test]
    fn a_result_and_its_start_round_trip_and_leave_the_version_alone() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let mut queue = Queue::default();
        finished(&mut queue);
        assert_eq!(queue.saved().version, 1);
        persist::save(&path, &queue.saved()).expect("save");
        assert_eq!(load(&path, 0).saved(), queue.saved());
        assert!(load(&path, 0).entries[0].result.is_some());
    }

    #[test]
    fn an_entry_without_a_result_is_written_as_before() {
        let mut queue = Queue::default();
        queue.add(task(100)).expect("add");
        let text = serde_json::to_string(&queue.saved()).expect("serialize");
        assert!(!text.contains("result") && !text.contains("started"), "{text}");
    }

    #[test]
    fn a_file_from_before_results_reads_as_before() {
        let dir = TempDir::new();
        let path = dir.path().join("queue.json");
        let before = r#"{"version":1,"next_id":1,"entries":[{"id":1,"at":100,"zone":"UTC","label":"","agent":"claude","command":"claude","program":"claude","mode":null,"prompt":"fix","submit":true,"place":{"project":"/p","workspace":null,"workspace_name":null,"workspace_id":null,"server":"","branch":null},"state":"started","pane":2,"server":"old"}]}"#;
        std::fs::write(&path, before).expect("write");
        let queue = load(&path, 0);
        let entry = &queue.entries[0];
        assert_eq!((entry.started, entry.result.clone()), (None, None));
        let written = serde_json::to_value(queue.saved()).expect("serialize");
        assert_eq!(written, serde_json::from_str::<serde_json::Value>(before).expect("json"));
    }

    #[test]
    fn listing_leaves_the_text_out_and_says_when_it_changed() {
        let mut queue = Queue::default();
        let id = finished(&mut queue);
        let value = serde_json::to_value(Listed::from(queue.entry(id).expect("entry"))).expect("serialize");
        assert_eq!(value["result_at"], 200);
        assert_eq!(value["started"], 150);
        assert!(value.get("result").is_none(), "{value}");
    }

    #[test]
    fn restoring_a_removal_brings_the_result_back() {
        let mut queue = Queue::default();
        let id = finished(&mut queue);
        let removed = queue.remove(id).expect("remove");
        queue.restore(removed);
        assert_eq!(
            queue.entry(id).and_then(|entry| entry.result.as_ref()).map(|r| r.text.as_str()),
            Some("Reviewed it.")
        );
    }
}
