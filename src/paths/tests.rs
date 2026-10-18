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

type Vars = &'static [(&'static str, &'static str)];

fn vars(vars: Vars) -> impl Fn(&str) -> Option<OsString> {
    move |name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| OsString::from(value))
}

const KEY: Option<&str> = Some("ecddcea2");
const OTHER_KEY: Option<&str> = Some("24fcc040");
const HOME: (&str, &str) = ("HOME", "/home/me");
const CUSTOM: (&str, &str) = (SOCKET_ENV, "/s/dev.sock");
const RUNTIME: (&str, &str) = ("XDG_RUNTIME_DIR", "/run/user/501");

mod the_key_of_a_source_tree {
    use super::*;

    #[test]
    fn is_the_first_eight_hex_digits_of_the_sha256_of_its_path() {
        assert_eq!(dev_key("/home/me/src/cornercase"), "ecddcea2");
    }

    #[test]
    fn differs_for_another_worktree() {
        assert_eq!(dev_key("/home/me/src/cornercase-2"), "24fcc040");
    }
}

#[rstest::rstest]
#[case::release(None, &[HOME], "/home/me/.local/state/cornercase/server.sock")]
#[case::release_xdg(None, &[HOME, ("XDG_STATE_HOME", "/st")], "/st/cornercase/server.sock")]
#[case::runtime_and_temp_do_not_matter(None, &[HOME, RUNTIME, ("TMPDIR", "/t")], "/home/me/.local/state/cornercase/server.sock")]
#[case::debug(KEY, &[HOME, RUNTIME], "/home/me/.local/state/cornercase-dev/ecddcea2/server.sock")]
#[case::custom_in_release(None, &[HOME, CUSTOM, RUNTIME], "/s/dev.sock")]
#[case::custom_in_debug(KEY, &[HOME, CUSTOM, RUNTIME], "/s/dev.sock")]
fn sockets(#[case] dev: Option<&str>, #[case] env: Vars, #[case] expected: &str) {
    assert_eq!(socket_in(&vars(env), dev), PathBuf::from(expected));
}

#[rstest::rstest]
#[case::empty(&[("XDG_STATE_HOME", ""), HOME])]
#[case::relative(&[("XDG_STATE_HOME", "rel"), HOME])]
fn relative_or_empty_xdg_folders_are_ignored(#[case] env: Vars) {
    assert_eq!(socket_in(&vars(env), None), PathBuf::from("/home/me/.local/state/cornercase/server.sock"));
}

#[rstest::rstest]
#[case::runtime_set(&[RUNTIME], true, &["/run/user/501/cornercase/server.sock", "/t/cornercase-501/server.sock"])]
#[case::linux_guesses_the_runtime_folder(&[], true, &["/run/user/501/cornercase/server.sock", "/t/cornercase-501/server.sock"])]
#[case::macos_has_none(&[], false, &["/t/cornercase-501/server.sock"])]
fn earlier_sockets(#[case] env: Vars, #[case] linux: bool, #[case] expected: &[&str]) {
    let found = earlier_sockets_from(&vars(env), 501, Path::new("/t"), linux);
    assert_eq!(found, expected.iter().map(PathBuf::from).collect::<Vec<_>>());
}

#[rstest::rstest]
#[case::release(None, &[HOME], "/home/me/.config/cornercase/config.json")]
#[case::release_xdg(None, &[HOME, ("XDG_CONFIG_HOME", "/c")], "/c/cornercase/config.json")]
#[case::debug(KEY, &[HOME], "/home/me/.config/cornercase-dev/config.json")]
#[case::debug_xdg(KEY, &[HOME, ("XDG_CONFIG_HOME", "/c")], "/c/cornercase-dev/config.json")]
#[case::custom_in_debug(KEY, &[HOME, CUSTOM], "/s/config.json")]
fn configs(#[case] dev: Option<&str>, #[case] env: Vars, #[case] expected: &str) {
    assert_eq!(config_in(&vars(env), dev), PathBuf::from(expected));
}

#[rstest::rstest]
#[case::release(None, &[HOME], "/home/me/.local/state/cornercase/session.json")]
#[case::release_xdg(None, &[HOME, ("XDG_STATE_HOME", "/st")], "/st/cornercase/session.json")]
#[case::debug(KEY, &[HOME], "/home/me/.local/state/cornercase-dev/ecddcea2/session.json")]
#[case::debug_xdg(KEY, &[HOME, ("XDG_STATE_HOME", "/st")], "/st/cornercase-dev/ecddcea2/session.json")]
#[case::custom_in_debug(KEY, &[HOME, CUSTOM], "/s/dev.json")]
fn sessions(#[case] dev: Option<&str>, #[case] env: Vars, #[case] expected: &str) {
    assert_eq!(session_in(&vars(env), dev), PathBuf::from(expected));
}

#[rstest::rstest]
#[case::release(None, &[HOME], "/home/me/.local/state/cornercase/server.log")]
#[case::debug(KEY, &[HOME], "/home/me/.local/state/cornercase-dev/ecddcea2/server.log")]
#[case::custom_in_debug(KEY, &[HOME, CUSTOM], "/s/dev.log")]
fn logs(#[case] dev: Option<&str>, #[case] env: Vars, #[case] expected: &str) {
    assert_eq!(log_in(&vars(env), dev), PathBuf::from(expected));
}

#[test]
fn two_trees_share_the_config_and_nothing_else() {
    let env = vars(&[HOME, RUNTIME]);
    assert_eq!(config_in(&env, KEY), config_in(&env, OTHER_KEY));
    assert_ne!(session_in(&env, KEY), session_in(&env, OTHER_KEY));
    assert_ne!(socket_in(&env, KEY), socket_in(&env, OTHER_KEY));
}

#[test]
fn the_source_file_sits_in_the_state_folder() {
    assert_eq!(
        source(Path::new("/st/cornercase-dev/ecddcea2/session.json")),
        PathBuf::from("/st/cornercase-dev/ecddcea2/source")
    );
}
