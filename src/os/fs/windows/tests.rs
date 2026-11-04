use std::fs;
use std::io::ErrorKind;

use super::*;
use crate::test_util::TempDir;

fn names(dir: &TempDir) -> Vec<String> {
    let entries = fs::read_dir(dir.path()).expect("list the folder");
    let mut names: Vec<String> =
        entries.map(|entry| entry.expect("an entry").file_name().to_string_lossy().into()).collect();
    names.sort();
    names
}

mod junctions {
    use super::*;

    #[test]
    fn a_verbatim_path_becomes_a_plain_one() {
        let dir = TempDir::new();
        let skills = dir.path().join("skills");
        fs::create_dir(&skills).expect("a folder");
        let text = skills.to_str().expect("a UTF-8 path");
        let plain = text.strip_prefix(r"\\?\").unwrap_or(text);
        let verbatim = PathBuf::from(format!(r"\\?\{plain}"));
        assert_eq!(on_a_local_disk(&verbatim).expect("a local disk"), PathBuf::from(plain));
    }

    #[test]
    fn a_relative_path_is_made_absolute_unless_the_current_folder_is_not_on_a_drive() {
        let here = fs::canonicalize(".").expect("the current folder");
        match on_a_local_disk(Path::new("src")) {
            Ok(local) => {
                assert!(local.is_absolute() && on_a_drive(&local).is_some());
                assert_eq!(fs::canonicalize(local).expect("canonical"), here.join("src"));
            }
            Err(refused) => {
                assert_eq!(refused.kind(), ErrorKind::Unsupported);
                assert!(on_a_drive(&here).is_none(), "refused although the current folder is on a drive");
            }
        }
    }

    #[test]
    fn a_missing_folder_is_not_linked() {
        let dir = TempDir::new();
        let missing = on_a_local_disk(&dir.path().join("missing")).expect_err("missing");
        assert_eq!(missing.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn a_network_folder_is_refused() {
        let refused = on_a_local_disk(Path::new(r"\\server\share\skills")).expect_err("a network folder");
        assert_eq!(refused.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn a_folder_on_a_drive_that_resolves_to_a_share_is_refused() {
        let share = |_: &Path| Ok(PathBuf::from(r"\\?\UNC\server\share\skills"));
        let refused = on_a_local_disk_through(Path::new(r"C:\skills"), share).expect_err("a share");
        assert_eq!(refused.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn the_junction_leads_to_the_resolved_folder() {
        let resolved = |_: &Path| Ok(PathBuf::from(r"\\?\D:\real\skills"));
        let local = on_a_local_disk_through(Path::new(r"C:\skills"), resolved).expect("a local disk");
        assert_eq!(local, PathBuf::from(r"D:\real\skills"));
    }

    #[test]
    fn a_folder_that_resolves_to_a_share_is_not_on_a_drive() {
        assert_eq!(on_a_drive(Path::new(r"\\?\UNC\server\share\skills")), None);
        assert_eq!(on_a_drive(Path::new(r"\\?\C:\Users\me")), Some(PathBuf::from(r"C:\Users\me")));
    }

    #[test]
    fn its_reparse_data_names_the_folder_twice() {
        let buffer = mount_point(Path::new(r"C:\a")).expect("a buffer");
        let units: Vec<u16> = buffer[4..].chunks(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        let names: Vec<u16> = r"\??\C:\a".encode_utf16().chain([0]).chain(r"C:\a".encode_utf16()).chain([0]).collect();
        assert_eq!(buffer[..4], MOUNT_POINT.to_le_bytes());
        assert_eq!(units[..6], [8 + 16 + 2 + 8 + 2, 0, 0, 16, 18, 8]);
        assert_eq!(units[6..], names);
    }

    #[test]
    fn it_leads_to_its_folder_and_goes_without_it() {
        let dir = TempDir::new();
        let original = dir.path().join("commands");
        fs::create_dir(&original).expect("the original folder");
        fs::write(original.join("review.md"), "review").expect("a file in it");
        let link = dir.path().join("linked");
        junction(&original, &link).expect("a junction");
        let target = fs::canonicalize(fs::read_link(&link).expect("where it leads")).expect("canonical");
        assert_eq!(target, fs::canonicalize(&original).expect("canonical"));
        fs::remove_dir(&link).expect("remove the junction");
        assert_eq!(fs::read_to_string(original.join("review.md")).expect("still there"), "review");
    }

    #[test]
    fn a_failed_one_leaves_no_folder_behind() {
        let dir = TempDir::new();
        let link = dir.path().join("linked");
        let refused = junction(Path::new(r"\\server\share\skills"), &link).expect_err("a network folder");
        assert_eq!(refused.kind(), ErrorKind::Unsupported);
        assert!(!link.exists());
    }
}

mod copies {
    use super::*;

    #[test]
    fn are_another_file_with_the_same_bytes() {
        let dir = TempDir::new();
        let original = dir.path().join("config.toml");
        fs::write(&original, "model = \"o3\"\n").expect("the original");
        let copy = dir.path().join("copy.toml");
        copy_new(&original, &copy).expect("a copy");
        assert_eq!(fs::read(&copy).expect("the copy"), fs::read(&original).expect("the original"));
        assert!(!is_shared(&copy, &original));
        assert_eq!(names(&dir), ["config.toml", "copy.toml"]);
    }

    #[test]
    fn never_replace_a_file_and_leave_no_temporary_one() {
        let dir = TempDir::new();
        let original = dir.path().join("config.toml");
        fs::write(&original, "shared").expect("the original");
        let mine = dir.path().join("mine.toml");
        fs::write(&mine, "mine").expect("mine");
        assert_eq!(copy_new(&original, &mine).expect_err("a file is there").kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&mine).expect("mine"), "mine");
        assert_eq!(names(&dir), ["config.toml", "mine.toml"]);
    }

    #[test]
    fn are_written_aside_and_only_then_take_the_name() {
        let dir = TempDir::new();
        let temp = dir.path().join(".partial");
        fs::write(&temp, "half").expect("a partial copy");
        let copy = dir.path().join("copy.toml");
        publish(&temp, &copy).expect("published");
        assert_eq!(
            (fs::read_to_string(&copy).expect("the copy"), names(&dir)),
            ("half".to_owned(), vec!["copy.toml".to_owned()])
        );
        fs::write(&temp, "again").expect("another partial copy");
        assert_eq!(publish(&temp, &copy).expect_err("the name is taken").kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&copy).expect("the copy"), "half");
    }

    #[test]
    fn of_a_missing_file_leave_nothing() {
        let dir = TempDir::new();
        let copy = dir.path().join("copy.toml");
        assert_eq!(copy_new(&dir.path().join("missing"), &copy).expect_err("missing").kind(), ErrorKind::NotFound);
        assert_eq!(names(&dir), Vec::<String>::new());
    }

    fn is_shared(link: &Path, original: &Path) -> bool {
        fs::symlink_metadata(link).is_ok_and(|meta| links_to(&meta, link, original))
    }
}

mod listing {
    use super::*;

    #[test]
    fn counts_the_length_in_blocks_and_the_last_write_as_both_times() {
        let dir = TempDir::new();
        let path = dir.path().join("big.bin");
        fs::write(&path, vec![0; 513]).expect("a file");
        let meta = fs::metadata(&path).expect("metadata");
        let written = meta.modified().expect("modified").duration_since(UNIX_EPOCH).expect("after 1970");
        assert_eq!((meta.blocks(), meta.nlink()), (2, 1));
        assert_eq!(
            (meta.mtime(), meta.mtime_nsec()),
            (i64::try_from(written.as_secs()).expect("secs"), i64::from(written.subsec_nanos()))
        );
        assert_eq!((meta.ctime(), meta.ctime_nsec()), (meta.mtime(), meta.mtime_nsec()));
    }

    #[test]
    fn what_it_cannot_know_never_passes_a_check() {
        let dir = TempDir::new();
        let path = dir.path().join("cornercase-plugin.toml");
        fs::write(&path, "").expect("a file");
        let meta = fs::metadata(&path).expect("metadata");
        assert_ne!(meta.ino(), fs::metadata(&path).expect("again").ino());
        assert_ne!(meta.gid(), gid());
    }
}

mod opening {
    use super::*;

    #[test]
    fn without_following_opens_the_link_itself() {
        let dir = TempDir::new();
        let original = dir.path().join("plugin");
        fs::create_dir(&original).expect("the original folder");
        let link = dir.path().join("linked");
        junction(&original, &link).expect("a junction");
        let opened = |flags: i32| {
            let mut options = OpenOptions::new();
            OpenOptionsExt::custom_flags(options.read(true), flags | FILE_FLAG_BACKUP_SEMANTICS.cast_signed());
            options.open(&link).and_then(|file| file.metadata()).expect("open")
        };
        assert!(opened(NOFOLLOW).file_type().is_symlink());
        assert!(opened(0).is_dir());
    }
}
