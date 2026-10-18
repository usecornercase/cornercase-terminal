use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::test_util::TempDir;

fn listening(dir: &Path, name: &str) -> (PathBuf, UnixListener) {
    fs::create_dir_all(dir).expect("folder");
    fs::set_permissions(dir, Permissions::from_mode(0o700)).expect("chmod");
    let path = dir.join(name);
    let listener = UnixListener::bind(&path).expect("bind");
    (path, listener)
}

#[test]
fn the_primary_socket_wins_when_it_answers() {
    let tmp = TempDir::new();
    let (primary, _a) = listening(&tmp.path().join("a"), "server.sock");
    let (earlier, _b) = listening(&tmp.path().join("b"), "server.sock");

    let found = find_among(&primary, &[earlier]).expect("no error").expect("found");

    assert_eq!(found.socket, primary);
}

#[test]
fn an_earlier_socket_is_used_when_the_primary_does_not_answer() {
    let tmp = TempDir::new();
    let primary = tmp.path().join("a").join("server.sock");
    let (earlier, _b) = listening(&tmp.path().join("b"), "server.sock");

    let found = find_among(&primary, std::slice::from_ref(&earlier)).expect("no error").expect("found");

    assert_eq!(found.socket, earlier);
}

#[test]
fn an_earlier_socket_in_an_unsafe_folder_is_skipped() {
    let tmp = TempDir::new();
    let primary = tmp.path().join("a").join("server.sock");
    let (earlier, _b) = listening(&tmp.path().join("b"), "server.sock");
    fs::set_permissions(earlier.parent().expect("folder"), Permissions::from_mode(0o777)).expect("chmod");

    assert!(find_among(&primary, &[earlier]).expect("no error").is_none());
}

#[test]
fn a_stale_socket_file_is_not_a_server() {
    let tmp = TempDir::new();
    let (stale, listener) = listening(&tmp.path().join("a"), "server.sock");
    drop(listener);

    assert!(find_among(&stale, &[]).expect("no error").is_none());
}

#[test]
fn listening_is_tried_without_leaving_anything_behind() {
    let tmp = TempDir::new();
    let socket = tmp.path().join("server.sock");

    can_listen(&socket, Folder::Given).expect("can listen");

    assert!(!socket.exists());
}

#[test]
fn a_stale_socket_file_is_removed_by_the_listen_check() {
    let tmp = TempDir::new();
    let (stale, listener) = listening(tmp.path(), "server.sock");
    drop(listener);

    can_listen(&stale, Folder::Given).expect("can listen");

    assert!(!stale.exists());
}

#[test]
fn a_held_lock_means_a_server_is_starting_so_nothing_is_touched() {
    let tmp = TempDir::new();
    let (socket, _listener) = listening(tmp.path(), "server.sock");
    let _lock = protocol::try_lock(&socket).expect("lock").expect("free");

    can_listen(&socket, Folder::Given).expect("skipped");

    assert!(socket.exists());
}

#[test]
fn a_folder_that_cannot_be_written_names_the_socket_setting() {
    if protocol::own_uid() == 0 {
        return;
    }
    let tmp = TempDir::new();
    let dir = tmp.path().join("ro");
    fs::create_dir(&dir).expect("folder");
    fs::set_permissions(&dir, Permissions::from_mode(0o500)).expect("chmod");

    let error = can_listen(&dir.join("server.sock"), Folder::Own).expect_err("cannot listen");

    assert!(matches!(error, Error::Listen { custom: false, .. }));
    assert!(error.to_string().contains("CORNERCASE_SOCKET"));
    fs::set_permissions(&dir, Permissions::from_mode(0o700)).expect("chmod back");
}
