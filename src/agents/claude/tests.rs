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
