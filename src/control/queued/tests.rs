use serde_json::json;

use super::*;
use crate::queue::{Entry, State, Task};

#[test]
fn a_queue_show_request_writes_both_fields() {
    let command = crate::control::Command::QueueShow(QueueShow { id: 7, conversation: false });

    let text = serde_json::to_value(&command).expect("serialize");

    assert_eq!(text, json!({"command": "queue-show", "args": {"id": 7, "conversation": false}}));
    assert_eq!(serde_json::from_value::<crate::control::Command>(text).expect("read"), command);
}

fn shown() -> QueueShown {
    let entry = Entry::new(7, Task::default(), State::Removed);
    QueueShown { entry: crate::queue::Listed::from(&entry), result: None, messages: None, earlier: false, note: None }
}

#[test]
fn every_key_of_the_answer_is_there_when_there_is_nothing() {
    let value = serde_json::to_value(shown()).expect("serialize");

    for key in ["result", "messages", "note"] {
        assert!(value[key].is_null(), "{key}: {value}");
    }
    assert_eq!(value["earlier"], false);
    assert_eq!(value["id"], 7);
}

#[test]
fn a_shown_entry_reads_back() {
    let mut full = shown();
    full.result = Some(ResultInfo { at: 5, text: "done".into(), conversation: true, ..ResultInfo::default() });
    full.messages = Some(vec![MessageInfo { role: "reply".into(), text: "done".into(), ..MessageInfo::default() }]);
    full.note = Some("why".into());

    let back: QueueShown = serde_json::from_value(serde_json::to_value(&full).expect("serialize")).expect("read");

    assert_eq!(back, full);
}
