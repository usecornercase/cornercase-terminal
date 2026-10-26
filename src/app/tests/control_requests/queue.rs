use super::*;
use crate::queue::{Entry, Queue, State, Task};

fn seed(app: &mut App, states: &[State]) -> Vec<Entry> {
    let mut queue = Queue::default();
    let at = crate::time::now() + 3600;
    for state in states {
        let task = Task {
            at,
            zone: "Europe/Madrid".into(),
            label: "login follow-up".into(),
            prompt: "review the login fix\nand its tests".into(),
            agent: "claude".into(),
            command: "claude --model sonnet".into(),
            program: "claude".into(),
            mode: Some("careful".into()),
            submit: true,
            accounts: [("claude".into(), "home".into())].into(),
            place: crate::queue::Place { project: PathBuf::from("/project"), ..Default::default() },
        };
        let id = queue.add(task).expect("scheduled");
        queue.entry_mut(id).expect("entry").state = state.clone();
    }
    let entries = queue.entries.clone();
    app.set_queue(queue);
    entries
}

fn queue(app: &mut App, action: &str, entry: u64) -> Value {
    ok(app, "queue", &json!({"action": action, "entry": entry}))
}

fn restore(app: &mut App, removed: &Value) -> Value {
    queue(app, "restore", removed["undo"].as_u64().expect("undo number"))
}

#[test]
fn a_removal_restores_the_captured_task_and_its_place_among_equal_times() {
    let (mut app, _rx) = app();
    let before = seed(&mut app, &[State::Pending, State::Pending, State::Pending]);
    let id = before[1].id;
    let removed = queue(&mut app, "remove", id);
    assert_eq!(removed["entries"], json!([before[1]]));
    assert!(app.panels.queued.queue().entry(id).is_none());

    let restored = restore(&mut app, &removed);

    assert_eq!(restored, json!({"entries": [before[1]]}));
    assert_eq!(app.panels.queued.queue().entries, before);
    let added = app.panels.queued.schedule(None, before[1].task.clone()).expect("another entry");
    assert!(added > before[2].id);
}

#[test]
fn clear_started_removes_only_started_entries_and_restores_the_batch() {
    let (mut app, _rx) = app();
    let started = State::Started { pane: 9, server: "old-server".into() };
    let states = [
        State::Pending,
        started.clone(),
        State::Starting,
        State::Missed,
        State::Failed { reason: "missing agent".into() },
        started,
    ];
    let before = seed(&mut app, &states);

    let cleared = ok(&mut app, "queue", &json!({"action": "clear-started"}));

    assert_eq!(cleared["entries"], json!([before[1], before[5]]));
    assert_eq!(
        app.panels.queued.queue().entries,
        [before[0].clone(), before[2].clone(), before[3].clone(), before[4].clone()]
    );
    let restored = restore(&mut app, &cleared);
    assert_eq!(restored["entries"], cleared["entries"]);
    assert_eq!(app.panels.queued.queue().entries, before);
}

#[test]
fn clearing_nothing_returns_no_undo_and_does_not_evict_a_removal() {
    let (mut app, _rx) = app();
    let before = seed(&mut app, &[State::Pending]);
    let removed = queue(&mut app, "remove", before[0].id);
    for _ in 0..17 {
        let cleared = ok(&mut app, "queue", &json!({"action": "clear-started"}));
        assert_eq!(cleared, json!({"entries": []}));
    }
    restore(&mut app, &removed);
    assert_eq!(app.panels.queued.queue().entries, before);
}

#[test]
fn unknown_and_consumed_undo_numbers_are_not_found() {
    let (mut app, _rx) = app();
    let before = seed(&mut app, &[State::Pending]);
    let removed = queue(&mut app, "remove", before[0].id);
    restore(&mut app, &removed);
    for undo in [removed["undo"].clone(), json!(999)] {
        let error = refused(&mut app, "queue", &json!({"action": "restore", "entry": undo}));
        assert_eq!(error.code, "not-found");
    }
    assert_eq!(app.panels.queued.queue().entries, before);
}

#[test]
fn only_the_last_sixteen_request_removals_can_be_restored() {
    let (mut app, _rx) = app();
    let entries = seed(&mut app, &[const { State::Pending }; 17]);
    let removed: Vec<_> = entries.iter().map(|entry| queue(&mut app, "remove", entry.id)).collect();
    let expired = refused(&mut app, "queue", &json!({"action": "restore", "entry": removed[0]["undo"]}));
    assert_eq!(expired.code, "not-found");
    for removal in removed.iter().skip(1).rev() {
        restore(&mut app, removal);
    }
    assert_eq!(app.panels.queued.queue().entries, entries[1..]);
}

#[test]
fn panel_undo_and_request_undo_keep_their_own_removals() {
    let (mut app, _rx) = app();
    let before = seed(&mut app, &[State::Pending, State::Pending]);
    app.remove_queued(before[0].id);
    let toast = app.messages.toast.clone().expect("panel undo");
    let removed = queue(&mut app, "remove", before[1].id);
    assert_eq!(app.messages.toast, Some(toast));
    restore(&mut app, &removed);
    let button = ui::toast_button(AREA, app.messages.toast.as_ref().expect("panel undo kept").view());
    click(&mut app, button.as_position());
    assert_eq!(app.panels.queued.queue().entries, before);
}

#[test]
fn removing_an_entry_while_it_starts_leaves_it_and_creates_no_undo() {
    let (mut app, _rx) = app();
    let before = seed(&mut app, &[State::Pending]);
    let id = before[0].id;
    app.panels.queued.start(id);
    let error = refused(&mut app, "queue", &json!({"action": "remove", "entry": id}));
    assert!(error.message.contains("already starting"));
    assert_eq!(app.panels.queued.queue().entry(id).expect("entry kept").state, State::Starting);
    assert!(app.panels.queued.take_removed(1).is_none());
}
