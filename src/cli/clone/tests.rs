use clap::error::ErrorKind;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::cli::args::{Command as Run, Control};
use crate::cli::tests::parse;
use crate::test_util::{TempDir, os_path};

const URL: &str = "https://example.com/shop.git";

fn parsed(args: &[&str]) -> CloneArgs {
    let cli = parse(args).expect("parse");
    let Some(Run::Control(Control::Clone(clone))) = cli.command else { panic!("not a clone") };
    clone
}

fn entry(path: &str, size: u64, too_big: bool) -> IgnoredFile {
    IgnoredFile { path: PathBuf::from(path), size, mode: 0o600, too_big }
}

fn listing(files: Vec<IgnoredFile>, left_out: usize) -> Listing {
    Listing { files, left_out, ..Listing::default() }
}

mod its_arguments {
    use super::*;

    #[test]
    fn need_a_url_unless_it_only_lists() {
        let refused = parse(&["clone"]).expect_err("no URL");

        assert_eq!((refused.kind(), refused.exit_code()), (ErrorKind::MissingRequiredArgument, 2));
        assert!(refused.to_string().contains("<URL>"), "{refused}");
        assert!(parsed(&["clone", "--list-ignored"]).url.is_none());
    }

    #[test]
    fn take_the_files_and_where_to_read_them() {
        let clone = parsed(&["clone", URL, "/srv/src", "--file", ".env", "--file", "web/.env.local", "--from", "here"]);

        assert_eq!(clone.url.as_deref(), Some(URL));
        assert_eq!(clone.files, [PathBuf::from(".env"), PathBuf::from("web/.env.local")]);
        assert_eq!(clone.from, Some(PathBuf::from("here")));
    }

    #[rstest]
    #[case::a_url(&["clone", "--list-ignored", URL])]
    #[case::a_file(&["clone", "--list-ignored", "--file", ".env"])]
    #[case::focus(&["clone", "--list-ignored", "--focus"])]
    #[case::a_view(&["clone", "--list-ignored", "--view", "41"])]
    fn refuse_what_means_nothing_for_a_list(#[case] args: &[&str]) {
        assert_eq!(parse(args).expect_err("nothing to clone").kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn refuse_a_folder_to_read_with_nothing_to_read() {
        let refused = parse(&["clone", URL, "--from", "here"]).expect_err("nothing to read");

        assert_eq!(refused.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn show_both_forms_in_the_usage() {
        let refused = parse(&["clone"]).expect_err("no URL").to_string();

        assert!(refused.contains("cornercase clone [OPTIONS] <URL> [FOLDER]"), "{refused}");
        assert!(refused.contains("cornercase clone --list-ignored"), "{refused}");
    }
}

mod the_list {
    use super::*;

    #[test]
    fn reads_as_the_dialog_offers_it() {
        let files = vec![entry(".env", 120, false), entry("data/big.sqlite", 9 << 20, true)];

        assert_eq!(
            listing_text(&listing(files, 3)),
            ".env  120 B\ndata/big.sqlite  9 MB · too big to copy\n… and 3 more not offered (200 at most)\n"
        );
    }

    #[test]
    fn says_so_when_it_is_empty() {
        assert_eq!(listing_text(&listing(Vec::new(), 0)), "No git-ignored files here.\n");
    }

    #[test]
    fn gives_each_file_its_size_mode_and_whether_it_is_too_big_in_json() {
        let shown = listing(vec![entry(".env", 120, false), entry("big.bin", 9 << 20, true)], 3);

        assert_eq!(
            serde_json::to_value(Offered::from(&shown)).expect("json"),
            json!({
                "root": "",
                "files": [
                    {"path": ".env", "size": 120, "mode": 0o600, "too_big": false},
                    {"path": "big.bin", "size": 9 << 20, "mode": 0o600, "too_big": true},
                ],
                "left_out": 3,
            })
        );
    }
}

mod the_files_to_copy {
    use super::*;

    #[test]
    fn are_none_without_looking_at_the_folder_when_none_is_named() {
        let folder = PathBuf::from(os_path("/nonexistent/repository"));
        let args = parsed(&["clone", URL, "--focus"]);

        assert_eq!(files_to_copy(&CloneArgs { from: Some(folder), ..args }).expect("no files"), []);
    }

    #[cfg(unix)]
    mod read_here {
        use std::os::unix::fs::PermissionsExt;

        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD;

        use super::*;
        use crate::git::ignored::MAX_FILE_BYTES;
        use crate::test_util::git_repo;

        fn clone_of(repo: &Path, files: &[&str]) -> CloneArgs {
            let mut args = vec!["clone", URL, "--from", repo.to_str().expect("a UTF-8 folder")];
            args.extend(files.iter().flat_map(|file| ["--file", *file]));
            parsed(&args)
        }

        #[test]
        fn travel_in_base64_with_their_permissions() {
            let repo = git_repo(&[(".gitignore", ".env\n")]);
            let env = repo.path().join(".env");
            std::fs::write(&env, "KEY=1").expect("write the file");
            std::fs::set_permissions(&env, std::fs::Permissions::from_mode(0o600)).expect("chmod");

            let files = files_to_copy(&clone_of(repo.path(), &[".env"])).expect("read");

            let data = STANDARD.encode("KEY=1");
            assert_eq!(files, [CloneFile { path: PathBuf::from(".env"), data, mode: 0o600 }]);
        }

        #[test]
        fn are_refused_past_what_a_clone_takes_in_all() {
            let repo = git_repo(&[(".gitignore", "*.bin\n")]);
            let names: Vec<String> = (0..9).map(|n| format!("{n}.bin")).collect();
            let size = usize::try_from(MAX_FILE_BYTES).expect("a small cap");
            for name in &names {
                std::fs::write(repo.path().join(name), vec![0; size]).expect("write the file");
            }
            let names: Vec<&str> = names.iter().map(String::as_str).collect();

            let why = files_to_copy(&clone_of(repo.path(), &names)).expect_err("too much").to_string();

            assert_eq!(why, "cannot copy the files: over the 8 MiB a clone takes");
        }

        #[test]
        fn outside_a_repository_name_the_folder() {
            let folder = TempDir::new();

            let why = files_to_copy(&clone_of(folder.path(), &[".env"])).expect_err("no repository").to_string();

            assert!(why.starts_with(&format!("cannot read the files of `{}`: ", folder.path().display())), "{why}");
        }
    }
}

#[cfg(windows)]
mod on_windows {
    use super::*;

    fn refused(args: &[&str]) -> String {
        run(&parsed(args)).expect_err("not on Windows").to_string()
    }

    #[test]
    fn the_files_are_refused_in_words_before_the_server_is_asked() {
        let folder = TempDir::new();
        let from = folder.path().to_str().expect("a UTF-8 folder");

        assert_eq!(
            refused(&["clone", URL, "--file", ".env", "--from", from]),
            "copying git-ignored files into a clone is not supported on Windows yet"
        );
    }

    #[test]
    fn the_list_is_refused_in_the_same_words() {
        let folder = TempDir::new();
        let from = folder.path().to_str().expect("a UTF-8 folder");

        assert_eq!(
            refused(&["clone", "--list-ignored", "--from", from]),
            "copying git-ignored files into a clone is not supported on Windows yet"
        );
    }
}
