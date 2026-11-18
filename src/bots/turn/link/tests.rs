use std::time::{Duration, UNIX_EPOCH};

use super::*;
use crate::test_util::TempDir;

fn binary(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let folder = dir.join(name);
    fs::create_dir_all(&folder).expect("folder");
    let path = folder.join("cornercase");
    fs::write(&path, content).expect("binary");
    path
}

#[test]
fn a_new_folder_gets_only_a_link_that_leads_to_the_binary() {
    let dir = TempDir::new();
    let program = binary(dir.path(), "a", "one");
    let folder = dir.path().join("bin");
    ensure(&folder, &program).expect("ensure");
    let names: Vec<_> = fs::read_dir(&folder).expect("list").map(|e| e.expect("entry").file_name()).collect();
    assert_eq!(names, [std::ffi::OsString::from(NAME)]);
    assert_eq!(fs::read(folder.join(NAME)).expect("read"), b"one");
    #[cfg(unix)]
    assert_eq!(fs::read_link(folder.join(NAME)).expect("link"), program);
}

#[test]
fn a_link_to_another_binary_is_made_again() {
    let dir = TempDir::new();
    let (a, b) = (binary(dir.path(), "a", "one"), binary(dir.path(), "b", "second"));
    let folder = dir.path().join("bin");
    ensure(&folder, &a).expect("first");
    ensure(&folder, &b).expect("second");
    assert_eq!(fs::read(folder.join(NAME)).expect("read"), b"second");
    #[cfg(unix)]
    assert_eq!(fs::read_link(folder.join(NAME)).expect("link"), b);
}

#[test]
fn a_binary_replaced_in_place_is_what_the_link_leads_to() {
    let dir = TempDir::new();
    let program = binary(dir.path(), "a", "one");
    let folder = dir.path().join("bin");
    ensure(&folder, &program).expect("first");
    let next = program.with_file_name("cornercase.new");
    fs::write(&next, "newer").expect("write");
    fs::rename(&next, &program).expect("replace");
    ensure(&folder, &program).expect("second");
    assert_eq!(fs::read(folder.join(NAME)).expect("read"), b"newer");
}

#[test]
fn a_copy_is_made_again_only_when_the_binary_changes() {
    let dir = TempDir::new();
    let program = binary(dir.path(), "a", "one");
    let folder = dir.path().join("bin");
    let copy = |from: &Path, to: &Path| fs::copy(from, to).map(drop);
    ensure_through(&folder, &program, copy).expect("first");
    let id = stat(&folder.join(NAME)).expect("stat").id();
    ensure_through(&folder, &program, copy).expect("again");
    assert_eq!(stat(&folder.join(NAME)).expect("stat").id(), id);
    fs::write(&program, "newer").expect("write");
    let file = File::options().write(true).open(&program).expect("open");
    file.set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000)).expect("time");
    drop(file);
    ensure_through(&folder, &program, copy).expect("changed");
    assert_eq!(fs::read(folder.join(NAME)).expect("read"), b"newer");
}

#[test]
fn a_server_that_runs_from_the_link_leaves_it_as_it_is() {
    let dir = TempDir::new();
    let program = binary(dir.path(), "a", "one");
    let folder = dir.path().join("bin");
    ensure(&folder, &program).expect("first");
    ensure(&folder, &folder.join(NAME)).expect("from the link");
    assert_eq!(fs::read(folder.join(NAME)).expect("read"), b"one");
    #[cfg(unix)]
    assert_eq!(fs::read_link(folder.join(NAME)).expect("link"), program);
}

#[test]
fn a_folder_that_cannot_be_made_is_an_error() {
    let dir = TempDir::new();
    let program = binary(dir.path(), "a", "one");
    assert!(ensure(&program.join("bin"), &program).is_err());
}
