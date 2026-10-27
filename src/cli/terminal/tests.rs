use super::*;
use crate::test_util::TempDir;

#[test]
fn a_file_is_named_after_itself_and_standard_input_has_no_name() {
    assert_eq!(default_name(Path::new("/x/y/shot.png")), "shot.png");
    assert_eq!(default_name(Path::new("-")), "");
}

#[test]
fn an_empty_file_is_refused_before_connecting() {
    let temp = TempDir::new();
    let file = temp.path().join("empty.png");
    std::fs::write(&file, b"").expect("file");

    let refused = pasted_bytes(&file).expect_err("refused");

    assert!(refused.to_string().contains("is empty: there is nothing to paste"), "{refused}");
}

#[test]
fn a_file_is_read_whole() {
    let temp = TempDir::new();
    let file = temp.path().join("a.txt");
    std::fs::write(&file, b"hello").expect("file");

    assert_eq!(pasted_bytes(&file).expect("bytes"), b"hello");
}

#[test]
fn a_missing_file_says_which() {
    let refused = pasted_bytes(Path::new("/nonexistent/a.png")).expect_err("refused");

    assert!(refused.to_string().contains("cannot read `/nonexistent/a.png`"), "{refused}");
}
