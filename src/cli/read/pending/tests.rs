use serde_json::json;

use super::*;
use crate::test_util::fixture;

fn answer(fixture: &str) -> Value {
    serde_json::from_str(fixture).expect("an answer")
}

mod one_target {
    use super::*;

    #[test]
    fn a_question_lists_its_numbered_options() {
        let text = text(&answer(fixture!("control/pending-question/v1/question.json")));

        assert_eq!(
            text,
            "Database: Which database should the service use?\n  \
             1. Postgres - What the other services use\n  \
             2. SQLite - One file, no server\n"
        );
    }

    #[test]
    fn questions_are_apart_and_an_option_without_a_description_is_its_label() {
        let text = text(&answer(fixture!("control/pending-question/v1/questions.json")));

        assert_eq!(
            text,
            "Database: Which database should the service use?\n  \
             1. Postgres - What the other services use\n  \
             2. SQLite - One file, no server\n\
             \n\
             Deploy: Where should it run first?\n  \
             1. Staging - Behind the VPN\n  \
             2. Production - Every customer sees it\n  \
             3. Nowhere yet\n"
        );
    }

    #[test]
    fn a_question_that_takes_several_options_says_so() {
        let text = text(&answer(fixture!("control/pending-question/v1/multi-select.json")));

        assert!(text.starts_with("Checks: Which checks should run on every push? [multi-select]\n"), "{text}");
    }

    #[test]
    fn a_question_without_a_header_is_only_the_question() {
        let value =
            json!({ "source": "question", "questions": [{ "header": null, "question": "Go on?", "options": [] }] });

        assert_eq!(text(&value), "Go on?\n");
    }

    #[test]
    fn a_permission_names_the_tool_and_its_command() {
        let text = text(&answer(fixture!("control/pending-question/v1/permission.json")));

        assert_eq!(text, "permission: Bash\ncargo test --locked\n");
    }

    #[test]
    fn a_permission_without_a_command_is_only_the_tool() {
        let value = json!({ "source": "permission", "tool": "Write", "command": null, "questions": [] });

        assert_eq!(text(&value), "permission: Write\n");
    }

    #[test]
    fn nothing_pending_says_so() {
        assert_eq!(text(&Value::Null), "nothing pending\n");
    }

    #[test]
    fn a_source_of_a_newer_server_is_named() {
        let value = json!({ "source": "plan", "questions": [] });

        assert_eq!(text(&value), "plan\n");
    }
}

mod several_targets {
    use super::*;

    #[test]
    fn each_has_its_block_with_its_header() {
        let panes = [
            answer(fixture!("control/pending-question/v1/permission.json")),
            json!({ "pane": 6, "source": null }),
            json!({ "tab": 8, "source": null }),
            json!({ "pane": 9, "error": "pane 9 runs no agent" }),
        ];

        assert_eq!(
            blocks(&panes),
            "pane 7  claude  permission\npermission: Bash\ncargo test --locked\n\n\
             pane 6  nothing pending\n\n\
             tab 8  nothing pending\n\n\
             pane 9\nerror: pane 9 runs no agent\n"
        );
    }
}
