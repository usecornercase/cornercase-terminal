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
    ];
    for (command, args) in cases {
        let raw = json!({"contract": 1, "command": "queue", "args": args});
        let request: control::Request = serde_json::from_value(raw.clone()).expect("queue request");
        assert_eq!(request.command, control::Command::Queue(command));
        assert_eq!(control::contract::unknown_fields(&raw, &request), None);
        assert_eq!(serde_json::to_value(request).expect("written request")["args"], args);
    }
}
