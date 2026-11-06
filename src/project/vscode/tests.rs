use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, os_path};

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, text).expect("write workspace");
    file
}

fn folders_read(dir: &Path, text: &str) -> Vec<PathBuf> {
    let file = write(dir, "w.code-workspace", text);
    read(&file).expect("read").folders.into_iter().map(|f| f.path).collect()
}

mod reading {
    use super::*;

    #[test]
    fn takes_its_name_from_the_file() {
        let tmp = TempDir::new();
        let file = write(tmp.path(), "work.code-workspace", r#"{"folders": []}"#);

        assert_eq!(read(&file).expect("read").name, "work");
    }

    #[test]
    fn resolves_relative_folders_from_the_file() {
        let tmp = TempDir::new();

        let paths = folders_read(tmp.path(), r#"{"folders": [{"path": "api"}, {"path": "../web"}]}"#);

        assert_eq!(paths, [tmp.path().join("api"), tmp.path().join("../web")]);
    }

    #[cfg(unix)]
    #[test]
    fn resolves_relative_folders_from_the_real_folder_of_the_file() {
        let tmp = TempDir::new();
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).expect("create folder");
        write(&real, "w.code-workspace", r#"{"folders": [{"path": "api"}]}"#);
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("link");

        let folders = read(&link.join("w.code-workspace")).expect("read").folders;

        assert_eq!(folders[0].path, real.join("api"));
    }

    #[test]
    fn keeps_absolute_folders_and_their_names() {
        let tmp = TempDir::new();
        let api = os_path("/srv/api");
        let file =
            write(tmp.path(), "w.code-workspace", &format!(r#"{{"folders": [{{"path": "{api}", "name": "API"}}]}}"#));

        let folders = read(&file).expect("read").folders;

        assert_eq!(folders, [Folder { path: PathBuf::from(api), name: Some("API".into()) }]);
    }

    #[test]
    fn reads_local_file_uris_and_skips_remote_ones() {
        let tmp = TempDir::new();
        let text = r#"{"folders": [
                {"uri": "file:///home/me/my%20app"},
                {"uri": "file://localhost/srv/api"},
                {"uri": "file://server/share/web"},
                {"uri": "vscode-remote://ssh-remote+box/srv/api"}
            ]}"#;

        let paths = folders_read(tmp.path(), text);

        assert_eq!(paths, [PathBuf::from("/home/me/my app"), PathBuf::from("/srv/api")]);
    }

    #[test]
    #[cfg(windows)]
    fn skips_folders_on_another_machine() {
        let tmp = TempDir::new();
        let text = r#"{"folders": [
                {"path": "api"},
                {"path": "\\\\server\\share\\web"},
                {"path": "//server/share/docs"},
                {"uri": "file:////server/share/lib"}
            ]}"#;

        assert_eq!(folders_read(tmp.path(), text), [tmp.path().join("api")]);
    }

    #[test]
    fn accepts_comments_and_trailing_commas() {
        let tmp = TempDir::new();
        let text = r#"{
                // the repos
                "folders": [
                    { "path": "api", }, /* the backend */
                    { "path": "web" },
                ],
                "settings": { "url": "http://example.com/x", },
            }"#;
        let file = write(tmp.path(), "w.code-workspace", text);

        assert_eq!(read(&file).expect("read").folders.len(), 2);
    }

    #[test]
    fn skips_a_byte_order_mark() {
        let tmp = TempDir::new();
        let file = write(tmp.path(), "w.code-workspace", "\u{feff}{\"folders\": [{\"path\": \"api\"}]}");

        assert_eq!(read(&file).expect("read").folders.len(), 1);
    }

    #[test]
    fn names_a_file_that_is_not_text() {
        let tmp = TempDir::new();
        let file = tmp.path().join("w.code-workspace");
        std::fs::write(&file, [0xff, 0xfe, 0x00]).expect("write workspace");

        let error = read(&file).expect_err("not text").to_string();

        assert!(error.contains("w.code-workspace"), "{error}");
    }

    #[test]
    fn says_why_a_broken_file_cannot_be_read() {
        let tmp = TempDir::new();
        let file = write(tmp.path(), "w.code-workspace", "{ nope");

        let error = read(&file).expect_err("broken file").to_string();

        assert!(error.contains("w.code-workspace"), "{error}");
    }
}

mod jsonc {
    use super::*;

    #[rstest]
    #[case::line_comment("{\"a\": 1 // x\n}", r#"{"a": 1}"#)]
    #[case::block_comment(r#"{/* x */"a": 1}"#, r#"{"a": 1}"#)]
    #[case::slashes_in_strings(r#"{"a": "//not /* a comment */"}"#, r#"{"a": "//not /* a comment */"}"#)]
    #[case::escaped_quotes(r#"{"a": "\"// x"}"#, r#"{"a": "\"// x"}"#)]
    #[case::trailing_commas(r#"{"a": [1, 2, ], }"#, r#"{"a": [1, 2]}"#)]
    #[case::commas_in_strings(r#"{"a": ", }"}"#, r#"{"a": ", }"}"#)]
    fn becomes_plain_json(#[case] text: &str, #[case] json: &str) {
        let got: serde_json::Value = serde_json::from_str(&strip_jsonc(text)).expect("valid json");
        let want: serde_json::Value = serde_json::from_str(json).expect("valid json");
        assert_eq!(got, want);
    }
}

mod is_workspace {
    use super::*;

    #[test]
    fn takes_a_file_with_the_extension() {
        let tmp = TempDir::new();

        assert!(is_workspace(&write(tmp.path(), "w.code-workspace", "{}")));
    }

    #[test]
    fn refuses_another_file_a_folder_and_what_is_not_there() {
        let tmp = TempDir::new();
        let folder = tmp.path().join("folder.code-workspace");
        std::fs::create_dir(&folder).expect("create folder");

        assert!(!is_workspace(&write(tmp.path(), "w.json", "{}")));
        assert!(!is_workspace(&folder));
        assert!(!is_workspace(&tmp.path().join("gone.code-workspace")));
    }
}
