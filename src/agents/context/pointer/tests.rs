use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

#[rstest]
#[case::claude(Record::Claude("/a".into()))]
#[case::codex(Record::Codex("/a".into()))]
#[case::gemini(Record::Gemini("/a".into()))]
#[case::opencode(Record::Opencode { database: "/a".into(), session: "s".into() })]
#[case::grok(Record::Grok("/a".into()))]
#[case::cursor(Record::Cursor("/a".into()))]
#[case::copilot(Record::Copilot("/a".into()))]
#[case::vibe(Record::Vibe("/a".into()))]
fn a_pointer_maps_to_its_record_and_back(#[case] record: Record) {
    assert_eq!(Pointer::of(&record).record(), record);
}

#[test]
fn pointer_serializes_as_the_journal_always_wrote_it() {
    let claude = serde_json::to_string(&Pointer::Claude("/a".into())).expect("serialize");
    assert_eq!(claude, r#"{"claude":"/a"}"#);
    let opencode = Pointer::Opencode { database: "/d".into(), session: "s".into() };
    let text = serde_json::to_string(&opencode).expect("serialize");
    assert_eq!(text, r#"{"opencode":{"database":"/d","session":"s"}}"#);
}

#[test]
fn a_pointer_is_readable_while_its_file_exists() {
    let dir = TempDir::new();
    let path = dir.path().join("record.jsonl");
    let pointer = Pointer::Claude(path.clone());
    assert!(!pointer.readable());
    std::fs::write(&path, "{}").expect("write");
    assert!(pointer.readable());
    std::fs::remove_file(&path).expect("remove");
    assert!(!pointer.readable());
}
