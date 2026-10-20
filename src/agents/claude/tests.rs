use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

pub(in crate::agents) fn session(text: &str, pid: i32) -> Option<Activity> {
    Session::parse(text, pid).and_then(|s| s.activity())
}

fn claude(dir: Option<&Path>, pid: i32, title: &str) -> Activity {
    Reader { session: Session::read(dir, pid), ..Reader::default() }.activity(title)
}

mod session_file {
    use super::*;

    #[rstest]
    #[case::busy(r#"{"pid":7,"status":"busy"}"#, Some(Activity::Working))]
    #[case::permission(r#"{"pid":7,"status":"waiting","waitingFor":"permission prompt"}"#, Some(Activity::Waiting))]
    #[case::question(r#"{"pid":7,"status":"waiting","waitingFor":"input needed"}"#, Some(Activity::Waiting))]
    #[case::idle(r#"{"pid":7,"status":"idle"}"#, Some(Activity::Idle))]
    #[case::background_shell(r#"{"pid":7,"status":"shell"}"#, Some(Activity::Shell))]
    #[case::no_status_yet(r#"{"pid":7,"sessionId":"a"}"#, None)]
    #[case::unknown_status(r#"{"pid":7,"status":"dreaming"}"#, None)]
    #[case::another_process(r#"{"pid":8,"status":"busy"}"#, None)]
    #[case::half_written(r#"{"pid":7,"sta"#, None)]
    fn says_what_claude_is_doing(#[case] text: &str, #[case] expected: Option<Activity>) {
        assert_eq!(session(text, 7), expected);
    }

    #[test]
    fn names_the_conversation_and_where_it_started() {
        let text = r#"{"pid":7,"sessionId":"a01c","cwd":"/home/a/shop","status":"idle","version":"2.1.289"}"#;

        let found = Session::parse(text, 7).map(|s| (s.id, s.cwd));

        assert_eq!(found, Some((Some("a01c".into()), Some(PathBuf::from("/home/a/shop")))));
    }

    #[test]
    fn is_read_from_the_sessions_folder() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join("sessions")).expect("create sessions");
        std::fs::write(dir.path().join("sessions/42.json"), r#"{"pid":42,"status":"waiting"}"#).expect("write");

        assert_eq!(claude(Some(dir.path()), 42, "✳ Claude Code"), Activity::Waiting);
    }

    #[test]
    fn falls_back_to_the_title_without_one() {
        let dir = TempDir::new();

        assert_eq!(claude(Some(dir.path()), 42, "◑ fix the login"), Activity::Working);
    }

    #[test]
    fn counts_as_idle_without_a_file_or_a_title() {
        assert_eq!(claude(None, 42, ""), Activity::Idle);
    }
}

mod title {
    use super::*;

    #[rstest]
    #[case::half_circle("◐ fix the login", Some(Activity::Working))]
    #[case::other_half("◑ fix the login", Some(Activity::Working))]
    #[case::braille("⠂ fix the login", Some(Activity::Working))]
    #[case::idle("✳ Claude Code", Some(Activity::Idle))]
    #[case::shell_title("zsh", None)]
    #[case::glyph_without_space("◐x", None)]
    #[case::empty("", None)]
    fn shows_whether_claude_works(#[case] title: &str, #[case] expected: Option<Activity>) {
        assert_eq!(from_title(title), expected);
    }
}

mod config_dir {
    use super::*;

    #[rstest]
    #[case::default(None, Some("/home/a/.claude"))]
    #[case::from_the_environment(Some("/opt/claude"), Some("/opt/claude"))]
    #[case::empty_variable(Some(""), Some("/home/a/.claude"))]
    fn is_where_claude_keeps_its_sessions(#[case] env: Option<&str>, #[case] expected: Option<&str>) {
        let found = claude_dir_from(env.map(OsString::from), Some(Path::new("/home/a")));
        assert_eq!(found, expected.map(PathBuf::from));
    }

    #[rstest]
    #[case::not_set(&[("HOME", "/home/a")], None)]
    #[case::set(&[("CLAUDE_CONFIG_DIR", "/home/a/accounts/work")], Some("/home/a/accounts/work"))]
    #[case::empty(&[("CLAUDE_CONFIG_DIR", "")], None)]
    #[case::relative(&[("CLAUDE_CONFIG_DIR", "work")], None)]
    fn of_an_agent_comes_from_its_own_environment(#[case] env: &[(&str, &str)], #[case] expected: Option<&str>) {
        let env = env.iter().map(|(name, value)| ((*name).to_string(), (*value).to_string())).collect();
        assert_eq!(own_dir(env), expected.map(PathBuf::from));
    }
}

mod conversation_files {
    use super::*;
    use crate::agents::Known;
    use crate::agents::claude::transcript::transcript_path;
    use crate::agents::context::tests::pane_look;
    use crate::agents::context::{Look, Pane, Record};

    struct Session {
        home: TempDir,
        work: TempDir,
        transcript: PathBuf,
    }

    impl Session {
        fn new() -> Self {
            let home = TempDir::new();
            let work = TempDir::new();
            let transcript = transcript_path(home.path(), work.path(), "session-1").expect("path");
            std::fs::create_dir_all(transcript.parent().expect("parent")).expect("project folder");
            std::fs::write(&transcript, "{}\n").expect("transcript");
            std::fs::create_dir(home.path().join("sessions")).expect("sessions");
            std::fs::write(
                home.path().join("sessions/7.json"),
                serde_json::json!({"pid": 7, "sessionId": "session-1", "cwd": work.path()}).to_string(),
            )
            .expect("session");
            Self { home, work, transcript }
        }

        fn files(&self) -> Option<Vec<(PathBuf, PathBuf)>> {
            let mut pane = Pane::default();
            pane.update(Some(Known::Claude), &Look { claude_dir: Some(self.home.path()), ..pane_look(7) });
            pane.conversation_files()
        }

        fn relative(&self) -> PathBuf {
            self.transcript.strip_prefix(self.home.path()).expect("under the home").to_path_buf()
        }
    }

    #[test]
    fn are_the_transcript_under_the_folder_of_the_sessions_cwd() {
        let session = Session::new();

        let files = session.files().expect("files");

        assert_eq!(files, vec![(session.transcript.clone(), session.relative())]);
        assert_eq!(Some(session.relative()), transcript_path(Path::new(""), session.work.path(), "session-1"));
    }

    #[test]
    fn include_the_folder_of_its_subagents_when_there_is_one() {
        let session = Session::new();
        let folder = session.transcript.with_extension("");
        std::fs::create_dir_all(folder.join("subagents")).expect("subagents");
        std::fs::write(folder.join("subagents/agent-a1.jsonl"), "{}\n").expect("subagent");

        let files = session.files().expect("files");

        assert_eq!(
            files,
            vec![(session.transcript.clone(), session.relative()), (folder, session.relative().with_extension("")),]
        );
    }

    #[test]
    fn are_none_for_a_transcript_outside_a_projects_folder() {
        assert_eq!(Record::Claude(PathBuf::from("/tmp/session.jsonl")).conversation_files(), None);
    }

    #[test]
    fn are_none_for_gemini_which_starts_afresh() {
        assert_eq!(Record::Gemini(PathBuf::from("/home/.gemini/tmp/chats/session.json")).conversation_files(), None);
    }
}
