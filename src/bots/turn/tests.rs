use std::ffi::OsString;
use std::path::Path;

use super::*;
use crate::bots::folder;
use crate::bots::sample;
use crate::test_util::{TempDir, fake_program};

fn input(folder: &Path, conversation: Option<&str>) -> Input {
    Input {
        bot: Bot { conversation: conversation.map(str::to_string), ..sample("prs", "PRs") },
        folder: folder.to_path_buf(),
        text: String::from("hello"),
        session: Session::Chat,
        from_cornercase: false,
        told: None,
        program: String::from("claude"),
        home: None,
        socket: Some(String::from("/s")),
        bin: None,
        timeout: Duration::from_secs(1),
    }
}

#[test]
fn the_links_folder_goes_first_and_the_path_follows_unchanged() {
    let dir = TempDir::new();
    let server = dir.path().join("server");
    let folder = dir.path().join("bin");
    std::fs::create_dir_all(&server).expect("server");
    let binary = fake_program(&server, "cornercase");
    assert_eq!(first_on_path(Some(&folder), &binary), Some(folder.clone()));
    let rest = std::env::join_paths([dir.path().join("a"), dir.path().join("b")]).expect("paths");
    let path = path_with(&folder, Some(&rest)).expect("a path");
    let found: Vec<_> = std::env::split_paths(&path).collect();
    assert_eq!(found, [folder.clone(), dir.path().join("a"), dir.path().join("b")]);
    let alone = path_with(&folder, None).expect("a path");
    assert_eq!(std::env::split_paths(&alone).collect::<Vec<_>>(), [folder]);
}

#[test]
fn a_program_beside_the_servers_binary_does_not_shadow_one_earlier_on_the_path() {
    let dir = TempDir::new();
    let (server, yours, folder) = (dir.path().join("server"), dir.path().join("yours"), dir.path().join("bin"));
    std::fs::create_dir_all(&server).expect("server");
    std::fs::create_dir_all(&yours).expect("yours");
    let binary = fake_program(&server, "cornercase");
    fake_program(&server, "gh");
    let own_gh = fake_program(&yours, "gh");
    let user = std::env::join_paths([&yours, &server]).expect("paths");
    let first = first_on_path(Some(&folder), &binary).expect("a folder");
    let path = path_with(&first, Some(&user)).expect("a path");
    let extensions = if cfg!(windows) { crate::process::program::extensions(None) } else { Vec::new() };
    let find = crate::process::program::on_path(cfg!(windows), Some(&path), &extensions);
    assert_eq!(find("gh"), Some(own_gh));
    let ours = find("cornercase").expect("cornercase");
    assert_eq!(ours.parent(), Some(folder.as_path()));
    let (ours, theirs) = (crate::os::fs::stat(&ours).expect("link"), crate::os::fs::stat(&binary).expect("binary"));
    assert!(ours.same_file(&theirs));
}

#[test]
fn without_a_folder_or_with_one_that_cannot_be_made_the_binarys_folder_goes_first() {
    let dir = TempDir::new();
    let binary = fake_program(dir.path(), "cornercase");
    assert_eq!(first_on_path(None, &binary).as_deref(), binary.parent());
    let file = dir.path().join("file");
    std::fs::write(&file, "x").expect("file");
    assert_eq!(first_on_path(Some(&file.join("bin")), &binary).as_deref(), binary.parent());
    assert_eq!(first_on_path(Some(dir.path()), Path::new("")), None);
}

struct Removed(PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_relative_folder_is_made_absolute_so_a_child_in_another_folder_finds_the_link() {
    let dir = TempDir::new();
    let binary = fake_program(dir.path(), "cornercase");
    let relative = Path::new("target").join(format!("link-test-{}", std::process::id()));
    let here = std::env::current_dir().expect("cwd");
    let _removed = Removed(here.join(&relative));
    let first = first_on_path(Some(&relative), &binary).expect("a folder");
    assert!(first.is_absolute(), "{first:?}");
    assert_eq!(first, here.join(&relative));
    let path = path_with(&first, None).expect("a path");
    let extensions = if cfg!(windows) { crate::process::program::extensions(None) } else { Vec::new() };
    let found = crate::process::program::on_path(cfg!(windows), Some(&path), &extensions)("cornercase");
    assert_eq!(found.as_deref().and_then(Path::parent), Some(first.as_path()));
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

#[test]
fn a_run_opens_a_new_session_even_when_the_bot_has_a_conversation() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "review").expect("create");
    let run = Input {
        session: Session::NewRun(12),
        text: String::from("This is your routine \"x\" (every 2h). Go."),
        ..input(&made, Some("s1"))
    };
    let asked = ask(&run, None, None);
    assert_eq!(asked.conversation, None);
    assert!(asked.prompt.contains("You are PRs") && asked.prompt.ends_with("Go."), "{}", asked.prompt);
    assert!(!asked.prompt.contains("The user says"));
}

#[test]
fn what_the_routines_answered_is_put_in_front_of_a_resumed_message_once_given() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "").expect("create");
    let told = Some(String::from("## Since your last message, your routines answered\n\n[A, 09:00]\nx"));
    let asked = ask(&Input { told, ..input(&made, Some("s1")) }, None, None);
    assert!(asked.prompt.starts_with("## Since your last message"), "{}", asked.prompt);
    assert!(asked.prompt.ends_with("## The user says\n\nhello"));
    assert_eq!(asked.conversation.as_deref(), Some("s1"));
}

fn variable(asked: &Ask, key: &str) -> std::result::Result<Option<OsString>, ()> {
    asked.env.iter().find(|(name, _)| *name == key).map(|(_, value)| value.clone()).ok_or(())
}

#[test]
fn a_run_follow_up_resumes_the_run_session_and_sets_the_run_variable() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "").expect("create");
    let follow = Input {
        session: Session::Resume { run: 12, session: String::from("run-s") },
        text: String::from("The agent you started finished."),
        ..input(&made, Some("chat-s"))
    };
    let asked = ask(&follow, None, None);
    assert_eq!(asked.conversation.as_deref(), Some("run-s"));
    assert_eq!(asked.prompt, "The agent you started finished.");
    assert_eq!(variable(&asked, "CORNERCASE_BOT_RUN"), Ok(Some(OsString::from("12"))));
}

#[test]
fn a_chat_turn_removes_the_run_variable() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "").expect("create");
    let asked = ask(&input(&made, Some("s1")), None, None);
    assert_eq!(variable(&asked, "CORNERCASE_BOT_RUN"), Ok(None));
}

#[test]
fn a_reports_only_opening_says_cornercase_says() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "").expect("create");
    let first = ask(&Input { from_cornercase: true, ..input(&made, None) }, None, None);
    assert!(first.prompt.ends_with("## cornercase says\n\nhello"), "{}", first.prompt);
    let next = ask(&Input { from_cornercase: true, ..input(&made, Some("s1")) }, None, None);
    assert_eq!(next.prompt, "## cornercase says\n\nhello");
    let plain = ask(&input(&made, Some("s1")), None, None);
    assert_eq!(plain.prompt, "hello");
}

#[test]
fn a_setup_opens_a_new_session_with_the_setup_section_even_when_the_bot_has_a_conversation() {
    let dir = TempDir::new();
    let made = folder::create(dir.path(), &sample("prs", "PRs"), "review").expect("create");
    let setup = Input { session: Session::Setup, ..input(&made, Some("s1")) };
    let asked = ask(&setup, None, None);
    assert_eq!(asked.conversation, None);
    assert!(asked.prompt.contains("You are PRs") && asked.prompt.contains("## You were just made"), "{}", asked.prompt);
    assert!(!asked.prompt.contains("The user says"));
}
