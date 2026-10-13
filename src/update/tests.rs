use rstest::rstest;

use super::*;
use crate::test_util::{FakeHttp, TempDir, write_executable};

mod check {
    use super::*;

    const LATEST: &str = r#"{"tag_name": "v9.0.0", "assets": [
            {"name": "cornercase-x86_64-unknown-linux-gnu.tar.gz", "browser_download_url": "https://x/a.tar.gz"}
        ]}"#;

    fn latest(server: &FakeHttp) -> String {
        format!("{}/releases/latest", server.url())
    }

    #[test]
    fn finds_a_newer_release_and_its_assets() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);

        let release = check(&latest(&server), "0.1.0").expect("check").expect("a newer release");

        assert_eq!(release.version, "9.0.0");
        assert_eq!(release.asset("cornercase-x86_64-unknown-linux-gnu.tar.gz").ok(), Some("https://x/a.tar.gz"));
    }

    #[rstest]
    #[case::only_the_notes(
        "## Release Notes\n\n- Faster.\n\n### Fixes\n\n- A crash.\n\n## Install cornercase 9.0.0\n\ncurl …",
        "- Faster.\n\n### Fixes\n\n- A crash."
    )]
    #[case::without_notes("## Install cornercase 9.0.0\n\ncurl …", "")]
    #[case::notes_at_the_end("## Release Notes\n\n- Faster.\n", "- Faster.")]
    fn keeps_the_release_notes_of_the_body(#[case] body: &str, #[case] expected: &str) {
        let json = serde_json::json!({"tag_name": "v9.0.0", "assets": [], "body": body});
        let notes = release(&json).expect("a release").notes;
        assert_eq!(notes.first().map_or("", |(_, notes)| notes.as_str()), expected);
    }

    #[test]
    fn ignores_a_release_that_is_not_newer() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
        assert_eq!(check(&latest(&server), "9.0.0").expect("check"), None);
    }

    #[test]
    fn sends_a_user_agent() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);

        let _ = check(&latest(&server), "0.1.0");

        assert!(server.request(0).to_lowercase().contains("user-agent: cornercase"), "{}", server.request(0));
    }

    #[test]
    fn fails_when_there_is_no_release() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 404, r#"{"message": "Not Found"}"#)]);

        let err = check(&latest(&server), "0.1.0").err().map(|e| e.to_string());

        assert_eq!(err.as_deref(), Some("GitHub answered 404 to the update check"));
    }
}

mod changelog {
    use super::*;

    const CHANGELOG: &str = "# Changelog\n\nIntro.\n\n## 9.1.0\n\n- Later.\n\n## 9.0.0\n\n- Newest.\n\n\
                                 ## 8.0.0\n\n- Middle.\n\n### Fixes\n\n- A crash.\n\n## 7.0.0\n\n\
                                 ## 0.1.0\n\n- Current.\n\n## 0.0.9\n\n- Older.\n";

    fn notes(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(version, notes)| ((*version).to_string(), (*notes).to_string())).collect()
    }

    fn with_body(assets: &Value) -> Release {
        let body = "## Release Notes\n\n- From the body.\n";
        release(&serde_json::json!({"tag_name": "v9.0.0", "assets": assets, "body": body})).expect("a release")
    }

    #[test]
    fn keeps_every_version_after_the_current_one_up_to_the_latest() {
        assert_eq!(
            changelog_notes(CHANGELOG, "0.1.0", "9.0.0"),
            notes(&[("9.0.0", "- Newest."), ("8.0.0", "- Middle.\n\n### Fixes\n\n- A crash.")])
        );
    }

    #[test]
    fn the_release_notes_come_from_its_changelog() {
        let server = FakeHttp::start(vec![("GET /CHANGELOG.md", 200, CHANGELOG)]);
        let url = format!("{}/CHANGELOG.md", server.url());

        let release = with_changelog(
            with_body(&serde_json::json!([{"name": "CHANGELOG.md", "browser_download_url": url}])),
            "0.1.0",
        );

        assert_eq!(release.notes, notes(&[("9.0.0", "- Newest."), ("8.0.0", "- Middle.\n\n### Fixes\n\n- A crash.")]));
    }

    #[test]
    fn a_release_without_a_changelog_keeps_its_own_notes() {
        assert_eq!(
            with_changelog(with_body(&serde_json::json!([])), "0.1.0").notes,
            notes(&[("9.0.0", "- From the body.")])
        );
    }

    #[test]
    fn a_changelog_without_the_release_keeps_its_own_notes() {
        let server = FakeHttp::start(vec![("GET /CHANGELOG.md", 200, "## 8.0.0\n\n- Middle.\n")]);
        let url = format!("{}/CHANGELOG.md", server.url());

        let release = with_changelog(
            with_body(&serde_json::json!([{"name": "CHANGELOG.md", "browser_download_url": url}])),
            "0.1.0",
        );

        assert_eq!(release.notes, notes(&[("9.0.0", "- From the body.")]));
    }

    #[test]
    fn a_changelog_that_cannot_be_downloaded_keeps_the_release_notes() {
        let server = FakeHttp::start(vec![("GET /CHANGELOG.md", 404, "")]);
        let url = format!("{}/CHANGELOG.md", server.url());

        let release = with_changelog(
            with_body(&serde_json::json!([{"name": "CHANGELOG.md", "browser_download_url": url}])),
            "0.1.0",
        );

        assert_eq!(release.notes, notes(&[("9.0.0", "- From the body.")]));
    }
}

mod install {
    use super::*;

    #[rstest]
    #[case::cellar("/opt/homebrew/Cellar/cornercase/0.1.0/bin/cornercase")]
    #[case::linuxbrew("/home/linuxbrew/.linuxbrew/bin/cornercase")]
    fn homebrew_installs_ask_for_brew(#[case] exe: &str) {
        assert_eq!(install(Path::new(exe), target()), Install::Command(BREW));
    }

    #[test]
    fn a_writable_folder_is_replaced_in_place() {
        let tmp = TempDir::new();
        let exe = tmp.path().join("cornercase");

        assert_eq!(install(&exe, Some("x86_64-unknown-linux-gnu")), Install::Replace(exe));
    }

    #[test]
    fn a_folder_we_cannot_write_asks_for_the_installer() {
        assert_eq!(install(Path::new("/cornercase-missing/cornercase"), target()), Install::Command(INSTALLER));
    }

    #[test]
    fn an_unknown_platform_asks_for_the_installer() {
        let tmp = TempDir::new();
        assert_eq!(install(&tmp.path().join("cornercase"), None), Install::Command(INSTALLER));
    }
}

mod replace {
    use super::*;

    const NEW: &str = "#!/bin/sh\necho 'cornercase 9.0.0'\n";

    fn archive(tmp: &TempDir, script: &str) -> Vec<u8> {
        let dir = tmp.path().join("build").join("cornercase-x86_64-unknown-linux-gnu");
        fs::create_dir_all(&dir).expect("archive folder");
        write_executable(&dir.join("cornercase"), script);
        let file = tmp.path().join("build.tar.gz");
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&file)
            .arg("-C")
            .arg(tmp.path().join("build"))
            .arg("cornercase-x86_64-unknown-linux-gnu")
            .status()
            .expect("run tar");
        assert!(status.success());
        fs::read(file).expect("read archive")
    }

    fn sums(bytes: &[u8]) -> String {
        let hex = hex(&Sha256::digest(bytes));
        format!("{hex} *cornercase-x86_64-unknown-linux-gnu.tar.gz\n")
    }

    fn installed(tmp: &TempDir) -> PathBuf {
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).expect("bin folder");
        let exe = bin.join("cornercase");
        fs::write(&exe, "old").expect("old binary");
        exe
    }

    #[test]
    fn swaps_the_binary_for_the_new_one() {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let bytes = archive(&tmp, NEW);

        replace(&bytes, &sums(&bytes), "9.0.0", &exe).expect("replace");

        assert_eq!(fs::read_to_string(&exe).expect("new binary"), NEW);
        assert_eq!(fs::read_dir(exe.parent().expect("bin")).expect("bin").count(), 1);
    }

    #[test]
    fn keeps_the_old_binary_when_the_checksum_differs() {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let bytes = archive(&tmp, NEW);

        let result = replace(&bytes, &sums(b"other"), "9.0.0", &exe);

        assert_refused_checksum(result, &exe);
    }

    fn assert_refused_checksum<T: std::fmt::Debug>(result: Result<T>, exe: &Path) {
        let err = result.err().map(|e| e.to_string());
        assert_eq!(err.as_deref(), Some("the download does not match its checksum"));
        assert_eq!(fs::read_to_string(exe).expect("old binary"), "old");
    }

    fn publish(tmp: &TempDir, sums: impl FnOnce(&[u8]) -> String) -> (FakeHttp, FakeHttp) {
        let bytes = archive(tmp, NEW);
        let checksums = sums(&bytes);
        let files = FakeHttp::start(vec![("GET /a.tar.gz", 200, bytes), ("GET /a.sha256", 200, checksums.into())]);
        let name = "cornercase-x86_64-unknown-linux-gnu.tar.gz";
        let json = serde_json::json!({"tag_name": "v9.0.0", "assets": [
            {"name": name, "browser_download_url": format!("{}/a.tar.gz", files.url())},
            {"name": format!("{name}.sha256"), "browser_download_url": format!("{}/a.sha256", files.url())},
        ]});
        let releases = FakeHttp::start(vec![("GET /releases/latest", 200, json.to_string())]);
        (releases, files)
    }

    fn install_from(releases: &FakeHttp, current: &str, install: &Install) -> Result<Outcome> {
        let url = format!("{}/releases/latest", releases.url());
        install_latest(&url, current, install, Some("x86_64-unknown-linux-gnu"))
    }

    #[test]
    fn installs_the_latest_release_over_the_binary() {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let (releases, _files) = publish(&tmp, sums);

        let outcome = install_from(&releases, "0.1.0", &Install::Replace(exe.clone())).expect("install");

        assert_eq!(outcome, Outcome::Updated("9.0.0".into()));
        assert_eq!(fs::read_to_string(&exe).expect("new binary"), NEW);
    }

    #[test]
    fn leaves_an_up_to_date_binary_alone() {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let (releases, files) = publish(&tmp, sums);

        let outcome = install_from(&releases, "9.0.0", &Install::Replace(exe.clone())).expect("install");

        assert_eq!(outcome, Outcome::UpToDate);
        assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
        assert!(files.requests().is_empty(), "{:?}", files.requests());
    }

    #[test]
    fn a_bad_checksum_keeps_the_old_binary() {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let (releases, _files) = publish(&tmp, |_| sums(b"other"));

        let result = install_from(&releases, "0.1.0", &Install::Replace(exe.clone()));

        assert_refused_checksum(result, &exe);
    }

    #[rstest]
    #[case::homebrew(BREW)]
    #[case::read_only_folder(INSTALLER)]
    fn an_install_we_cannot_replace_gets_its_command(#[case] command: &'static str) {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let (releases, files) = publish(&tmp, sums);

        let outcome = install_from(&releases, "0.1.0", &Install::Command(command)).expect("install");

        assert_eq!(outcome, Outcome::Manual("9.0.0".into(), command));
        assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
        assert!(files.requests().is_empty(), "{:?}", files.requests());
    }

    #[test]
    fn keeps_the_old_binary_when_the_new_one_does_not_run() {
        let tmp = TempDir::new();
        let exe = installed(&tmp);
        let bytes = archive(&tmp, "#!/bin/sh\necho 'missing library' >&2\nexit 127\n");

        let err = replace(&bytes, &sums(&bytes), "9.0.0", &exe).err().map(|e| e.to_string());

        assert_eq!(err.as_deref(), Some("the new cornercase does not run here: missing library"));
        assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
        assert_eq!(fs::read_dir(exe.parent().expect("bin")).expect("bin").count(), 1);
    }
}
