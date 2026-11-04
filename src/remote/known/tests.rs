use super::*;
use crate::test_util::TempDir;

fn kept_in(dir: &TempDir) -> PathBuf {
    dir.path().join("remote-shells.json")
}

#[test]
fn a_windows_shell_is_remembered_for_the_next_command() {
    let dir = TempDir::new();
    let mut known = Known::at(kept_in(&dir));

    known.learn("winbox", RemoteShell::Cmd);

    assert_eq!(Known::at(kept_in(&dir)).of("winbox"), Some(RemoteShell::Cmd));
}

#[test]
fn sh_is_what_cornercase_tries_anyway_so_it_is_never_written() {
    let dir = TempDir::new();
    let mut known = Known::at(kept_in(&dir));

    known.learn("devbox", RemoteShell::Posix);

    assert!(!kept_in(&dir).exists());
}

#[test]
fn a_machine_that_answers_with_sh_again_is_forgotten() {
    let dir = TempDir::new();
    let mut known = Known::at(kept_in(&dir));
    known.learn("winbox", RemoteShell::PowerShell);

    known.learn("winbox", RemoteShell::Posix);

    assert_eq!(Known::at(kept_in(&dir)).of("winbox"), None);
}

#[test]
fn what_another_command_learned_meanwhile_is_kept() {
    let dir = TempDir::new();
    let mut first = Known::at(kept_in(&dir));
    let mut second = Known::at(kept_in(&dir));
    first.learn("winbox", RemoteShell::Cmd);

    second.learn("other", RemoteShell::PowerShell);

    let read = Known::at(kept_in(&dir));
    assert_eq!((read.of("winbox"), read.of("other")), (Some(RemoteShell::Cmd), Some(RemoteShell::PowerShell)));
}

#[test]
fn an_unreadable_file_knows_nothing() {
    let dir = TempDir::new();
    std::fs::write(kept_in(&dir), "{").expect("a broken file");

    assert_eq!(Known::at(kept_in(&dir)).of("winbox"), None);
}
