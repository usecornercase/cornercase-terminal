use serde_json::json;

use super::*;
use crate::agents::context::message::{Role, Spoken};
use crate::agents::vibe::fake::FakeVibe;

fn parse(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

mod arguments {
    use super::*;

    #[test]
    fn one_auto_approval_spelling_normalizes_to_the_agent() {
        assert_eq!(normalized(&parse(&["--yolo"])), ["--agent", "auto-approve"]);
        assert_eq!(normalized(&parse(&["--auto-approve"])), ["--agent", "auto-approve"]);
        assert_eq!(normalized(&parse(&["--agent=auto-approve"])), ["--agent", "auto-approve"]);
        assert_eq!(normalized(&parse(&["--smart-approve"])), ["--agent", "smart-approve"]);
        assert_eq!(normalized(&parse(&["--add-dir", "/work"])), ["--add-dir", "/work"]);
    }

    #[test]
    fn resumes_by_session_id() {
        assert_eq!(
            resume(parse(&["--agent", "ask"]), "8c7d".into()),
            Some(parse(&["--agent", "ask", "--resume", "8c7d"]))
        );
    }

    #[test]
    fn recognizes_the_rust_binary_behind_the_launcher() {
        assert!(runs_as("/home/a/.local/share/uv/tools/mistral-vibe/lib/python3.14/site-packages/vibe/_bin/vibe-rs"));
        assert!(!runs_as("vibe"));
        assert!(!runs_as("vibe-app-server"));
    }

    #[test]
    fn shell_teleport_and_slash_commands_stay_local_but_skills_are_prompts() {
        assert!(local_command("!git status"));
        assert!(local_command("&move this to the cloud"));
        assert!(local_command("/compact"));
        assert!(!local_command(" /skill review"));
        assert!(!local_command("fix the login"));
    }
}

mod messages {
    use super::*;

    fn journal() -> FakeVibe {
        let fake = FakeVibe::new();
        fake.journal(&[
            json!({"type": "command_reserved", "payload": {
                "method": "turn/start",
                "params": {"injected": false, "message": [{"type": "text", "text": "fix the login"}]},
            }}),
            json!({"type": "action_result", "payload": {"result": {"result": {
                "parts": [{"type": "reasoning", "text": "thinking"}, {"type": "text", "text": "first"}],
            }}}}),
            json!({"type": "action_result", "payload": {"result": {"result": {
                "parts": [{"type": "tool_call"}, {"type": "text", "text": "done"}],
            }}}}),
        ]);
        fake
    }

    #[test]
    fn the_last_message_is_the_newest_assistant_text() {
        let fake = journal();
        let said = message::last_message(&fake.session()).expect("read").expect("a reply");

        assert_eq!(said.text, "done");
    }

    #[test]
    fn search_reads_prompts_and_replies_in_order() {
        let fake = journal();
        let mut seen: Vec<Spoken> = Vec::new();
        message::said_all(&fake.session(), &mut |spoken| {
            seen.push(spoken);
            true
        })
        .expect("read");

        let roles: Vec<Role> = seen.iter().map(|spoken| spoken.role).collect();
        let texts: Vec<&str> = seen.iter().map(|spoken| spoken.said.text.as_str()).collect();
        assert_eq!(roles, [Role::Prompt, Role::Reply, Role::Reply]);
        assert_eq!(texts, ["fix the login", "first", "done"]);
    }

    #[test]
    fn a_session_without_a_reply_has_no_last_message() {
        let fake = FakeVibe::new();
        fake.journal(&[json!({"type": "command_reserved", "payload": {"method": "turn/start"}})]);

        assert_eq!(message::last_message(&fake.session()).expect("read"), None);
    }
}

mod identity {
    use super::*;

    #[test]
    #[expect(clippy::assertions_on_constants, reason = "the constants are the contract of the folder")]
    fn it_is_a_known_agent_with_a_compact_that_takes_instructions() {
        assert_eq!(NAME, "vibe");
        assert_eq!(TITLE, "Vibe CLI");
        assert_eq!(COMPACT.map(|compact| (compact.command, compact.keeps)), Some(("/compact", true)));
        assert_eq!(ADD_DIR, Some("--add-dir"));
        assert!(HOLDS_PROMPTS);
        assert!(!STATUS_BY_FOLDER && !RECORD_BY_FOLDER);
        assert_eq!(MODEL_FLAG, None);
        assert_eq!(
            MODES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["ask", "smart approve", "auto-approve (dangerous)"]
        );
    }
}
