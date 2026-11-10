use super::*;

#[test]
fn uses_the_five_hour_reset_and_rejects_missing_or_past_resets() {
    let mut report = usage::Report::default();
    assert!(reset_time(&report, 100).is_err());
    report.windows.push(usage::Window {
        label: "week".into(),
        kind: "weekly_all".into(),
        scope: None,
        percent: 100,
        severity: usage::Severity::Critical,
        resets_at: Some(200),
    });
    assert!(reset_time(&report, 100).is_err());
    report.windows.push(usage::Window {
        label: "session (5h)".into(),
        kind: "session".into(),
        scope: None,
        percent: 80,
        severity: usage::Severity::Warning,
        resets_at: Some(150),
    });
    assert_eq!(reset_time(&report, 100).expect("reset"), 150);
    assert!(reset_time(&report, 150).is_err());
}

#[test]
fn queue_removal_requests_keep_their_wire_shape_and_have_no_unknown_fields() {
    let cases = [
        (control::Queue::Remove(5), json!({"action": "remove", "entry": 5})),
        (control::Queue::ClearStarted, json!({"action": "clear-started"})),
        (control::Queue::Restore(8), json!({"action": "restore", "entry": 8})),
        (control::Queue::Pause(3), json!({"action": "pause", "entry": 3})),
        (control::Queue::Resume(3), json!({"action": "resume", "entry": 3})),
    ];
    for (command, args) in cases {
        let raw = json!({"contract": 1, "command": "queue", "args": args});
        let request: control::Request = serde_json::from_value(raw.clone()).expect("queue request");
        assert_eq!(request.command, control::Command::Queue(command));
        assert_eq!(control::contract::unknown_fields(&raw, &request), None);
        assert_eq!(serde_json::to_value(request).expect("written request")["args"], args);
    }
}

#[test]
fn the_default_mode_is_none_and_no_mode_given_is_the_one_chosen_in_settings() {
    use crate::config::Config;
    use crate::queue::{Entry, Task};

    let config = Config {
        agent_args: [("claude".to_string(), vec!["--permission-mode".to_string(), "plan".to_string()])].into(),
        ..Config::default()
    };
    let entry = |mode: Option<&str>| Entry {
        id: 1,
        task: Task { agent: "claude".into(), mode: mode.map(str::to_string), ..Task::default() },
        state: State::Pending,
    };
    let add = |mode: Option<&str>| QueueAdd { mode: mode.map(str::to_string), ..QueueAdd::default() };

    assert_eq!(prepare::queue_mode(&config, "claude", &add(Some("default")), None), None);
    assert_eq!(prepare::queue_mode(&config, "claude", &add(None), None).as_deref(), Some("plan"));
    assert_eq!(prepare::queue_mode(&config, "claude", &add(None), Some(&entry(None))), None);
    assert_eq!(prepare::queue_mode(&config, "claude", &add(Some("default")), Some(&entry(Some("auto")))), None);
}

#[test]
fn a_started_entry_carries_the_choice_it_kept_for_its_branch() {
    let entry =
        |fresh| queue::Entry { id: 1, task: queue::Task { fresh, ..queue::Task::default() }, state: State::Pending };

    let starts = |fresh| queued_start(&entry(fresh), (1, None), Some("feat/x".into())).fresh;

    assert_eq!([starts(Some(true)), starts(Some(false)), starts(None)], [Some(true), Some(false), None]);
}
