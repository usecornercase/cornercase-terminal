use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::agents::context::message::tests::{Written, joined_lines};
use crate::test_util::{TempDir, fixture};

fn read(text: &str) -> Option<Awaited> {
    let file = Written::new(text);
    awaited(&file.path).expect("read the transcript")
}

fn read_lines(lines: &[Value]) -> Option<Awaited> {
    read(&joined_lines(&lines.iter().map(Value::to_string).collect::<Vec<_>>()))
}

fn called(message: &str, id: &str, name: &str, input: &Value) -> Value {
    json!({
        "type": "assistant",
        "isSidechain": false,
        "message": {
            "id": message,
            "role": "assistant",
            "content": [{ "type": "tool_use", "id": id, "name": name, "input": input }],
        },
    })
}

fn answered(id: &str) -> Value {
    json!({
        "type": "user",
        "isSidechain": false,
        "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": "ok" }] },
    })
}

fn prompt(text: &str) -> Value {
    json!({ "type": "user", "isSidechain": false, "message": { "role": "user", "content": text } })
}

fn choice(number: usize, label: &str, description: Option<&str>) -> Choice {
    Choice { number, label: label.into(), description: description.map(Into::into), selected: None }
}

fn database() -> Question {
    Question {
        header: Some("Database".into()),
        text: "Which database should the service use?".into(),
        multi_select: false,
        free_text: true,
        options: vec![
            choice(1, "Postgres", Some("What the other services use")),
            choice(2, "SQLite", Some("One file, no server")),
        ],
    }
}

fn bash(command: &str) -> Value {
    json!({ "command": command, "timeout": 600_000 })
}

mod a_question {
    use super::*;

    #[test]
    fn waiting_for_its_answer_is_read_with_its_options_numbered_from_one() {
        let found = read(fixture!("claude/2.1.332/ask-one.jsonl"));

        assert_eq!(found, Some(Awaited::questions(vec![database()])));
    }

    #[test]
    fn several_come_in_their_order_and_an_empty_description_is_none() {
        let found = read(fixture!("claude/2.1.332/ask-several.jsonl")).expect("questions");

        assert_eq!(found.questions.len(), 2);
        assert_eq!(found.questions[0], database());
        assert_eq!(found.questions[1].header.as_deref(), Some("Deploy"));
        assert_eq!(
            found.questions[1].options,
            [
                choice(1, "Staging", Some("Behind the VPN")),
                choice(2, "Production", Some("Every customer sees it")),
                choice(3, "Nowhere yet", None),
            ]
        );
    }

    #[test]
    fn that_takes_several_options_says_so() {
        let found = read(fixture!("claude/2.1.332/ask-multi-select.jsonl")).expect("questions");

        assert!(found.questions[0].multi_select);
        assert_eq!(found.questions[0].options.len(), 3);
    }

    #[test]
    fn says_nothing_of_what_only_the_screen_knows() {
        let found = read(fixture!("claude/2.1.332/ask-one.jsonl")).expect("questions");

        assert_eq!(found.open, None);
        assert!(found.questions[0].free_text);
        assert!(found.questions[0].options.iter().all(|option| option.selected.is_none()));
    }

    #[test]
    fn with_fields_missing_takes_their_defaults() {
        let found = read_lines(&[called(
            "m1",
            "t1",
            ASK,
            &json!({ "questions": [{ "question": "Go on?", "options": [{ "label": "Yes" }, {}] }] }),
        )]);

        assert_eq!(
            found,
            Some(Awaited::questions(vec![Question {
                header: None,
                text: "Go on?".into(),
                multi_select: false,
                free_text: true,
                options: vec![choice(1, "Yes", None), choice(2, "", None)],
            }]))
        );
    }

    #[rstest]
    #[case::answered(fixture!("claude/2.1.332/ask-answered.jsonl"))]
    #[case::rejected(fixture!("claude/2.1.332/ask-rejected.jsonl"))]
    fn with_its_result_is_no_longer_pending(#[case] transcript: &str) {
        assert_eq!(read(transcript), None);
    }

    #[test]
    fn wins_over_the_other_calls_of_its_answer() {
        let found = read_lines(&[
            called("m1", "t1", "Bash", &bash("ls")),
            called("m1", "t2", ASK, &json!({ "questions": [] })),
        ]);

        assert_eq!(found.map(|found| found.source), Some(crate::agents::context::awaited::Source::Question));
    }
}

mod a_permission {
    use super::*;

    #[test]
    fn names_the_tool_and_its_command() {
        let found = read(fixture!("claude/2.1.332/permission-bash.jsonl"));

        assert_eq!(found, Some(Awaited::permission("Bash".into(), Some("cargo test --locked".into()))));
    }

    #[test]
    fn of_a_tool_without_a_command_has_none() {
        let found = read_lines(&[called("m1", "t1", "Write", &json!({ "file_path": "/tmp/x", "content": "y" }))]);

        assert_eq!(found, Some(Awaited::permission("Write".into(), None)));
    }

    #[test]
    fn is_the_call_left_once_the_others_of_its_answer_have_results() {
        let found = read(fixture!("claude/2.1.332/parallel.jsonl"));

        assert_eq!(found, Some(Awaited::permission("Bash".into(), Some("cargo build".into()))));
    }

    #[test]
    fn is_the_first_call_the_agent_made() {
        let found =
            read_lines(&[called("m1", "t1", "Bash", &bash("first")), called("m1", "t2", "Bash", &bash("second"))]);

        assert_eq!(found.and_then(|found| found.command).as_deref(), Some("first"));
    }

    #[rstest]
    #[case::agent("Agent")]
    #[case::task("Task")]
    fn is_not_a_subagent_that_runs(#[case] name: &str) {
        assert_eq!(read_lines(&[called("m1", "t1", name, &json!({ "prompt": "look" }))]), None);
    }
}

mod nothing_is_pending {
    use super::*;

    #[test]
    fn once_a_prompt_follows_a_call_without_a_result() {
        assert_eq!(read_lines(&[called("m1", "t1", "Bash", &bash("ls")), prompt("never mind")]), None);
    }

    #[test]
    fn for_a_call_of_an_earlier_answer() {
        let found = read_lines(&[
            called("m1", "t1", "Bash", &bash("ls")),
            called("m2", "t2", "Read", &json!({})),
            answered("t2"),
        ]);

        assert_eq!(found, None);
    }

    #[test]
    fn for_a_call_of_a_subagent() {
        let mut line = called("m1", "t1", "Bash", &bash("ls"));
        line["isSidechain"] = true.into();

        assert_eq!(read_lines(&[line]), None);
    }

    #[test]
    fn when_the_hook_line_names_the_call() {
        let hook = json!({ "type": "attachment", "attachment": { "type": "hook_success", "toolUseID": "t1" } });

        let found = read_lines(&[called("m1", "t1", "Bash", &bash("ls")), hook]);

        assert!(found.is_some(), "a hook is not a result");
    }

    #[test]
    fn without_a_transcript() {
        let dir = TempDir::new();

        assert_eq!(awaited(&dir.path().join("missing.jsonl")).expect("no error"), None);
    }

    #[test]
    fn in_an_empty_transcript() {
        assert_eq!(read(""), None);
    }
}
