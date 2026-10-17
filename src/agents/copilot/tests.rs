use rstest::rstest;

use super::*;
use crate::agents::{DEFAULT_TRUST_PROMPT, TrustPrompt, asks_trust};
use crate::test_util::fixture;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|a| (*a).to_string()).collect()
}

#[rstest]
#[case::yolo(&["--yolo"], &["--allow-all"])]
#[case::allow_all(&["--allow-all"], &["--allow-all"])]
#[case::plan_flag(&["--plan"], &["--mode", "plan"])]
#[case::autopilot_flag(&["--autopilot"], &["--mode", "autopilot"])]
#[case::mode_equals(&["--mode=autopilot"], &["--mode", "autopilot"])]
#[case::mode_apart(&["--mode", "plan"], &["--mode", "plan"])]
#[case::narrower_flags(&["--allow-all-tools"], &["--allow-all-tools"])]
#[case::other_flags(&["--model", "auto", "--no-ask-user"], &["--model", "auto", "--no-ask-user"])]
fn normalizes_the_spellings_of_a_mode(#[case] given: &[&str], #[case] expected: &[&str]) {
    assert_eq!(normalized(&args(given)), args(expected));
}

#[test]
fn resumes_by_session_id_with_an_equals_sign() {
    assert_eq!(resume(args(&["--allow-all"]), "abc".into()), Some(args(&["--allow-all", "--resume=abc"])));
}

#[rstest]
#[case::slash("/clear", true)]
#[case::shell(" !ls", true)]
#[case::help("?", true)]
#[case::a_question("? why", false)]
#[case::text("fix the bug", false)]
fn knows_local_commands(#[case] text: &str, #[case] local: bool) {
    assert_eq!(local_command(text), local);
}

#[rstest]
#[case::npm("/usr/lib/node_modules/@github/copilot/npm-loader.js", true)]
#[case::npx("/home/a/.npm/_npx/9f2/node_modules/@github/copilot/npm-loader.js", true)]
#[case::another_package("/usr/lib/node_modules/other/npm-loader.js", false)]
#[case::another_scope("/usr/lib/node_modules/@acme/copilot/npm-loader.js", false)]
#[case::another_file("/usr/lib/node_modules/@github/copilot/index.js", false)]
#[case::a_bare_name("npm-loader.js", false)]
fn knows_the_npm_loader(#[case] path: &str, #[case] known: bool) {
    assert_eq!(runs_as(path), known);
}

#[test]
fn the_trust_screen_matches_the_default_prompt_and_enter_answers_it() {
    let mut trust = TrustPrompt::default();
    let pattern = trust.regex(DEFAULT_TRUST_PROMPT).expect("pattern");
    let screen = fixture!("copilot/1.0.98/trust.txt");

    assert_eq!(
        (asks_trust(pattern, screen), crate::agents::launch::answer_keys(screen, false)),
        (true, b"\r".to_vec())
    );
}
