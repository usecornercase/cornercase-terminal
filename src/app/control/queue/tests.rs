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
    let entry = |mode: Option<&str>| {
        Entry::new(
            1,
            Task { agent: "claude".into(), mode: mode.map(str::to_string), ..Task::default() },
            State::Pending,
        )
    };
    let add = |mode: Option<&str>| QueueAdd { mode: mode.map(str::to_string), ..QueueAdd::default() };

    assert_eq!(prepare::queue_mode(&config, "claude", &add(Some("default")), None), None);
    assert_eq!(prepare::queue_mode(&config, "claude", &add(None), None).as_deref(), Some("plan"));
    assert_eq!(prepare::queue_mode(&config, "claude", &add(None), Some(&entry(None))), None);
    assert_eq!(prepare::queue_mode(&config, "claude", &add(Some("default")), Some(&entry(Some("auto")))), None);
}

#[test]
fn a_started_entry_carries_the_choice_it_kept_for_its_branch() {
    let entry = |fresh| queue::Entry::new(1, queue::Task { fresh, ..queue::Task::default() }, State::Pending);

    let starts = |fresh| queued_start(&entry(fresh), (1, None), Some("feat/x".into())).fresh;

    assert_eq!([starts(Some(true)), starts(Some(false)), starts(None)], [Some(true), Some(false), None]);
}

const LONG: &str = "Review the open pull requests and fix the failing checks on main, then write a short summary of what changed and what is left for tomorrow.";

#[rstest::rstest]
#[case::written(Some("nightly review"), LONG, "nightly review")]
#[case::from_the_prompt(None, LONG, "Review the open pull requests and fix…")]
#[case::first_line(None, "Fix the login form\nand add a test", "Fix the login form")]
fn a_name_is_written_or_taken_from_the_prompts_first_words(
    #[case] label: Option<&str>,
    #[case] prompt: &str,
    #[case] name: &str,
) {
    let add = QueueAdd { label: label.map(str::to_string), ..QueueAdd::default() };

    assert_eq!(prepare::queue_label(&add, prompt, None), name);
}

#[test]
fn an_unchanged_prompt_keeps_the_entrys_name() {
    let old = queue::Entry::new(
        1,
        queue::Task { label: "nightly review".into(), prompt: LONG.into(), ..queue::Task::default() },
        State::Pending,
    );

    assert_eq!(prepare::queue_label(&QueueAdd::default(), LONG, Some(&old)), "nightly review");
}
