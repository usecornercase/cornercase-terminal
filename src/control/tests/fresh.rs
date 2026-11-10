use super::*;

fn written<T: Serialize>(request: &T) -> Value {
    serde_json::to_value(request).expect("json")
}

#[test]
fn a_request_that_asks_for_nothing_writes_no_fresh_key() {
    assert!(written(&FlowRun::default()).get("fresh").is_none());
    assert!(written(&Start::default()).get("fresh").is_none());
    assert!(written(&NewWorkspace::default()).get("fresh").is_none());
}

#[test]
fn a_request_that_asks_writes_it_and_logs_it() {
    let run = FlowRun { fresh: Some(true), ..FlowRun::default() };
    let start = Start { fresh: Some(false), ..Start::default() };
    let new = NewWorkspace { fresh: Some(true), ..NewWorkspace::default() };

    assert_eq!(written(&run)["fresh"], json!(true));
    assert_eq!(written(&start)["fresh"], json!(false));
    assert_eq!(written(&new)["fresh"], json!(true));
    for command in [Command::FlowRun(run), Command::Start(start), Command::NewWorkspace(new)] {
        assert!(command.fields().iter().any(|(name, _)| *name == "fresh"), "{:?}", command.fields());
    }
}

#[test]
fn a_queue_entry_always_writes_it() {
    assert_eq!(written(&QueueAdd::default())["fresh"], Value::Null);
    assert_eq!(written(&QueueAdd { fresh: Some(true), ..QueueAdd::default() })["fresh"], json!(true));
}
