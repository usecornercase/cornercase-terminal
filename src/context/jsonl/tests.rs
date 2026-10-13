use std::io::Write;
use std::time::Duration;

use super::*;
use crate::test_util::TempDir;

#[test]
fn an_old_transcript_starts_at_its_end_and_reads_only_appended_complete_lines() {
    let dir = TempDir::new();
    let path = dir.path().join("old.jsonl");
    std::fs::write(&path, "old\npartial").expect("old transcript");
    let since = std::fs::metadata(&path).expect("metadata").modified().expect("modified") + Duration::from_secs(1);
    let mut tail = Tail::default();
    assert_eq!(tail.read_since(&path, Some(since)).expect("seed").bytes, [] as [u8; 0]);
    assert_eq!(tail.read, 11);
    let mut file = std::fs::OpenOptions::new().append(true).open(&path).expect("append");
    file.write_all(b" continuation\nnew\n").expect("records");
    assert_eq!(tail.read_since(&path, Some(since)).expect("read").bytes, b"new\n");
}
