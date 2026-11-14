use std::ffi::OsString;
use std::path::Path;

use super::*;
use crate::bots::folder;
use crate::bots::sample;
use crate::test_util::TempDir;

fn input(folder: &Path, conversation: Option<&str>) -> Input {
    Input {
        bot: Bot { conversation: conversation.map(str::to_string), ..sample("prs", "PRs") },
        folder: folder.to_path_buf(),
        text: String::from("hello"),
        program: String::from("claude"),
        home: None,
        socket: Some(String::from("/s")),
        timeout: Duration::from_secs(1),
    }
}

#[test]
fn the_servers_folder_goes_first_on_the_path_and_the_rest_stays() {
    let bin = Path::new("/the/bin");
    let rest = std::env::join_paths(["/a", "/b"]).expect("paths");
    let path = path_with(bin, Some(&rest)).expect("a path");
    let found: Vec<_> = std::env::split_paths(&path).collect();
    assert_eq!(found, [Path::new("/the/bin"), Path::new("/a"), Path::new("/b")]);
    let alone = path_with(bin, None).expect("a path");
    assert_eq!(std::env::split_paths(&alone).collect::<Vec<_>>(), [Path::new("/the/bin")]);
}

#[test]
fn held_messages_are_joined_by_a_blank_line() {
    assert_eq!(joined(&[String::from("a"), String::from("b")]), "a\n\nb");
    assert_eq!(joined(&[String::from("a")]), "a");
}

#[test]
fn a_new_conversation_opens_with_the_brief_and_a_continued_one_sends_the_text_alone() {
    let dir = TempDir::new();
    let bot = sample("prs", "PRs");
    let made = folder::create(dir.path(), &bot, "review").expect("create");
    let opened = ask(&input(&made, None), None, None);
    assert!(opened.prompt.contains("You are PRs") && opened.prompt.contains("review"), "{}", opened.prompt);
    assert!(opened.prompt.ends_with("hello"));
    let next = ask(&input(&made, Some("s1")), None, None);
    assert_eq!((next.prompt.as_str(), next.conversation.as_deref()), ("hello", Some("s1")));
}

#[test]
fn the_environment_names_the_bot_and_takes_the_place_out() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "").expect("create");
    let asked = ask(&input(&made, None), Some(Path::new("/the/bin")), Some(OsStr::new("/usr/bin")));
    let value = |key: &str| asked.env.iter().find(|(name, _)| *name == key).map(|(_, value)| value.clone());
    assert_eq!(value("CORNERCASE_BOT"), Some(Some(OsString::from("prs"))));
    for key in ["CORNERCASE_PANE", "CORNERCASE_SERVER", "CORNERCASE_REMOTE"] {
        assert_eq!(value(key), Some(None), "{key}");
    }
    let path = value("PATH").flatten().expect("a path");
    assert_eq!(std::env::split_paths(&path).next().as_deref(), Some(Path::new("/the/bin")));
    assert_eq!(asked.socket.as_deref(), Some("/s"));
}

#[test]
fn a_failure_keeps_the_agents_words() {
    let error = Error::Probe(String::from("Not logged in · Please run /login"));
    assert_eq!(failure("claude", &error, "prs"), "Not logged in · Please run /login");
    let slow = Error::Probe(String::from("claude did not answer in time"));
    assert_eq!(failure("claude", &slow, "prs"), "claude did not answer in time");
}

#[test]
fn running_out_of_steps_says_so() {
    let error = Error::Probe(String::from("error_max_turns"));
    assert_eq!(failure("claude", &error, "prs"), "claude stopped after 40 steps, its limit for one message");
}

#[rstest::rstest]
#[case::claude("Prompt is too long")]
#[case::window("exceeds the context window")]
#[case::codex("context_length_exceeded")]
fn a_full_context_says_to_start_a_new_conversation(#[case] words: &str) {
    let text = failure("claude", &Error::Probe(words.to_string()), "prs");
    assert_eq!(text, format!("{words}; start a new conversation: cornercase bot reset prs"));
}
