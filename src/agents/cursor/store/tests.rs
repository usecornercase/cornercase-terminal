use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rstest::rstest;

use super::*;
use crate::agents::cursor::fake::{self, FakeCursor};
use crate::test_util::{Sleeper, TempDir};

fn running(cwd: &Path) -> (FakeCursor, Sleeper) {
    let fake = FakeCursor::new(cwd);
    let sleeper = fake.start(cwd);
    (fake, sleeper)
}

fn follow_now(sleeper: &Sleeper, previous: Option<Session>) -> Session {
    follow(sleeper.pid(), SystemTime::now(), previous).expect("the store is found")
}

fn first(sleeper: &Sleeper) -> Session {
    follow_now(sleeper, None)
}

fn model_and_percent(session: &Session) -> (Option<&str>, Option<u16>) {
    (session.context.as_ref().map(|c| c.model.as_str()), session.context.as_ref().and_then(|c| c.percent))
}

mod finding_the_store {
    use super::*;

    #[rstest]
    #[case::a_chat("/c/chats/0123456789abcdef0123456789abcdef/abc-1/store.db", true)]
    #[case::the_wal("/c/chats/0123456789abcdef0123456789abcdef/abc-1/store.db-wal", false)]
    #[case::a_short_bucket("/c/chats/0123/abc-1/store.db", false)]
    #[case::not_hex("/c/chats/zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz/abc-1/store.db", false)]
    #[case::acp_sessions("/c/acp-sessions/abc-1/store.db", false)]
    #[case::odd_id("/c/chats/0123456789abcdef0123456789abcdef/a b/store.db", false)]
    fn takes_only_a_chat_store(#[case] path: &str, #[case] taken: bool) {
        assert_eq!(chat(Path::new(path)).is_some(), taken);
    }

    #[test]
    fn follows_the_store_the_process_holds() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");

        let session = first(&sleeper);

        assert_eq!((session.id.as_str(), session.path.clone()), (fake::ID, fake.store.clone()));
    }

    #[test]
    fn finds_nothing_in_a_process_holding_no_store() {
        let sleeper = Sleeper::with_env(&[]);

        assert!(follow(sleeper.pid(), SystemTime::now(), None).is_none());
    }

    #[rstest]
    #[case::dashes("/home/a/proj", "home-a-proj")]
    #[case::runs_collapse("/home/a/.config/x_y", "home-a-config-x-y")]
    #[case::trimmed("//a//", "a")]
    fn slugs_a_folder_like_cursor(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(slug(path), expected);
    }
}

mod context {
    use super::*;

    #[test]
    fn shows_the_routed_model_and_the_share_of_the_window() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        fake.reply("hi");
        fake.routed("Opus 5.5");
        fake.context(8_000, 200_000);

        assert_eq!(model_and_percent(&first(&sleeper)), (Some("Opus 5.5"), Some(4)));
    }

    #[test]
    fn falls_back_to_the_last_used_model_and_calls_the_default_auto() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        fake.context(1_000, 100_000);

        assert_eq!(model_and_percent(&first(&sleeper)), (Some("Auto"), Some(1)));
        fake.model("gpt-5");
        assert_eq!(model_and_percent(&first(&sleeper)).0, Some("gpt-5"));
    }

    #[test]
    fn hides_the_percentage_after_a_model_change_until_the_next_checkpoint() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        fake.context(50_000, 200_000);
        let session = first(&sleeper);
        assert_eq!(model_and_percent(&session), (Some("Auto"), Some(25)));

        fake.model("sonnet");
        let session = follow_now(&sleeper, Some(session));
        assert_eq!(model_and_percent(&session), (Some("sonnet"), None));

        fake.turn("again");
        let session = follow_now(&sleeper, Some(session));
        assert_eq!(model_and_percent(&session), (Some("sonnet"), Some(25)));
    }

    #[test]
    fn leaves_out_the_percentage_without_a_window() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        fake.context(5, 0);

        assert_eq!(model_and_percent(&first(&sleeper)), (Some("Auto"), None));
    }

    #[test]
    fn follows_a_summary_that_shrinks_the_use() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        fake.context(150_000, 200_000);
        let session = first(&sleeper);
        fake.context(10_000, 200_000);

        assert_eq!(model_and_percent(&follow_now(&sleeper, Some(session))).1, Some(5));
    }

    #[test]
    fn shows_nothing_before_the_first_checkpoint() {
        let dir = TempDir::new();
        let (_fake, sleeper) = running(dir.path());

        let session = first(&sleeper);

        assert_eq!((session.context.is_none(), session.resumable), (true, false));
    }
}

mod prompts {
    use super::*;

    fn counted(session: &Session) -> Vec<String> {
        session.counted.iter().map(|record| record.id.clone()).collect()
    }

    #[test]
    fn counts_a_new_turn_once_and_not_the_history_it_found() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("old");
        let session = first(&sleeper);
        assert!(session.counted.is_empty() && session.prompted.is_none());

        fake.turn("new");
        let session = follow_now(&sleeper, Some(session));
        assert_eq!(counted(&session), ["message-1"]);
        assert!(session.prompted.is_some());

        fake.reply("done");
        let session = follow_now(&sleeper, Some(session));
        assert!(session.counted.is_empty() && session.prompted.is_none());
    }

    #[test]
    fn counts_the_first_turn_of_a_chat_born_after_the_pane_started() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        let later = SystemTime::now() + Duration::from_secs(60);
        fake.born(u64::try_from(later.duration_since(UNIX_EPOCH).expect("time").as_millis()).expect("ms"));
        fake.turn("first");

        assert_eq!(counted(&first(&sleeper)), ["message-0"]);
    }

    #[test]
    fn times_a_prompt_by_the_message_or_by_the_read() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        let session = first(&sleeper);
        fake.turn_at("timed", Some(1_790_000_000_000));
        let timed = follow_now(&sleeper, Some(session));
        assert_eq!(timed.prompted, Some(UNIX_EPOCH + Duration::from_secs(1_790_000_000)));

        fake.turn_at("untimed", None);
        let before = SystemTime::now();
        let untimed = follow_now(&sleeper, Some(timed));
        assert!(untimed.prompted.is_some_and(|at| at >= before));
    }

    #[test]
    fn leaves_shell_turns_out() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        let session = first(&sleeper);
        fake.shell_turn("ls");

        let session = follow_now(&sleeper, Some(session));

        assert!(session.counted.is_empty() && session.resumable);
    }

    #[test]
    fn knows_the_conversation_once_it_has_a_turn() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        let session = first(&sleeper);
        assert!(!session.resumable);
        fake.turn("hello");

        assert!(follow_now(&sleeper, Some(session)).resumable);
    }
}

mod the_transcript {
    use super::*;

    fn open(session: &Session) -> bool {
        session.turn_open
    }

    #[test]
    fn is_open_from_the_user_line_to_the_end_of_the_turn() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        let session = first(&sleeper);
        assert!(!open(&session));

        fake.turn("work");
        let session = follow_now(&sleeper, Some(session));
        assert!(open(&session));

        fake.reply("done");
        fake.end("success");
        assert!(!open(&follow_now(&sleeper, Some(session))));
    }

    #[rstest]
    #[case::aborted("aborted")]
    #[case::failed("error")]
    fn ends_on_an_aborted_or_failed_turn(#[case] status: &str) {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("work");
        let session = first(&sleeper);
        assert!(open(&session));

        fake.end(status);

        assert!(!open(&follow_now(&sleeper, Some(session))));
    }

    #[test]
    fn stays_open_when_a_new_prompt_replaces_a_turn_without_an_end() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("one");
        let session = first(&sleeper);
        fake.turn("sent now");

        assert!(open(&follow_now(&sleeper, Some(session))));
    }

    #[test]
    fn follows_a_rewritten_file() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("one");
        fake.reply("a longer answer than what the rewrite will keep");
        fake.end("success");
        let session = first(&sleeper);
        assert!(!open(&session));

        std::fs::write(&fake.transcript, "{\"role\":\"user\",\"message\":{}}\n").expect("rewrite");

        assert!(open(&follow_now(&sleeper, Some(session))));
    }

    #[test]
    fn is_closed_when_the_file_is_gone() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("one");
        let session = first(&sleeper);
        std::fs::remove_file(&fake.transcript).expect("remove");

        assert!(!open(&follow_now(&sleeper, Some(session))));
    }

    #[test]
    fn skips_lines_that_are_not_json() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        crate::test_util::append(&fake.transcript, "not json\n{\"role\":\"user\"}\n");

        assert!(open(&first(&sleeper)));
    }
}

mod bad_stores {
    use super::*;

    #[test]
    fn shows_no_context_for_a_root_that_is_not_there() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        fake.root(&"ab".repeat(32));

        let session = first(&sleeper);

        assert_eq!((session.context.is_none(), session.resumable), (true, false));
    }

    #[test]
    fn shows_no_context_for_a_truncated_root() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        let id = fake.blob(&[0x42, 0x20, 1, 2]);
        fake.root(&id);

        let session = first(&sleeper);

        assert_eq!((session.context.is_none(), session.resumable, session.counted.len()), (true, false, 0));
    }

    #[test]
    fn skips_a_turn_it_cannot_read() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        let session = first(&sleeper);
        fake.turn("fine");
        let bad = fake.blob(&[0x0a, 0x7f]);
        let root = [fake::bytes(8, &hex_bytes(&bad))].concat();
        let id = fake.blob(&root);
        fake.root(&id);

        let session = follow_now(&sleeper, Some(session));

        assert_eq!(session.counted.len(), 0);
    }

    fn hex_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex")).collect()
    }

    #[test]
    fn follows_a_store_whose_meta_row_is_garbage() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        fake.turn("hello");
        Connection::open(&fake.store)
            .and_then(|db| db.execute("UPDATE meta SET value = 'zz'", []))
            .expect("break the meta row");

        let session = first(&sleeper);

        assert!(session.context.is_none() && !session.resumable);
    }

    #[test]
    fn follows_a_store_that_is_not_sqlite() {
        let dir = TempDir::new();
        let (fake, sleeper) = running(dir.path());
        std::fs::write(&fake.store, "not a database").expect("overwrite");

        assert!(first(&sleeper).context.is_none());
    }
}

mod the_last_message {
    use super::*;

    fn said(fake: &FakeCursor) -> Option<Said> {
        last_message(&fake.store).expect("readable")
    }

    #[test]
    fn is_the_newest_reply_without_thinking_or_tools() {
        let dir = TempDir::new();
        let fake = FakeCursor::new(dir.path());
        fake.turn("work");
        fake.reply("first note");
        fake.reply("the answer");
        fake.tool("lots of output");
        fake.thinking("hmm");

        let said = said(&fake).expect("a message");

        assert_eq!(said.text, "the answer");
        assert!(said.written.is_some_and(|at| at.starts_with("2026-")));
    }

    #[test]
    fn looks_back_past_a_turn_without_a_reply() {
        let dir = TempDir::new();
        let fake = FakeCursor::new(dir.path());
        fake.turn("one");
        fake.reply("answer to one");
        fake.turn("two");
        fake.thinking("working");
        fake.shell_turn("ls");

        assert_eq!(said(&fake).map(|s| s.text).as_deref(), Some("answer to one"));
    }

    #[test]
    fn is_none_before_any_reply() {
        let dir = TempDir::new();
        let fake = FakeCursor::new(dir.path());
        assert!(said(&fake).is_none());
        fake.turn("one");
        fake.reply("   ");
        assert!(said(&fake).is_none());
    }

    #[test]
    fn is_none_for_a_root_it_cannot_read() {
        let dir = TempDir::new();
        let fake = FakeCursor::new(dir.path());
        fake.turn("one");
        fake.reply("answer");
        let id = fake.blob(&[0x42, 0x20, 1]);
        fake.root(&id);

        assert!(said(&fake).is_none());
    }

    #[test]
    fn fails_for_a_store_that_cannot_be_opened() {
        assert!(last_message(Path::new("/nonexistent/store.db")).is_err());
    }
}

mod searching {
    use super::*;

    #[test]
    fn lists_prompts_and_replies_newest_first_without_thinking_or_tools() {
        let dir = TempDir::new();
        let fake = FakeCursor::new(dir.path());
        fake.turn("one");
        fake.reply("answer one");
        fake.turn("two");
        fake.thinking("hmm");
        fake.tool("output");
        fake.reply("answer two");
        let mut seen = Vec::new();

        said_all(&fake.store, &mut |spoken| {
            seen.push((spoken.role, spoken.said.text));
            true
        })
        .expect("readable");

        let expected =
            [(Role::Reply, "answer two"), (Role::Prompt, "two"), (Role::Reply, "answer one"), (Role::Prompt, "one")];
        assert_eq!(seen, expected.map(|(role, text)| (role, text.to_string())));
    }

    #[test]
    fn stops_when_asked() {
        let dir = TempDir::new();
        let fake = FakeCursor::new(dir.path());
        fake.turn("one");
        fake.reply("answer");
        let mut count = 0;

        said_all(&fake.store, &mut |_| {
            count += 1;
            false
        })
        .expect("readable");

        assert_eq!(count, 1);
    }
}
