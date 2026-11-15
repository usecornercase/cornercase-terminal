use std::path::Path;

use super::*;
use crate::agents::headless::{args_of, bot_ask, feed, steps_of};
use crate::test_util::fixture;

fn ask(tools: Tools) -> Ask {
    Ask { model: Some("m".into()), ..bot_ask(Path::new("/work/bots/prs"), tools, Some("s")) }
}

fn argv(ask: &Ask) -> Vec<String> {
    args_of(&command("gemini", ask))
}

mod the_stream {
    use super::*;

    #[test]
    fn a_first_turn_gives_its_session_the_steps_and_only_the_last_answer() {
        let (ran, last) = feed(read, fixture!("gemini/0.67.0/bot-new.jsonl"));
        assert!(last.expect("a result").is_ok());
        assert_eq!(ran.session.as_deref(), Some("8b2f4c1e-5d3a-4e7b-9a06-1c2d3e4f5a6b"));
        assert_eq!(
            steps_of(&ran),
            [
                ("run_shell_command", "cornercase --version"),
                ("write_file", "write_file .gemini/settings.json"),
                ("write_file", "write_file notes.md"),
                ("run_shell_command", "cornercase restart --help"),
            ]
        );
        assert_eq!(ran.steps, 4);
        assert_eq!(ran.text, "Steps 1 and 3 worked (steps 2 and 4 were refused by policy).");
        assert_eq!(ran.tokens, Used { input: 21003, output: 167, cache_read: 36956, cache_write: 0 });
        assert_eq!(ran.model.as_deref(), Some("gemini-3.8-flash"));
    }

    #[test]
    fn a_resumed_turn_names_the_same_session() {
        let (ran, last) = feed(read, fixture!("gemini/0.67.0/bot-resumed.jsonl"));
        assert!(last.expect("a result").is_ok());
        assert_eq!(ran.session.as_deref(), Some("8b2f4c1e-5d3a-4e7b-9a06-1c2d3e4f5a6b"));
        assert_eq!(ran.text, "The first command printed version 0.13.80.");
        assert_eq!(ran.steps, 0);
    }

    #[test]
    fn a_failed_turn_fails_with_the_agents_words() {
        let (_, last) = feed(read, fixture!("gemini/0.67.0/bot-failed.jsonl"));
        let error = last.expect("a result").expect_err("failed");
        assert!(error.to_string().contains("no-such-model-xyz is not found"), "{error}");
    }

    #[test]
    fn a_turn_without_an_answer_is_an_error() {
        let (_, last) = feed(read, r#"{"type":"result","status":"success","stats":{}}"#);
        assert_eq!(last.expect("a result").expect_err("empty").to_string(), "Gemini CLI answered nothing");
    }
}

mod the_command {
    use super::*;

    #[test]
    fn a_bot_with_cornercase_access_runs_with_its_rules_file_and_resumes() {
        let ask = ask(Tools::Bot(Access::Cornercase));
        let rules = policy_file(&ask.folder).display().to_string();
        let wanted = [
            "--skip-trust",
            "--output-format",
            "stream-json",
            "--approval-mode",
            "default",
            "--policy",
            &rules,
            "--model",
            "m",
            "--resume",
            "s",
            "--prompt=hi",
        ];
        assert_eq!(argv(&ask), wanted);
    }

    #[test]
    fn full_access_is_yolo_without_a_policy() {
        let args = argv(&ask(Tools::Bot(Access::Full)));
        assert!(args.windows(2).any(|pair| pair == ["--approval-mode", "yolo"]), "{args:?}");
        assert!(!args.iter().any(|arg| arg == "--policy"), "{args:?}");
    }

    #[test]
    fn the_read_only_arm_has_no_policy_and_no_bypass() {
        let args = argv(&ask(Tools::Recap));
        assert!(args.windows(2).any(|pair| pair == ["--approval-mode", "default"]), "{args:?}");
        assert!(!args.iter().any(|arg| arg == "--policy" || arg == "yolo"), "{args:?}");
    }

    #[test]
    fn a_prompt_starting_with_a_dash_stays_one_argument() {
        let args = argv(&Ask { prompt: "- what did I say?".into(), ..ask(Tools::Bot(Access::Full)) });
        assert_eq!(args.last().map(String::as_str), Some("--prompt=- what did I say?"));
    }
}

mod the_rules {
    use super::*;

    const WANTED: &str = r#"[[rule]]
toolName = "run_shell_command"
commandPrefix = ["cornercase", "git log", "git show", "git diff", "git status", "gh pr list", "gh pr view", "gh pr checks", "gh issue list", "gh issue view"]
decision = "allow"
priority = 900

[[rule]]
toolName = "run_shell_command"
commandPrefix = ["cornercase kill-server", "cornercase restart", "cornercase update"]
decision = "deny"
priority = 950

[[rule]]
toolName = ["write_file", "replace", "web_fetch"]
decision = "allow"
priority = 900

[[rule]]
toolName = ["write_file", "replace"]
argsPattern = '"file_path":"[^"]*(\.[gG][eE][mM][iI][nN][iI]|\.[eE][nN][vV]|[bB][oO][tT]\.[jJ][sS][oO][nN]|[cC][hH][aA][tT]\.[jJ][sS][oO][nN][lL])'
decision = "deny"
priority = 950

[[rule]]
toolName = ["write_file", "replace"]
argsPattern = '"file_path":"[^"]*[%\\]'
decision = "deny"
priority = 950
"#;

    #[test]
    fn they_are_what_was_checked_live() {
        assert_eq!(policy(&ask(Tools::Bot(Access::Cornercase))), WANTED);
    }

    fn denies(path: &str) -> bool {
        let text = policy(&ask(Tools::Bot(Access::Cornercase)));
        let patterns = text.lines().filter_map(|line| line.strip_prefix("argsPattern = '")?.strip_suffix('\''));
        let args = format!("{{\"file_path\":{}}}", serde_json::to_string(path).expect("a json string"));
        patterns.into_iter().any(|pattern| regex::Regex::new(pattern).expect("a pattern").is_match(&args))
    }

    #[rstest::rstest]
    #[case::the_settings("/work/bots/prs/.gemini/settings.json")]
    #[case::the_bots_own_file("/work/bots/prs/bot.json")]
    #[case::its_chat("/work/bots/prs/chat.jsonl")]
    #[case::the_environment("/work/bots/prs/.env")]
    #[case::a_percent_escaped_dot("/work/bots/prs/%2Egemini/settings.json")]
    #[case::a_percent_escaped_letter("/work/bots/prs/.%67emini/settings.json")]
    #[case::a_percent_escaped_name("/work/bots/prs/bot%2Ejson")]
    #[case::a_quote_to_end_the_match("/work/bots/prs/\"/../.gemini/settings.json")]
    #[case::another_case("/work/bots/prs/.Gemini/Settings.json")]
    #[case::the_name_in_capitals("/work/bots/prs/BOT.JSON")]
    fn a_write_that_could_reach_what_it_protects_is_denied(#[case] path: &str) {
        assert!(denies(path), "{path}");
    }

    #[rstest::rstest]
    #[case::notes("/work/bots/prs/notes.md")]
    #[case::memory("/work/bots/prs/memory.md")]
    #[case::a_subfolder("/work/bots/prs/reports/today.md")]
    fn an_ordinary_write_is_not(#[case] path: &str) {
        assert!(!denies(path), "{path}");
    }

    #[test]
    fn a_protected_file_is_denied_with_its_dots_escaped() {
        let ask = Ask { protected: vec!["bot.json", "keys.txt"], ..ask(Tools::Bot(Access::Cornercase)) };
        assert!(policy(&ask).contains(r"[kK][eE][yY][sS]\.[tT][xX][tT]"));
    }
}
