use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::git::clone::files::MAX_TOTAL_BYTES;
use crate::test_util::{TempDir, git_repo};

fn repo(ignore: &str) -> TempDir {
    git_repo(&[(".gitignore", ignore), ("config/app.json", "{}")])
}

fn write(repo: &TempDir, path: &str, contents: &[u8]) {
    let path = repo.path().join(path);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("create folder");
    std::fs::write(path, contents).expect("write file");
}

fn paths(repo: &TempDir) -> Vec<String> {
    list(repo.path()).expect("list").files.into_iter().map(|file| file.path.display().to_string()).collect()
}

fn file(path: &str, size: u64) -> IgnoredFile {
    IgnoredFile { path: PathBuf::from(path), size, mode: 0o644, too_big: false }
}

mod list {
    use super::*;

    #[test]
    fn offers_the_ignored_files_in_git_order() {
        let repo = repo(".env\nconfig/secrets.dev.json\n");
        write(&repo, "config/secrets.dev.json", b"{}");
        write(&repo, ".env", b"KEY=1");

        assert_eq!(paths(&repo), [".env", "config/secrets.dev.json"]);
    }

    #[test]
    fn never_offers_nor_walks_an_ignored_folder() {
        let repo = repo("node_modules/\ntarget\n.env\n");
        write(&repo, "node_modules/left-pad/index.js", b"x");
        write(&repo, "target/debug/app", b"x");
        write(&repo, ".env", b"KEY=1");

        assert_eq!(paths(&repo), [".env"]);
    }

    #[test]
    fn offers_an_ignored_file_in_a_folder_git_does_not_track() {
        let repo = repo("*.secret\n");
        write(&repo, "notes/token.secret", b"x");

        assert_eq!(paths(&repo), ["notes/token.secret"]);
    }

    #[test]
    fn leaves_out_a_symlink() {
        let repo = repo(".env\n");
        let outside = TempDir::new();
        write(&outside, "real.env", b"KEY=1");
        std::os::unix::fs::symlink(outside.path().join("real.env"), repo.path().join(".env")).expect("symlink");

        assert_eq!(paths(&repo), Vec::<String>::new());
    }

    #[test]
    fn is_empty_without_ignored_files() {
        let repo = repo(".env\n");
        write(&repo, "untracked.txt", b"x");

        assert_eq!(list(repo.path()).expect("list"), Listing::default());
    }

    #[test]
    fn gives_each_file_its_size_and_permissions() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");
        let path = repo.path().join(".env");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");

        assert_eq!(list(repo.path()).expect("list").files, [IgnoredFile { mode: 0o600, ..file(".env", 5) }]);
    }

    #[test]
    fn marks_a_file_past_the_size_cap_too_big() {
        let repo = repo("*.bin\n");
        let cap = usize::try_from(MAX_FILE_BYTES).expect("a small cap");
        write(&repo, "big.bin", &vec![0; cap + 1]);
        write(&repo, "small.bin", &vec![0; cap]);

        let files = list(repo.path()).expect("list").files;

        assert_eq!(
            files.iter().map(|file| (file.size, file.too_big)).collect::<Vec<_>>(),
            [(MAX_FILE_BYTES + 1, true), (MAX_FILE_BYTES, false)]
        );
    }

    #[test]
    fn stops_at_the_entry_cap_and_counts_what_it_left_out() {
        let repo = repo("*.log\n");
        for n in 0..MAX_FILES + 2 {
            write(&repo, &format!("logs/{n:03}.log"), b"x");
        }

        let listing = list(repo.path()).expect("list");

        let last = listing.files.last().map(|file| file.path.display().to_string());
        assert_eq!((listing.files.len(), last.as_deref(), listing.left_out), (MAX_FILES, Some("logs/199.log"), 2));
    }

    #[test]
    fn looks_at_no_entry_past_the_cap_and_only_counts_it() {
        let repo = repo("*.log\n");
        for n in 0..MAX_FILES {
            write(&repo, &format!("logs/{n:03}.log"), b"x");
        }
        let outside = TempDir::new();
        write(&outside, "real.log", b"x");
        std::os::unix::fs::symlink(outside.path().join("real.log"), repo.path().join("logs/zz.log")).expect("symlink");

        let listing = list(repo.path()).expect("list");

        assert_eq!((listing.files.len(), listing.left_out), (MAX_FILES, 1));
    }

    #[test]
    fn leaves_the_total_to_the_ticks() {
        let repo = repo("*.bin\n");
        let cap = usize::try_from(MAX_FILE_BYTES).expect("a small cap");
        for n in 0..=MAX_TOTAL_BYTES / MAX_FILE_BYTES {
            write(&repo, &format!("{n}.bin"), &vec![0; cap]);
        }

        let files = list(repo.path()).expect("list").files;

        assert!(files.iter().all(|file| !file.too_big), "{files:?}");
    }

    #[test]
    fn names_a_file_of_a_project_in_a_subfolder_from_the_repository_s_root() {
        let repo = repo("web/.env\n");
        write(&repo, "web/.env", b"KEY=1");
        write(&repo, "web/index.html", b"x");

        let listing = list(&repo.path().join("web")).expect("list");

        let files: Vec<(String, u64)> =
            listing.files.iter().map(|file| (file.path.display().to_string(), file.size)).collect();
        assert_eq!(files, [("web/.env".to_string(), 5)]);
        assert_eq!(toplevel(&repo.path().join("web")).expect("the top"), repo.path());
    }

    #[test]
    fn fails_outside_a_repository() {
        let folder = TempDir::new();

        assert!(list(folder.path()).is_err());
    }
}

mod read {
    use super::*;

    #[test]
    fn reads_a_listed_file() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");

        assert_eq!(read(repo.path(), &file(".env", 5)).expect("read"), b"KEY=1");
    }

    fn linked(repo: &Path, target: &Path) -> PathBuf {
        std::os::unix::fs::symlink(target, repo.join(".env")).expect("symlink");
        PathBuf::from(".env")
    }

    fn up_from(repo: &Path, target: &Path) -> PathBuf {
        let up = repo.components().skip(1).map(|_| Component::ParentDir);
        up.map(Component::as_os_str).chain(target.components().skip(1).map(Component::as_os_str)).collect()
    }

    fn absolute(_: &Path, target: &Path) -> PathBuf {
        target.to_path_buf()
    }

    fn in_a_linked_folder(repo: &Path, target: &Path) -> PathBuf {
        std::os::unix::fs::symlink(target.parent().expect("a folder"), repo.join("secrets")).expect("symlink");
        Path::new("secrets").join(target.file_name().expect("a name"))
    }

    #[rstest::rstest]
    #[case::outside(linked)]
    #[case::a_parent(up_from)]
    #[case::absolute(absolute)]
    #[case::through_a_folder(in_a_linked_folder)]
    fn refuses_a_symlink_or_a_path_out_of_the_project(#[case] aim: fn(&Path, &Path) -> PathBuf) {
        let repo = repo(".env\n");
        let outside = TempDir::new();
        write(&outside, "real.env", b"KEY=1");
        let path = aim(repo.path(), &outside.path().join("real.env"));
        assert_eq!(std::fs::read(repo.path().join(&path)).expect("the file is there"), b"KEY=1");

        let ignored = IgnoredFile { path, size: 5, mode: 0o644, too_big: false };
        assert!(read(repo.path(), &ignored).is_err());
    }

    #[test]
    fn refuses_a_file_that_grew_past_the_limit() {
        let repo = repo(".env\n");
        write(&repo, ".env", &vec![b'x'; usize::try_from(MAX_FILE_BYTES).expect("a size") + 1]);

        let why = read(repo.path(), &file(".env", 5)).expect_err("too big").to_string();

        assert!(why.contains(".env grew past"), "{why}");
    }

    #[test]
    fn refuses_a_fifo_without_waiting_for_a_writer() {
        let repo = repo(".env\n");
        let fifo = repo.path().join(".env");
        let made = std::process::Command::new("mkfifo").arg(&fifo).status().expect("mkfifo");
        assert!(made.success());

        let root = repo.path().to_path_buf();
        let (told, heard) = std::sync::mpsc::channel();
        std::thread::spawn(move || told.send(read(&root, &file(".env", 0)).map_err(|error| error.to_string())));

        let read = heard.recv_timeout(std::time::Duration::from_secs(10)).expect("read waits for a writer on the fifo");
        let why = read.expect_err("a fifo");

        assert!(why.contains("not a regular file"), "{why}");
    }
}
