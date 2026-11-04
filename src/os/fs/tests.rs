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

mod canonical_paths {
    use super::*;

    fn inner(dir: &TempDir) -> PathBuf {
        let inner = dir.path().join("inner");
        fs::create_dir(&inner).expect("a folder");
        inner
    }

    #[test]
    fn a_folder_reached_the_long_way_is_canonical() {
        let dir = TempDir::new();
        let inner = inner(&dir);
        let roundabout = inner.join("..").join("inner");
        assert_eq!(canonicalize(&roundabout).expect("canonical"), inner);
        assert_eq!(canonical(&roundabout), inner);
    }

    #[test]
    fn a_missing_folder_is_kept_as_given() {
        let dir = TempDir::new();
        let missing = dir.path().join("missing");
        assert_eq!(canonicalize(&missing).expect_err("missing").kind(), io::ErrorKind::NotFound);
        assert_eq!(canonical(&missing), missing);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_is_what_std_makes_of_it() {
        let dir = TempDir::new();
        let link = dir.path().join("link");
        link_to(inner(&dir), &link).expect("a link");
        assert_eq!(canonicalize(&link).expect("canonical"), fs::canonicalize(&link).expect("std"));
    }

    #[cfg(not(unix))]
    #[test]
    fn a_folder_comes_back_without_the_verbatim_prefix() {
        let dir = TempDir::new();
        let inner = inner(&dir);
        assert!(fs::canonicalize(&inner).expect("std").to_string_lossy().starts_with(r"\\?\"));
        assert!(!inner.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(canonicalize(&inner).expect("canonical"), inner);
        assert_eq!(canonicalize(crate::test_util::verbatim(&inner)).expect("canonical"), inner);
    }

    #[cfg(not(unix))]
    #[test]
    fn a_missing_verbatim_folder_is_kept_without_the_prefix() {
        let dir = TempDir::new();
        let missing = dir.path().join("missing");
        assert_eq!(canonical(crate::test_util::verbatim(&missing)), missing);
    }
}

mod paths_beyond_the_drives {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::a_share(r"\\host\share\ding.wav", true, false)]
    #[case::a_share_with_slashes("//host/share/ding.wav", true, false)]
    #[case::mixed_slashes(r"/\host\share\ding.wav", true, false)]
    #[case::a_verbatim_share(r"\\?\UNC\host\share\ding.wav", true, false)]
    #[case::a_volume(r"\\?\Volume{0b1c}\ding.wav", true, false)]
    #[case::a_device(r"\\.\C:\ding.wav", true, false)]
    #[case::a_device_with_slashes("//?/C:/ding.wav", true, false)]
    #[case::a_verbatim_drive(r"\\?\C:\ding.wav", false, true)]
    #[case::a_verbatim_name_with_a_slash(r"\\?\C:/ding.wav", true, false)]
    #[case::a_drive(r"C:\Users\ana\ding.wav", false, true)]
    #[case::a_drive_with_slashes("d:/sounds/ding.wav", false, true)]
    #[case::a_drive_without_its_root("C:ding.wav", false, false)]
    #[case::the_drive_s_root(r"\ding.wav", false, false)]
    #[case::unix("/usr/share/sounds/ding.wav", false, false)]
    #[case::home("~/ding.wav", false, false)]
    #[case::relative("ding.wav", false, false)]
    fn are_told_apart(#[case] path: &str, #[case] beyond: bool, #[case] drive: bool) {
        assert_eq!((unc_or_device(path), drive_path(path)), (beyond, drive));
    }

    #[test]
    fn reach_the_network_only_on_windows() {
        assert_eq!(reaches_network(r"\\host\share\ding.wav"), cfg!(windows));
        assert!(!reaches_network(r"C:\ding.wav"));
    }
}

mod links_to_the_network {
    use super::*;

    type Walked = (io::Result<Option<PathBuf>>, Vec<PathBuf>);

    fn walk(path: &str, links: &[(&str, &str)]) -> Walked {
        let mut read = Vec::new();
        let found = network_link_through(Path::new(path), |next: &Path| {
            read.push(next.to_path_buf());
            Ok(links.iter().find(|(link, _)| Path::new(link) == next).map(|(_, target)| PathBuf::from(target)))
        });
        (found, read)
    }

    fn paths(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_folder_without_links_leads_nowhere_else() {
        let (found, read) = walk("/code/tools", &[]);
        assert_eq!(found.expect("walked"), None);
        assert_eq!(read, paths(&["/code", "/code/tools"]));
    }

    #[test]
    fn a_link_above_the_folder_is_found_and_nothing_under_it_is_read() {
        let (found, read) = walk("/code/shared/tools", &[("/code/shared", r"\\host\share")]);
        assert_eq!(found.expect("walked"), Some(PathBuf::from(r"\\host\share")));
        assert_eq!(read, paths(&["/code", "/code/shared"]));
    }

    #[test]
    fn the_folder_itself_as_a_link_is_found() {
        let (found, _) = walk("/code/tools", &[("/code/tools", "//host/share/tools")]);
        assert_eq!(found.expect("walked"), Some(PathBuf::from("//host/share/tools")));
    }

    #[test]
    fn a_relative_link_is_walked_from_its_folder_into_the_next_link() {
        let links = [("/code/vendor", "../srv"), ("/srv/tools", r"\\?\UNC\host\share\tools")];
        let (found, read) = walk("/code/vendor/tools/notes", &links);
        assert_eq!(found.expect("walked"), Some(PathBuf::from(r"\\?\UNC\host\share\tools")));
        assert_eq!(read, paths(&["/code", "/code/vendor", "/srv", "/srv/tools"]));
    }

    #[test]
    fn a_link_to_a_local_folder_is_walked_on() {
        let (found, read) = walk("/code/vendor/tools", &[("/code/vendor", "/srv/plugins")]);
        assert_eq!(found.expect("walked"), None);
        assert_eq!(read, paths(&["/code", "/code/vendor", "/srv", "/srv/plugins", "/srv/plugins/tools"]));
    }

    #[test]
    fn a_parent_folder_goes_back_by_name_as_windows_reads_it() {
        let (found, read) = walk("/code/../srv/tools", &[]);
        assert_eq!(found.expect("walked"), None);
        assert_eq!(read, paths(&["/code", "/srv", "/srv/tools"]));
    }

    #[test]
    fn links_in_a_loop_are_an_error() {
        let (found, read) = walk("/a/tools", &[("/a", "/b"), ("/b", "/a")]);
        assert!(found.is_err());
        assert_eq!(read.len(), MAX_LINKS + 1);
    }

    #[test]
    fn a_link_that_cannot_be_read_is_an_error() {
        let found = network_link_through(Path::new("/code/tools"), |_: &Path| Err(io::Error::other("unreadable")));
        assert_eq!(found.expect_err("unreadable").to_string(), "unreadable");
    }

    #[cfg(unix)]
    #[test]
    fn on_unix_nothing_is_on_the_network() {
        let dir = TempDir::new();
        let link = dir.path().join("shared");
        link_to("//host/share", &link).expect("a link");
        assert_eq!(network_link(&link.join("tools")).expect("walked"), None);
    }

    #[cfg(windows)]
    mod on_windows {
        use super::*;

        const PRIVILEGE_NOT_HELD: i32 = 1314;

        fn reached(path: &Path) -> Option<String> {
            network_link(path).expect("walked").map(|share| share.to_string_lossy().into_owned())
        }

        #[test]
        fn a_junction_to_a_folder_on_the_disk_leads_nowhere_else() {
            let dir = TempDir::new();
            let real = dir.path().join("real");
            fs::create_dir_all(real.join("tools")).expect("a folder");
            link_to(&real, dir.path().join("linked")).expect("a junction");
            assert_eq!(reached(&dir.path().join("linked").join("tools")), None);
        }

        #[test]
        fn a_verbatim_folder_on_the_disk_leads_nowhere_else() {
            let dir = TempDir::new();
            fs::create_dir_all(dir.path().join("tools")).expect("a folder");
            assert_eq!(reached(&crate::test_util::verbatim(&dir.path().join("tools"))), None);
        }

        #[test]
        fn a_link_to_a_verbatim_folder_on_the_disk_is_walked_on() {
            let (found, _) = walk(r"C:\code\vendor\tools", &[(r"C:\code\vendor", r"\\?\D:\srv")]);
            assert_eq!(found.expect("walked"), None);
        }

        #[test]
        fn a_junction_written_to_lead_to_a_share_is_found_without_following_it() {
            let dir = TempDir::new();
            let link = dir.path().join("shared");
            make_junction(&link, Path::new(r"UNC\cornercase.invalid\share")).expect("a junction");
            let share = reached(&link.join("tools")).expect("on the network");
            assert!(unc_or_device(&share) && share.contains("cornercase.invalid"), "{share}");
        }

        #[test]
        fn a_symbolic_link_to_a_share_is_found_without_following_it() {
            let dir = TempDir::new();
            let link = dir.path().join("shared");
            match std::os::windows::fs::symlink_dir(r"\\cornercase.invalid\share", &link) {
                Err(e) if e.raw_os_error() == Some(PRIVILEGE_NOT_HELD) => {
                    eprintln!("skipped: a symbolic link needs Developer Mode or an administrator ({e})");
                }
                made => {
                    made.expect("a symbolic link");
                    let share = reached(&link.join("tools")).expect("on the network");
                    assert!(unc_or_device(&share) && share.contains("cornercase.invalid"), "{share}");
                }
            }
        }
    }
}
