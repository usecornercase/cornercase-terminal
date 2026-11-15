use std::ffi::OsString;
use std::path::Path;

use super::*;
use crate::agents::headless::{Used, args_of, bot_ask, feed, steps_of};
use crate::test_util::fixture;

fn ask(tools: Tools) -> Ask {
    let folder = Path::new("/work/bots/prs");
    Ask { model: Some("gpt-x".into()), effort: Some("low".into()), ..bot_ask(folder, tools, Some("s")) }
}

fn argv(ask: &Ask) -> Vec<String> {
    args_of(&command("copilot", ask))
}

mod the_stream {
    use super::*;

    #[test]
    fn a_first_turn_gives_its_session_the_steps_and_only_the_last_answer() {
        let (ran, last) = feed(read, fixture!("copilot/1.0.98/bot-new.jsonl"));
        assert!(last.expect("a result").is_ok());
        assert_eq!(ran.session.as_deref(), Some("b7d0c3a2-6e19-4c84-95f1-2a8e4d6c1b07"));
        assert_eq!(
            steps_of(&ran),
            [
                ("bash", "cornercase --version"),
                ("bash", "cornercase restart --help"),
                ("apply_patch", "apply_patch memory.md"),
            ]
        );
        assert_eq!(
            ran.text,
            "Version `0.13.80` printed, and the second command did not run because permission was denied."
        );
        assert_eq!(ran.tokens, Used::default());
        assert_eq!(ran.model.as_deref(), Some("gpt-5.6-luna"));
    }

    #[test]
    fn a_resumed_turn_names_the_same_session() {
        let (ran, last) = feed(read, fixture!("copilot/1.0.98/bot-resumed.jsonl"));
        assert!(last.expect("a result").is_ok());
        assert_eq!(ran.session.as_deref(), Some("b7d0c3a2-6e19-4c84-95f1-2a8e4d6c1b07"));
        assert_eq!(ran.text, "The first command printed version `0.13.80`.");
    }

    #[test]
    fn chunks_of_the_answer_are_joined_and_a_subagents_message_is_not_the_answer() {
        let chunks = fixture!("copilot/1.0.98/chunks.jsonl");
        let sub = r#"{"type":"assistant.message","data":{"content":"inner","parentToolCallId":"t1"}}"#;
        let done = r#"{"type":"result","sessionId":"s","exitCode":0}"#;
        let (ran, last) = feed(read, &format!("{chunks}\n{sub}\n{done}"));
        assert!(last.expect("a result").is_ok());
        assert_eq!(ran.text, "First half. Second half.");
    }

    #[test]
    fn a_result_with_an_exit_code_fails() {
        let (_, last) = feed(read, r#"{"type":"result","sessionId":"s","exitCode":1}"#);
        assert_eq!(last.expect("a result").expect_err("failed").to_string(), "copilot exited with 1");
    }

    #[test]
    fn a_turn_without_an_answer_is_an_error() {
        let (_, last) = feed(read, r#"{"type":"result","sessionId":"s","exitCode":0}"#);
        assert_eq!(last.expect("a result").expect_err("empty").to_string(), "GitHub Copilot CLI answered nothing");
    }
}

mod the_command {
    use super::*;

    #[test]
    fn a_bot_with_cornercase_access_may_run_cornercase_and_write_in_its_folder() {
        let args = argv(&ask(Tools::Bot(Access::Cornercase)));
        let rules: Vec<&str> = args
            .windows(2)
            .filter(|pair| pair[0] == "--allow-tool" || pair[0] == "--deny-tool")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(rules.len(), 16);
        for wanted in [
            "shell(cornercase:*)",
            "shell(git log:*)",
            "shell(gh issue view:*)",
            "write",
            "shell(cornercase kill-server:*)",
            "shell(cornercase restart:*)",
            "write(/work/bots/prs/bot.json)",
            "write(/work/bots/prs/chat.jsonl)",
        ] {
            assert!(rules.contains(&wanted), "{wanted} in {rules:?}");
        }
        assert!(!args.iter().any(|arg| arg == "--allow-all"), "{args:?}");
        assert_eq!(args.last().map(String::as_str), Some("--prompt=hi"));
        assert!(args.contains(&String::from("--resume=s")), "{args:?}");
    }

    #[test]
    fn full_access_allows_everything_and_no_rules() {
        let args = argv(&ask(Tools::Bot(Access::Full)));
        assert!(
            args.contains(&String::from("--allow-all")) && args.contains(&String::from("--no-ask-user")),
            "{args:?}"
        );
        assert!(!args.iter().any(|arg| arg == "--allow-tool"), "{args:?}");
    }

    #[test]
    fn an_effort_needs_a_named_model_that_is_not_auto() {
        let with = |model: Option<&str>| {
            let ask = Ask { model: model.map(str::to_string), ..ask(Tools::Bot(Access::Full)) };
            argv(&ask).iter().any(|arg| arg == "--reasoning-effort")
        };
        assert_eq!([with(None), with(Some("auto")), with(Some("gpt-x"))], [false, false, true]);
    }

    #[test]
    fn what_would_loosen_its_rules_is_removed_from_its_environment() {
        let command = command("copilot", &ask(Tools::Bot(Access::Cornercase)));
        let removed: Vec<OsString> =
            command.get_envs().filter(|(_, value)| value.is_none()).map(|(key, _)| key.to_os_string()).collect();
        for name in ["COPILOT_ALLOW_ALL", "COPILOT_ASSISTED_APPROVAL", "COPILOT_CLI"] {
            assert!(removed.contains(&OsString::from(name)), "{name} in {removed:?}");
        }
    }
}
