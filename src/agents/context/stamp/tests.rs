use std::io::Write as _;
use std::path::Path;

use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn append(file: &Path, line: &str) {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(file).expect("open the file");
    writeln!(file, "{line}").expect("append a line");
}

mod stamp {
    use super::*;

    #[test]
    fn a_transcript_that_grows_changes_its_stamp_and_one_left_alone_keeps_it() {
        let dir = TempDir::new();
        let transcript = dir.path().join("session.jsonl");
        append(&transcript, r#"{"type":"user"}"#);
        let record = Record::Claude(transcript.clone());
        let before = record.stamp();
        assert_eq!(record.stamp(), before);
        append(&transcript, r#"{"type":"assistant"}"#);
        assert_ne!(record.stamp(), before);
    }

    #[test]
    fn a_record_not_written_yet_changes_once_it_is() {
        let dir = TempDir::new();
        let rollout = dir.path().join("rollout.jsonl");
        let record = Record::Codex(rollout.clone());
        let before = record.stamp();
        assert_eq!(record.stamp(), before);
        append(&rollout, "{}");
        assert_ne!(record.stamp(), before);
    }

    #[rstest]
    #[case::opencode(|database| Record::Opencode { database, session: "ses_1".into() })]
    #[case::cursor(Record::Cursor)]
    fn a_write_to_the_write_ahead_log_changes_a_database_record(#[case] record: fn(PathBuf) -> Record) {
        let dir = TempDir::new();
        let database = dir.path().join("store.db");
        std::fs::write(&database, "database").expect("write the database");
        let record = record(database);
        let before = record.stamp();
        assert_eq!(record.stamp(), before);
        append(&dir.path().join("store.db-wal"), "page");
        assert_ne!(record.stamp(), before);
    }

    #[rstest]
    #[case::grok(Record::Grok, "chat_history.jsonl")]
    #[case::copilot(Record::Copilot, "events.jsonl")]
    fn the_file_inside_a_folder_record_is_what_counts(#[case] record: fn(PathBuf) -> Record, #[case] file: &str) {
        let dir = TempDir::new();
        append(&dir.path().join(file), "{}");
        let record = record(dir.path().to_path_buf());
        let before = record.stamp();
        append(&dir.path().join("other.jsonl"), "{}");
        assert_eq!(record.stamp(), before);
        append(&dir.path().join(file), "{}");
        assert_ne!(record.stamp(), before);
    }

    #[test]
    fn a_new_vibe_journal_changes_the_stamp() {
        let dir = TempDir::new();
        let journal = dir.path().join("journal");
        std::fs::create_dir(&journal).expect("create the journal folder");
        append(&journal.join("1.jsonl"), "{}");
        let record = Record::Vibe(dir.path().to_path_buf());
        let before = record.stamp();
        assert_eq!(record.stamp(), before);
        append(&journal.join("2.jsonl"), "{}");
        assert_ne!(record.stamp(), before);
    }
}
