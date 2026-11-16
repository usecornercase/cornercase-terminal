use super::*;
use crate::agents::headless::feed;
use crate::test_util::fixture;

#[test]
fn a_real_run_gives_the_text_the_model_and_the_tokens() {
    let (ran, last) = feed(read, fixture!("claude/2.1.318/recap-ok.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.text, "hello");
    assert_eq!(ran.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(ran.steps, 1);
    assert_eq!(ran.tokens.output, 73);
}

#[test]
fn an_error_result_fails_with_its_text() {
    let (_, last) = feed(read, fixture!("claude/2.1.318/recap-failed.jsonl"));
    assert_eq!(last.expect("a result").expect_err("failed").to_string(), "Not logged in · Please run /login");
}

#[test]
fn an_empty_result_is_an_error() {
    let (_, last) = feed(read, r#"{"type":"result","subtype":"success","is_error":false,"result":"  "}"#);
    assert!(last.expect("a result").is_err());
}

#[test]
fn a_run_stopped_by_its_turns_is_an_error_named_by_its_subtype() {
    let (_, last) = feed(read, r#"{"type":"result","subtype":"error_max_turns","is_error":false}"#);
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
    let (ran, last) = feed(read, fixture!("claude/2.1.325/bot-new.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
    assert_eq!(ran.steps, 2);
    let steps: Vec<_> = ran.fresh.iter().map(|step| (step.tool.as_str(), step.text.as_str())).collect();
    assert_eq!(steps, [("Bash", "cornercase status --json"), ("Read", "Read /work/bots/prs/bot.md")]);
}

#[test]
fn a_resumed_run_keeps_the_same_session() {
    let (ran, _) = feed(read, fixture!("claude/2.1.325/bot-resumed.jsonl"));
    assert_eq!(ran.session.as_deref(), Some("8b610e1d-23f2-4ef4-8666-258ef8b082dc"));
    assert_eq!(ran.text, "a.txt");
}

#[test]
fn a_result_alone_names_the_session() {
    let (ran, _) =
        feed(read, r#"{"type":"result","subtype":"success","is_error":false,"result":"hi","session_id":"s9"}"#);
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
    assert_eq!(action_of(tool, &input).text, wanted);
}

#[rstest::rstest]
#[case::unix("/home/u/b", false, "//home/u/b")]
#[case::windows(r"C:\Users\u\b", true, "//c/Users/u/b")]
fn the_rule_path_of_a_folder(#[case] folder: &str, #[case] windows: bool, #[case] wanted: &str) {
    assert_eq!(rule_path(folder, windows), wanted);
}

mod model {
    use std::path::{Path, PathBuf};

    use crate::agents::headless::{Access, Ask, Tools, args_of, bot_ask};

    pub(super) fn ask(repositories: &[&str]) -> Ask {
        Ask {
            repositories: repositories.iter().map(PathBuf::from).collect(),
            ..bot_ask(Path::new("/b/prs"), Tools::Bot(Access::Cornercase), None)
        }
    }

    pub(super) fn rules(args: &[String], flag: &str) -> Vec<String> {
        let at = args.iter().position(|arg| arg == flag).expect("the flag");
        args[at + 1].split(',').map(String::from).collect()
    }

    fn glob(pattern: &str, text: &str) -> bool {
        let Some((head, rest)) = pattern.split_once('*') else { return pattern == text };
        let Some(tail) = text.strip_prefix(head) else { return false };
        (0..=tail.len()).filter(|at| tail.is_char_boundary(*at)).any(|at| glob(rest, &tail[at..]))
    }

    fn matches(rule: &str, command: &str) -> bool {
        let Some(inner) = rule.strip_prefix("Bash(").and_then(|rule| rule.strip_suffix(')')) else { return false };
        match inner.strip_suffix(":*") {
            Some(stem) => command == stem || command.strip_prefix(stem).is_some_and(|rest| rest.starts_with(' ')),
            None => glob(inner, command),
        }
    }

    pub(super) fn allows(ask: &Ask, command: &str) -> bool {
        let args = args_of(&super::super::command("claude", ask));
        let allowed = rules(&args, "--allowedTools").iter().any(|rule| matches(rule, command));
        allowed && !rules(&args, "--disallowedTools").iter().any(|rule| matches(rule, command))
    }
}

mod the_rules {
    use super::model::{allows, ask};

    #[rstest::rstest]
    #[case::bare("git -C /w/shop log --oneline -1")]
    #[case::single_quotes("git -C '/w/shop' diff --stat")]
    #[case::double_quotes_and_a_space("git -C \"/w/my shop\" status --short")]
    #[case::a_show_of_a_file("git -C '/w/my shop' show HEAD:f")]
    #[case::gh("gh issue list -R github.com/o/r")]
    #[case::cornercase("cornercase status --json")]
    fn what_was_allowed_live_on_2_1_325(#[case] command: &str) {
        assert!(allows(&ask(&["/w/shop", "/w/my shop"]), command), "{command}");
    }

    #[rstest::rstest]
    #[case::a_config_override("git -C /w/shop -c core.fsmonitor=x status")]
    #[case::output_joined("git -C /w/shop log -1 -p --output=/tmp/x")]
    #[case::output_apart("git -C /w/shop diff --output /tmp/x")]
    #[case::no_index("git diff --no-index a b")]
    #[case::no_folder("git log --oneline")]
    #[case::another_folder("git -C /w/other log")]
    #[case::a_parent("git -C /w/shop/.. log")]
    #[case::an_override_after_quotes("git -C \"/w/shop\" -c x=y log")]
    #[case::a_git_dir("git --git-dir=/w/shop/.git log")]
    #[case::output_after_quotes("git -C '/w/my shop' log --output x")]
    #[case::output_quoted_single("git -C /w/shop log -1 -p '--output=/tmp/x'")]
    #[case::output_quoted_double("git -C /w/shop diff \"--output=/tmp/x\"")]
    #[case::output_quoted_apart("git -C /w/shop show HEAD '--output' /tmp/x")]
    #[case::a_stop("cornercase kill-server")]
    fn what_was_refused_live_on_2_1_325(#[case] command: &str) {
        assert!(!allows(&ask(&["/w/shop", "/w/my shop"]), command), "{command}");
    }
}

mod the_dashed_names {
    use super::super::rule_path;
    use super::model::{ask, rules};
    use crate::agents::headless::args_of;

    #[test]
    fn a_bot_may_not_make_a_name_starting_with_a_dash_at_the_top_of_its_folder() {
        let args = args_of(&super::super::command("claude", &ask(&[])));
        let folder = rule_path("/b/prs", cfg!(windows));
        assert!(rules(&args, "--disallowedTools").contains(&format!("Edit({folder}/-*)")));
        assert!(rules(&args, "--allowedTools").contains(&format!("Edit({folder}/**)")));
    }
}

mod the_brief {
    use super::model::{allows, ask};
    use crate::agents::headless::Access;
    use crate::bots::{Bot, brief, sample};

    #[test]
    fn every_git_and_gh_command_the_brief_teaches_is_allowed() {
        let bot = Bot { access: Access::Cornercase, ..sample("prs", "PRs") };
        let text = brief::brief(&bot, std::path::Path::new("/b"));
        let taught = brief::taught(&text, "/w/my shop");
        assert!(taught.iter().filter(|command| command.starts_with("git -C")).count() >= 4, "{taught:?}");
        assert!(taught.iter().any(|command| command.starts_with("gh ")), "{taught:?}");
        let ask = ask(&["/w/my shop"]);
        for command in taught {
            assert!(allows(&ask, &command), "{command}");
        }
    }
}
