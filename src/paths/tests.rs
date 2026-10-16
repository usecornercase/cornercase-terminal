use super::*;

#[test]
fn lives_next_to_the_config() {
    assert_eq!(secrets(Path::new("/c/cornercase/config.json")), PathBuf::from("/c/cornercase/secrets.json"));
}

#[test]
fn plugin_folders_never_meet_when_config_and_state_share_a_folder() {
    let next_to_the_socket = Path::new("/run/cc/server.json");
    let config = plugin_config(Path::new("/run/cc/config.json"), "acme.notes");
    let state = plugin_state(next_to_the_socket, "acme.notes");
    assert_eq!(
        (plugins(Path::new("/run/cc/config.json")), config, state),
        (
            PathBuf::from("/run/cc/plugins.json"),
            PathBuf::from("/run/cc/plugins/config/acme.notes"),
            PathBuf::from("/run/cc/plugins/state/acme.notes")
        )
    );
}

#[rstest::rstest]
#[case::default("/c/cornercase/session.json", "/c/cornercase/queue.json")]
#[case::custom("/c/server.json", "/c/server.queue.json")]
#[case::another_server("/c/other.json", "/c/other.queue.json")]
fn queue_files_follow_the_session_without_colliding(#[case] session: &str, #[case] expected: &str) {
    assert_eq!(queue(Path::new(session)), PathBuf::from(expected));
}

#[test]
fn the_star_state_sits_next_to_the_session() {
    assert_eq!(star(Path::new("/c/session.json")), PathBuf::from("/c/star.json"));
}
