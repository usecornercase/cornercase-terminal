use super::*;
use crate::agents::context::{Pointer, Said};
use crate::queue::outcome::Taken;
use crate::queue::{Place, State, Task};

fn apart_with_entry() -> (Apart, u64) {
    let mut world = apart();
    let place = Place { project: "/work/shop".into(), ..Place::default() };
    let task = Task {
        at: 1_794_474_000,
        zone: "UTC".into(),
        agent: "claude".into(),
        label: "review".into(),
        place,
        ..Task::default()
    };
    let id = world.app.panels.queued.schedule(None, task).expect("scheduled");
    world.app.panels.queued.started_at(id, 999, 1_794_474_060);
    (world, id)
}

fn keep(world: &mut Apart, id: u64, record: Pointer) {
    let said = Said { text: "Reviewed it.".into(), written: None };
    let taken = Taken { agent: Some("claude".into()), record: Some(record), ..Taken::default() };
    world.app.panels.queued.keep_result(id, taken, Some(said), 1_794_476_460);
}

fn transcript(world: &Apart) -> Pointer {
    let path = world.folders[0].path().join("record.jsonl");
    let reply = json!({"type": "assistant", "message": {"id": "m1", "model": "claude-opus-5-5",
                       "content": [{"type": "text", "text": "Reviewed it."}]}});
    std::fs::write(&path, format!("{reply}\n")).expect("record");
    Pointer::Claude(path)
}

#[test]
fn queue_show_answers_the_entry_and_its_result() {
    let (mut world, id) = apart_with_entry();
    let record = transcript(&world);
    keep(&mut world, id, record);

    let shown = ok(&mut world.app, "queue-show", &json!({"id": id, "conversation": false}));

    assert_eq!((shown["state"].as_str(), shown["started"].as_i64()), (Some("started"), Some(1_794_474_060)));
    assert_eq!(
        (shown["result"]["text"].as_str(), shown["result"]["conversation"].as_bool()),
        (Some("Reviewed it."), Some(true))
    );
    assert!(shown["messages"].is_null() && shown["note"].is_null() && shown.get("record").is_none(), "{shown}");
    assert!(shown["result"].get("record").is_none(), "{shown}");
}

#[test]
fn a_record_that_is_gone_answers_the_result_alone_and_says_so() {
    let (mut world, id) = apart_with_entry();
    let missing = Pointer::Claude(world.folders[0].path().join("missing.jsonl"));
    keep(&mut world, id, missing);

    let shown = ok(&mut world.app, "queue-show", &json!({"id": id, "conversation": true}));

    assert!(shown["messages"].is_null());
    assert_eq!(shown["result"]["text"], "Reviewed it.");
    assert!(shown["note"].as_str().is_some_and(|note| note.contains("no longer has")), "{shown}");
}

#[test]
fn an_unknown_entry_is_not_found() {
    let mut world = apart();

    assert_eq!(refused(&mut world.app, "queue-show", &json!({"id": 99, "conversation": false})).code, "not-found");
}

#[test]
fn removal_answers_carry_the_result_but_never_the_agents_file() {
    let (mut world, id) = apart_with_entry();
    let record = transcript(&world);
    keep(&mut world, id, record);
    let path = world.folders[0].path().join("record.jsonl").display().to_string();

    let removed = ok(&mut world.app, "queue", &json!({"action": "remove", "entry": id}));
    let restored = ok(&mut world.app, "queue", &json!({"action": "restore", "entry": removed["undo"]}));

    for answer in [&removed, &restored] {
        assert_eq!(answer["entries"][0]["result"]["text"], "Reviewed it.");
        assert!(!answer.to_string().contains(&path), "{answer}");
    }
    let entry = world.app.panels.queued.queue().entry(id).expect("restored");
    assert!(entry.result.as_ref().is_some_and(|result| result.record.is_some()));
    assert!(matches!(entry.state, State::Started { .. }));
}
