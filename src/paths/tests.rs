use super::*;

#[test]
fn lives_next_to_the_config() {
    assert_eq!(secrets(Path::new("/c/cornercase/config.json")), PathBuf::from("/c/cornercase/secrets.json"));
}

#[rstest::rstest]
#[case::default("/c/cornercase/session.json", "/c/cornercase/queue.json")]
#[case::custom("/c/server.json", "/c/server.queue.json")]
#[case::another_server("/c/other.json", "/c/other.queue.json")]
fn queue_files_follow_the_session_without_colliding(#[case] session: &str, #[case] expected: &str) {
    assert_eq!(queue(Path::new(session)), PathBuf::from(expected));
}

#[rstest::rstest]
#[case::default("/c/cornercase/session.json", "/c/cornercase/runs")]
#[case::custom("/c/server.json", "/c/server.runs")]
fn run_folders_follow_the_session_without_colliding(#[case] session: &str, #[case] expected: &str) {
    assert_eq!(runs(Path::new(session)), PathBuf::from(expected));
}

#[test]
fn the_star_state_sits_next_to_the_session() {
    assert_eq!(star(Path::new("/c/session.json")), PathBuf::from("/c/star.json"));
}
