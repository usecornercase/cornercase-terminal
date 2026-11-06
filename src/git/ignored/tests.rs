#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::git::clone::files::MAX_TOTAL_BYTES;
use crate::test_util::{TempDir, git, git_repo};

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

fn root(repo: &Path) -> Root {
    list(repo).expect("list").root
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

        let listing = list(repo.path()).expect("list");

        assert_eq!((listing.files, listing.left_out), (Vec::new(), 0));
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

    #[cfg(target_os = "linux")]
    #[test]
    fn leaves_out_a_name_a_request_could_not_carry() {
        let repo = repo("*.env\n");
        write(&repo, "plain.env", b"KEY=1");
        let name = <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"\xff.env");
        std::fs::write(repo.path().join(name), "KEY=2").expect("write file");

        assert_eq!(paths(&repo), ["plain.env"]);
    }
}

mod picked {
    use super::*;

    fn asked(repo: &Path, paths: &[&str]) -> Vec<IgnoredFile> {
        let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        picked(repo, &paths).expect("picked").files
    }

    fn refused(repo: &Path, path: &str) -> Error {
        picked(repo, &[PathBuf::from(path)]).expect_err("refused")
    }

    #[test]
    fn gives_the_files_asked_for_with_their_size_and_permissions() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");
        std::fs::set_permissions(repo.path().join(".env"), std::fs::Permissions::from_mode(0o600)).expect("chmod");

        assert_eq!(asked(repo.path(), &[".env"]), [IgnoredFile { mode: 0o600, ..file(".env", 5) }]);
    }

    #[test]
    fn reads_the_paths_from_the_repository_s_root_whatever_the_folder() {
        let repo = repo("web/.env\n");
        write(&repo, "web/.env", b"KEY=1");

        let inside = asked(&repo.path().join("web"), &["web/.env"]);

        assert_eq!(inside.iter().map(|file| file.path.clone()).collect::<Vec<_>>(), [PathBuf::from("web/.env")]);
    }

    #[test]
    fn takes_a_file_git_does_not_ignore_for_the_clone_to_judge() {
        let repo = repo(".env\n");

        assert_eq!(asked(repo.path(), &["config/app.json"]).len(), 1);
    }

    #[test]
    fn reads_a_file_named_twice_once_under_its_plain_path() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");

        let files = asked(repo.path(), &[".env", "./.env"]);

        assert_eq!(files.iter().map(|file| file.path.clone()).collect::<Vec<_>>(), [PathBuf::from(".env")]);
    }

    #[test]
    fn hands_the_dialog_s_reading_a_root_that_opens_the_files() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");

        let listing = picked(repo.path(), &[PathBuf::from("./.env")]).expect("picked");

        assert_eq!(read(&listing.root, &listing.files[0]).expect("read").0, b"KEY=1");
    }

    #[rstest::rstest]
    #[case::going_up("../outside.env", "cannot copy `../outside.env`: it goes up with `..`")]
    #[case::going_up_inside("config/../../outside.env", "cannot copy `config/../../outside.env`: it goes up with `..`")]
    #[case::absolute("/etc/passwd", "cannot copy `/etc/passwd`: it is not relative to the project")]
    #[case::into_git(".git/config", "cannot copy `.git/config`: it goes into `.git`")]
    #[case::into_git_in_capitals(".GIT/config", "cannot copy `.GIT/config`: it goes into `.git`")]
    #[case::the_folder_itself(".", "cannot copy `.`: its path is empty")]
    fn calls_a_path_that_cannot_be_written_wrong_usage(#[case] path: &str, #[case] said: &str) {
        let repo = repo(".env\n");

        let refused = refused(repo.path(), path);

        assert!(matches!(refused, Error::WrongUsage(_)), "{refused:?}");
        assert_eq!(refused.to_string(), said);
    }

    fn a_missing_file(_: &Path) {}

    fn a_folder(repo: &Path) {
        std::fs::create_dir(repo.join(".env")).expect("a folder");
    }

    fn a_link(repo: &Path) {
        std::os::unix::fs::symlink("config/app.json", repo.join(".env")).expect("symlink");
    }

    fn a_fifo(repo: &Path) {
        assert!(std::process::Command::new("mkfifo").arg(repo.join(".env")).status().expect("mkfifo").success());
    }

    fn a_big_file(repo: &Path) {
        let size = usize::try_from(MAX_FILE_BYTES).expect("a small cap") + 1;
        std::fs::write(repo.join(".env"), vec![0; size]).expect("write file");
    }

    #[rstest::rstest]
    #[case::missing(a_missing_file, "it is not in `")]
    #[case::folder(a_folder, "it is a folder")]
    #[case::link(a_link, "it is a link")]
    #[case::fifo(a_fifo, "it is not a regular file")]
    #[case::too_big(a_big_file, "it is over the 1 MB a file can be")]
    fn says_what_is_wrong_with_what_is_on_the_disk(#[case] put: fn(&Path), #[case] said: &str) {
        let repo = repo(".env\n");
        put(repo.path());

        let refused = refused(repo.path(), ".env");

        assert!(matches!(refused, Error::Control(_)), "{refused:?}");
        assert!(refused.to_string().starts_with(&format!("cannot copy `.env`: {said}")), "{refused}");
    }

    #[test]
    fn takes_a_file_at_the_size_cap() {
        let repo = repo("*.bin\n");
        write(&repo, "full.bin", &vec![0; usize::try_from(MAX_FILE_BYTES).expect("a small cap")]);

        assert_eq!(asked(repo.path(), &["full.bin"]).len(), 1);
    }

    #[test]
    fn says_where_it_looked_for_a_file_in_a_missing_folder_or_under_a_file() {
        let repo = repo(".env\n");
        let looked = format!("it is not in `{}`, the repository's root", repo.path().display());

        for path in ["no/such/.env", "config/app.json/.env"] {
            let why = refused(repo.path(), path).to_string();

            assert_eq!(why, format!("cannot copy `{path}`: {looked}"));
        }
    }

    #[test]
    fn refuses_a_file_through_a_linked_folder_and_names_the_link() {
        let repo = repo(".env\n");
        let outside = TempDir::new();
        write(&outside, "deep/real.env", b"KEY=1");
        std::os::unix::fs::symlink(outside.path(), repo.path().join("secrets")).expect("symlink");

        let why = refused(repo.path(), "secrets/deep/real.env").to_string();

        assert_eq!(why, "cannot copy `secrets/deep/real.env`: `secrets` on its way is a link");
    }

    #[test]
    fn refuses_a_name_a_request_could_not_carry_before_it_looks() {
        let repo = repo(".env\n");
        let name = <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"\xff.env");

        let refused = picked(repo.path(), &[PathBuf::from(name)]).expect_err("not UTF-8");

        assert!(matches!(&refused, Error::Control(why) if why.ends_with(NOT_UTF8)), "{refused:?}");
    }

    #[test]
    fn refuses_more_files_than_a_clone_takes() {
        let repo = repo("*.log\n");
        let paths: Vec<PathBuf> = (0..=MAX_FILES).map(|n| PathBuf::from(format!("{n}.log"))).collect();
        for path in &paths {
            std::fs::write(repo.path().join(path), "x").expect("write file");
        }

        let refused = picked(repo.path(), &paths).expect_err("too many");

        assert_eq!(refused.to_string(), "the files are over the 200 a clone takes");
        assert_eq!(picked(repo.path(), &paths[1..]).expect("as many as it takes").files.len(), MAX_FILES);
    }

    #[test]
    fn fails_outside_a_repository() {
        let folder = TempDir::new();

        assert!(matches!(refused(folder.path(), ".env"), Error::Git(_)));
    }
}

mod read {
    use super::*;

    #[test]
    fn reads_a_listed_file() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");

        assert_eq!(read(&root(repo.path()), &file(".env", 5)).expect("read").0, b"KEY=1");
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
        assert!(read(&root(repo.path()), &ignored).is_err());
    }

    #[test]
    fn names_a_file_it_cannot_open_and_keeps_the_kind_of_failure() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");
        let listing = list(repo.path()).expect("list");
        std::fs::remove_file(repo.path().join(".env")).expect("remove the file");

        let Error::Io(why) = read(&listing.root, &listing.files[0]).expect_err("gone") else {
            panic!("not an io error")
        };

        assert_eq!(why.kind(), io::ErrorKind::NotFound);
        assert!(why.to_string().starts_with(".env: "), "{why}");
    }

    #[test]
    fn refuses_a_file_that_grew_past_the_limit() {
        let repo = repo(".env\n");
        write(&repo, ".env", &vec![b'x'; usize::try_from(MAX_FILE_BYTES).expect("a size") + 1]);

        let why = read(&root(repo.path()), &file(".env", 5)).expect_err("too big").to_string();

        assert!(why.contains(".env grew past"), "{why}");
    }

    #[test]
    fn refuses_a_fifo_without_waiting_for_a_writer() {
        let repo = repo(".env\n");
        let fifo = repo.path().join(".env");
        let made = std::process::Command::new("mkfifo").arg(&fifo).status().expect("mkfifo");
        assert!(made.success());

        let root = root(repo.path());
        let (told, heard) = std::sync::mpsc::channel();
        std::thread::spawn(move || told.send(read(&root, &file(".env", 0)).map_err(|error| error.to_string())));

        let read = heard.recv_timeout(std::time::Duration::from_secs(10)).expect("read waits for a writer on the fifo");
        let why = read.expect_err("a fifo");

        assert!(why.contains("not a regular file"), "{why}");
    }

    #[test]
    fn gives_the_permissions_the_file_has_when_it_is_read() {
        let repo = repo(".env\n");
        write(&repo, ".env", b"KEY=1");
        let path = repo.path().join(".env");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        let listing = list(repo.path()).expect("list");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");

        assert_eq!(read(&listing.root, &listing.files[0]).expect("read").1, 0o600);
    }

    fn another_folder(top: &Path, _: &Path) {
        std::fs::create_dir(top).expect("a new folder");
        std::fs::write(top.join(".env"), "KEY=2").expect("write file");
    }

    fn a_symlink(top: &Path, outside: &Path) {
        another_folder(outside, top);
        std::os::unix::fs::symlink(outside, top).expect("symlink");
    }

    #[rstest::rstest]
    #[case::another_folder(another_folder)]
    #[case::a_symlink(a_symlink)]
    fn refuses_a_project_folder_that_changed_since_its_files_were_listed(#[case] swap: fn(&Path, &Path)) {
        let outer = TempDir::new();
        let top = outer.path().join("shop");
        std::fs::create_dir(&top).expect("the project folder");
        git(&top, &["init", "--quiet"]);
        std::fs::write(top.join(".gitignore"), ".env\n").expect("write file");
        std::fs::write(top.join(".env"), "KEY=1").expect("write file");
        let listing = list(&top).expect("list");
        let moved = outer.path().join("moved");
        std::fs::rename(&top, &moved).expect("move the folder");

        swap(&top, &outer.path().join("outside"));

        let why = read(&listing.root, &listing.files[0]).expect_err("a changed folder").to_string();
        assert!(why.contains(MOVED), "{why}");
    }
}
