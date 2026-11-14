use super::*;
use crate::test_util::fixture;

fn feed(text: &str) -> (Ran, Option<Result<()>>) {
    let mut ran = Ran::default();
    let mut last = None;
    for line in text.lines() {
        last = read(line, &mut ran);
    }
    (ran, last)
}

#[test]
fn a_real_run_gives_the_text_the_model_and_the_tokens() {
    let (ran, last) = feed(fixture!("claude/2.1.318/recap-ok.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.text, "hello");
    assert_eq!(ran.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(ran.steps, 1);
    assert_eq!(ran.tokens.output, 73);
}

#[test]
fn an_error_result_fails_with_its_text() {
    let (_, last) = feed(fixture!("claude/2.1.318/recap-failed.jsonl"));
    assert_eq!(last.expect("a result").expect_err("failed").to_string(), "Not logged in · Please run /login");
}

#[test]
fn an_empty_result_is_an_error() {
    let (_, last) = feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"  "}"#);
    assert!(last.expect("a result").is_err());
}

#[test]
fn a_run_stopped_by_its_turns_is_an_error_named_by_its_subtype() {
    let (_, last) = feed(r#"{"type":"result","subtype":"error_max_turns","is_error":false}"#);
    assert_eq!(last.expect("a result").expect_err("stopped").to_string(), "error_max_turns");
}

#[test]
fn lines_that_are_not_json_or_unknown_are_passed() {
    let mut ran = Ran::default();
    assert!(read("not json", &mut ran).is_none());
    assert!(read(r#"{"type":"rate_limit_event"}"#, &mut ran).is_none());
}

#[test]
fn a_bot_run_gives_its_session_and_each_tool_it_used() {
    let (ran, last) = feed(fixture!("claude/2.1.325/bot-new.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
    assert_eq!(ran.steps, 2);
    let steps: Vec<_> = ran.fresh.iter().map(|step| (step.tool.as_str(), step.text.as_str())).collect();
    assert_eq!(steps, [("Bash", "cornercase status --json"), ("Read", "Read /work/bots/prs/bot.md")]);
}

#[test]
fn a_resumed_run_keeps_the_same_session() {
    let (ran, _) = feed(fixture!("claude/2.1.325/bot-resumed.jsonl"));
    assert_eq!(ran.session.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
    assert_eq!(ran.text, "a.txt");
}

#[test]
fn a_result_alone_names_the_session() {
    let (ran, _) = feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"hi","session_id":"s9"}"#);
    assert_eq!(ran.session.as_deref(), Some("s9"));
}

#[rstest::rstest]
#[case::file("Write", r#"{"file_path":"/a/b.txt"}"#, "Write /a/b.txt")]
#[case::search("WebSearch", r#"{"query":"rust async"}"#, "WebSearch rust async")]
#[case::fetch("WebFetch", r#"{"url":"https://x.dev"}"#, "WebFetch https://x.dev")]
#[case::grep("Grep", r#"{"pattern":"todo"}"#, "Grep todo")]
#[case::command("Bash", r#"{"command":"gh pr list\nmore"}"#, "gh pr list")]
#[case::unknown("Skill", r#"{"skill":"x"}"#, "Skill")]
fn a_tool_is_summed_up_in_one_line(#[case] tool: &str, #[case] input: &str, #[case] wanted: &str) {
    let input: Value = serde_json::from_str(input).expect("json");
    assert_eq!(summary(tool, &input), wanted);
}

#[rstest::rstest]
#[case::unix("/home/u/b", false, "//home/u/b")]
#[case::windows(r"C:\Users\u\b", true, "//c/Users/u/b")]
fn the_rule_path_of_a_folder(#[case] folder: &str, #[case] windows: bool, #[case] wanted: &str) {
    assert_eq!(rule_path(folder, windows), wanted);
}
