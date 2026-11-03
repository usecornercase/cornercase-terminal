use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::*;
use crate::test_util::TempDir;

fn with_file(dir: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, text).expect("write the file");
    path
}

fn skills(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("skills");
    fs::create_dir(&path).expect("the original folder");
    fs::write(path.join("one.md"), "one").expect("a file in it");
    path
}

fn lock_file(dir: &TempDir) -> impl Fn() -> File {
    let path = dir.path().join("server.lock");
    move || OpenOptions::new().create(true).write(true).truncate(false).open(&path).expect("open the lock")
}

fn is_link(link: &Path, original: &Path) -> bool {
    fs::symlink_metadata(link).is_ok_and(|meta| links_to(&meta, link, original))
}

mod linking {
    use super::*;

    #[test]
    fn a_folder_shows_what_its_original_holds() {
        let dir = TempDir::new();
        let original = skills(&dir);
        let link = dir.path().join("account-skills");
        link_to(&original, &link).expect("link the folder");
        fs::write(original.join("two.md"), "two").expect("a later file");
        assert_eq!(fs::read_to_string(link.join("two.md")).expect("read through the link"), "two");
        assert!(fs::symlink_metadata(&link).expect("the link").file_type().is_symlink());
        assert!(is_link(&link, &original));
    }

    #[test]
    fn a_file_shares_what_is_written_to_its_original() {
        let dir = TempDir::new();
        let original = with_file(&dir, "settings.json", "{}");
        let link = dir.path().join("account-settings.json");
        link_to(&original, &link).expect("link the file");
        fs::write(&original, r#"{"theme":"dark"}"#).expect("change the original");
        assert_eq!(fs::read_to_string(&link).expect("read through the link"), r#"{"theme":"dark"}"#);
        assert!(is_link(&link, &original));
    }

    #[test]
    fn a_real_file_in_its_place_is_not_a_link() {
        let dir = TempDir::new();
        let original = with_file(&dir, "settings.json", "{}");
        let replaced = with_file(&dir, "account-settings.json", "{}");
        assert!(!is_link(&replaced, &original));
    }

    #[test]
    fn what_is_already_there_is_never_replaced() {
        let dir = TempDir::new();
        let original = with_file(&dir, "CLAUDE.md", "shared");
        let mine = with_file(&dir, "account-CLAUDE.md", "mine");
        let refused = link_to(&original, &mine).expect_err("a file is there");
        assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&mine).expect("still mine"), "mine");
    }

    #[test]
    fn removing_the_folder_that_holds_links_keeps_what_they_lead_to() {
        let dir = TempDir::new();
        let original = skills(&dir);
        let settings = with_file(&dir, "settings.json", "{}");
        let account = dir.path().join("account");
        fs::create_dir(&account).expect("the account");
        link_to(&original, account.join("skills")).expect("link the folder");
        link_to(&settings, account.join("settings.json")).expect("link the file");
        fs::remove_dir_all(&account).expect("remove the account");
        assert_eq!(fs::read_to_string(original.join("one.md")).expect("still there"), "one");
        assert_eq!(fs::read_to_string(&settings).expect("still there"), "{}");
    }
}

mod identity {
    use super::*;

    #[test]
    fn two_names_of_one_file_are_the_same_file() {
        let dir = TempDir::new();
        let first = with_file(&dir, "a.jsonl", "line\n");
        let second = dir.path().join("b.jsonl");
        fs::hard_link(&first, &second).expect("a second name");
        assert!(stat(&first).expect("first").same_file(&stat(&second).expect("second")));
    }

    #[test]
    fn a_copy_is_another_file() {
        let dir = TempDir::new();
        let first = with_file(&dir, "a.jsonl", "line\n");
        let copy = dir.path().join("b.jsonl");
        fs::copy(&first, &copy).expect("a copy");
        assert!(!stat(&first).expect("first").same_file(&stat(&copy).expect("copy")));
    }

    #[test]
    fn an_open_file_is_the_one_at_its_path() {
        let dir = TempDir::new();
        let path = with_file(&dir, "server.log", "started\n");
        let open = File::open(&path).expect("open");
        let by_path = stat(&path).expect("by path");
        let by_handle = stat_file(&open).expect("by handle");
        assert!(by_path.same_file(&by_handle));
        assert_eq!(by_handle.meta.len(), 8);
    }

    #[test]
    fn a_file_renamed_over_another_keeps_its_identity_and_replaces_the_other() {
        let dir = TempDir::new();
        let path = with_file(&dir, "session.json", "old");
        let before = stat(&path).expect("before");
        let tmp = dir.path().join("session.json.tmp");
        let file = OpenOptions::new().write(true).create(true).truncate(true).open(&tmp).expect("tmp");
        let written = stat_file(&file).expect("the new file");
        drop(file);
        fs::rename(&tmp, &path).expect("rename");
        let after = stat(&path).expect("after");
        assert!(after.same_file(&written));
        assert!(!after.same_file(&before));
    }

    #[test]
    fn a_missing_file_has_none() {
        let dir = TempDir::new();
        assert_eq!(stat(&dir.path().join("missing")).expect_err("missing").kind(), io::ErrorKind::NotFound);
    }
}

mod locks {
    use super::*;

    #[test]
    fn an_exclusive_lock_keeps_every_other_out() {
        let dir = TempDir::new();
        let open = lock_file(&dir);
        let held = locked(open(), Lock::Exclusive).expect("lock").expect("free");
        assert!(locked(open(), Lock::Exclusive).expect("try").is_none());
        assert!(locked(open(), Lock::Shared).expect("try").is_none());
        drop(held);
    }

    #[test]
    fn shared_locks_go_together_and_keep_an_exclusive_one_out() {
        let dir = TempDir::new();
        let open = lock_file(&dir);
        let first = locked(open(), Lock::Shared).expect("lock").expect("free");
        let second = locked(open(), Lock::Shared).expect("lock").expect("shared");
        assert!(!try_lock(&open(), Lock::Exclusive).expect("try"));
        drop((first, second));
    }

    #[test]
    fn a_lock_goes_with_its_file() {
        let dir = TempDir::new();
        let open = lock_file(&dir);
        drop(locked(open(), Lock::Exclusive).expect("lock").expect("free"));
        assert!(try_lock(&open(), Lock::Exclusive).expect("try"));
    }
}

mod writing {
    use super::*;

    #[test]
    fn a_folder_of_ours_is_writable_and_the_check_leaves_nothing_behind() {
        let dir = TempDir::new();
        assert!(writable(dir.path()));
        assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 0);
    }

    #[test]
    fn a_missing_folder_is_not() {
        let dir = TempDir::new();
        assert!(!writable(&dir.path().join("missing")));
    }

    #[test]
    fn a_private_file_or_folder_can_always_be_made() {
        let dir = TempDir::new();
        let path = with_file(&dir, "secrets.json", "{}");
        let file = OpenOptions::new().write(true).mode(0o600).open(&path).expect("open");
        set_file_mode(&file, 0o600).expect("a private file");
        set_mode(dir.path(), 0o700).expect("a private folder");
        fs::write(&path, "{\"a\":1}").expect("still ours to write");
    }
}
