use super::*;
use crate::agents::headless::{plain_args, steps_of};
use crate::test_util::fixture;

fn argv(tools: Tools, conversation: Option<&str>) -> Vec<String> {
    plain_args(command, tools, conversation)
}

fn to_the_end(text: &str) -> (Ran, Option<Result<()>>) {
    let mut ran = Ran::default();
    for line in text.lines() {
        if let Some(result) = read(line, &mut ran) {
            return (ran, Some(result));
        }
    }
    let last = ended(&mut ran);
    (ran, last)
}

fn env_of(tools: Tools, name: &str) -> Option<String> {
    let ask = Ask { tools, ..Ask::default() };
    let command = command("opencode", &ask);
    let mut found = command.get_envs().filter(|(key, _)| *key == name);
    found.next().and_then(|(_, value)| value).map(|value| value.to_string_lossy().into_owned())
}

#[test]
fn a_first_turn_gives_its_session_the_steps_and_only_the_last_answer() {
    let (ran, last) = to_the_end(fixture!("opencode/1.18.35/bot-new.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("ses_e2a1b2c3d4ffeAbCdEfGhIjKlM"));
    assert_eq!(
        steps_of(&ran),
        [
            ("bash", "cornercase --version"),
            ("write", "write /work/bots/prs/inside.txt"),
            ("write", "write /work/outside/outside.txt"),
            ("write", "write /work/bots/prs/bot.json"),
        ]
    );
    assert_eq!(ran.text, "Steps 1, 2, 5 worked; steps 3, 4, 6, 7 were refused.");
    assert_eq!(
        ran.tokens,
        Used {
            input: 6523 + 102 + 69 + 2514 + 131,
            output: 85 + 79 + 79 + 78 + 27,
            cache_read: 4096 + 10624 + 10752 + 16000 + 18560,
            cache_write: 0
        }
    );
    assert_eq!(ran.model, None);
}

#[test]
fn a_resumed_turn_names_the_same_session() {
    let (ran, last) = to_the_end(fixture!("opencode/1.18.35/bot-resumed.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("ses_e2a1b2c3d4ffeAbCdEfGhIjKlM"));
    assert_eq!(ran.text, "You asked me to say hi in one short sentence.");
    assert_eq!(ran.tokens.output, 12 + 12);
}

#[test]
fn a_failed_turn_fails_with_the_agents_words() {
    let (_, last) = to_the_end(fixture!("opencode/1.18.35/bot-failed.jsonl"));
    let error = last.expect("a result").expect_err("failed");
    assert!(error.to_string().contains("Unexpected server error"), "{error}");
}

#[test]
fn a_resumed_turn_that_compacts_first_answers_with_what_comes_after() {
    let (ran, last) = to_the_end(fixture!("opencode/1.18.35/bot-compacted.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.text, "The keeper's name was Silas.");
    assert_eq!(ran.session.as_deref(), Some("ses_e2a1b2c3d4ffeAbCdEfGhIjKlM"));
    assert_eq!(ran.tokens.output, 317 + 1055 + 10 + 41);
}

#[test]
fn text_written_for_the_model_is_never_the_answer() {
    let synthetic = r#"{"type":"text","sessionID":"s","part":{"type":"text","synthetic":true,"text":"Carry on."}}"#;
    let (ran, last) = to_the_end(synthetic);
    assert!(last.is_none());
    assert_eq!(ran.text, "");
}

#[test]
fn no_step_ends_the_turn_before_the_output_does() {
    for reason in ["stop", "tool-calls"] {
        let line = format!(r#"{{"type":"step_finish","sessionID":"s","part":{{"reason":"{reason}","tokens":{{}}}}}}"#);
        assert!(read(&line, &mut Ran::default()).is_none(), "{reason}");
    }
}

#[test]
fn a_turn_that_ends_without_text_has_no_answer() {
    let (_, last) = to_the_end(r#"{"type":"step_finish","sessionID":"s","part":{"reason":"stop","tokens":{}}}"#);
    assert!(last.is_none());
}

#[test]
fn a_recap_runs_with_read_only_rules_and_no_bypass() {
    let recap = argv(Tools::Recap, None);
    assert_eq!(recap, ["run", "--format", "json", "--agent", RECAP_AGENT, "--model", "m", "--", "- hi"]);
    assert_eq!(env_of(Tools::Recap, PERMISSION_ENV).as_deref(), Some(READ_ONLY));
    for bot in [Tools::Bot(Access::Full), Tools::Bot(Access::Cornercase)] {
        assert_eq!((env_of(bot, PERMISSION_ENV), env_of(bot, CONFIG_ENV)), (None, None));
    }
    let rules: Value = serde_json::from_str(READ_ONLY).expect("the rules are JSON");
    for tool in ["*", "bash", "edit", "task", "external_directory"] {
        assert_eq!(rules[tool], "deny", "{tool}");
    }
    assert_eq!(rules["read"], "allow");
}

#[test]
fn a_recap_runs_as_an_agent_of_its_own_whose_rules_are_the_read_only_ones() {
    let config: Value = serde_json::from_str(&env_of(Tools::Recap, CONFIG_ENV).expect("a config")).expect("json");
    let agent = &config["agent"][RECAP_AGENT];
    assert_eq!(agent["mode"], "primary");
    assert_eq!(agent["permission"], serde_json::from_str::<Value>(READ_ONLY).expect("the rules"));
}

#[test]
fn only_full_access_bypasses_permissions_and_a_session_is_continued() {
    let full = argv(Tools::Bot(Access::Full), Some("ses_1"));
    assert_eq!(full, ["run", "--format", "json", "--auto", "--session", "ses_1", "--model", "m", "--", "- hi"]);
    let other = argv(Tools::Bot(Access::Cornercase), None);
    assert_eq!(other, ["run", "--format", "json", "--model", "m", "--", "- hi"]);
}
