use rstest::rstest;

use super::*;
#[cfg(unix)]
use crate::test_util::write_executable;
use crate::test_util::{FakeHttp, TempDir};

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
    #[case::linux("x86_64-unknown-linux-gnu", "cornercase-x86_64-unknown-linux-gnu.tar.gz")]
    #[case::macos("aarch64-apple-darwin", "cornercase-aarch64-apple-darwin.tar.gz")]
    #[case::windows("x86_64-pc-windows-msvc", "cornercase-x86_64-pc-windows-msvc.zip")]
    fn each_target_downloads_its_own_archive(#[case] target: &str, #[case] expected: &str) {
        assert_eq!(archive_name(target), expected);
    }

    #[test]
    fn windows_runs_the_tar_of_its_system_folder_by_its_path() {
        let folder = PathBuf::from("Windows");

        assert_eq!(tar_in(true, Some(folder.clone())), folder.join("System32").join("tar.exe"));
    }

    #[rstest]
    #[case::unix(false, Some("Windows"))]
    #[case::no_windows_folder(true, None)]
    fn elsewhere_tar_is_looked_up(#[case] windows: bool, #[case] folder: Option<&str>) {
        assert_eq!(tar_in(windows, folder.map(PathBuf::from)), PathBuf::from("tar"));
    }

    #[test]
    fn the_installer_command_fits_the_system() {
        let expected = if cfg!(windows) { "install.ps1 | iex" } else { "install.sh | sh" };
        assert!(INSTALLER.contains(expected), "{INSTALLER}");
    }

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

#[cfg(unix)]
pub(crate) mod replace {
    use super::*;

    pub(crate) const NEW: &str = "#!/bin/sh\necho 'cornercase 9.0.0'\n";

    fn archive(tmp: &TempDir, script: &str) -> Vec<u8> {
        archive_for(tmp, "x86_64-unknown-linux-gnu", script)
    }

    fn archive_for(tmp: &TempDir, target: &str, script: &str) -> Vec<u8> {
        let dir = tmp.path().join("build").join(format!("cornercase-{target}"));
        fs::create_dir_all(&dir).expect("archive folder");
        write_executable(&dir.join("cornercase"), script);
        let file = tmp.path().join("build.tar.gz");
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&file)
            .arg("-C")
            .arg(tmp.path().join("build"))
            .arg(format!("cornercase-{target}"))
            .status()
            .expect("run tar");
        assert!(status.success());
        fs::read(file).expect("read archive")
    }

    fn sums(bytes: &[u8]) -> String {
        sums_for("x86_64-unknown-linux-gnu", bytes)
    }

    pub(crate) fn sums_for(target: &str, bytes: &[u8]) -> String {
        let hex = hex(&Sha256::digest(bytes));
        format!("{hex} *cornercase-{target}.tar.gz\n")
    }

    pub(crate) fn installed(tmp: &TempDir) -> PathBuf {
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
        publish_for(tmp, "x86_64-unknown-linux-gnu", sums)
    }

    pub(crate) fn publish_for(tmp: &TempDir, target: &str, sums: impl FnOnce(&[u8]) -> String) -> (FakeHttp, FakeHttp) {
        let bytes = archive_for(tmp, target, NEW);
        let checksums = sums(&bytes);
        let files = FakeHttp::start(vec![("GET /a.tar.gz", 200, bytes), ("GET /a.sha256", 200, checksums.into())]);
        let name = archive_name(target);
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

#[cfg(windows)]
mod on_windows {
    use std::process::Stdio;

    use super::*;
    use crate::os::fs::stat;
    use crate::test_util::{Running, fake_program, verbatim, wait_until};

    const TARGET: &str = "x86_64-pc-windows-msvc";

    fn sums(bytes: &[u8]) -> String {
        format!("{} *{}\n", hex(&Sha256::digest(bytes)), archive_name(TARGET))
    }

    fn zipped(tmp: &TempDir, seed: &Path, says: &[(&str, &str)]) -> Vec<u8> {
        let build = tmp.path().join("build");
        fs::create_dir_all(&build).expect("build folder");
        fs::copy(seed, build.join("cornercase.exe")).expect("the new binary");
        let mut names = vec!["cornercase.exe".to_string()];
        for (extension, text) in says {
            let name = format!("cornercase.{extension}");
            fs::write(build.join(&name), text).expect("what the new binary says");
            names.push(name);
        }
        let zip = tmp.path().join("build.zip");
        let status = Command::new(tar())
            .arg("-a")
            .arg("-cf")
            .arg(&zip)
            .arg("-C")
            .arg(&build)
            .args(&names)
            .status()
            .expect("tar");
        assert!(status.success());
        fs::read(zip).expect("read zip")
    }

    fn bin(tmp: &TempDir) -> PathBuf {
        let bin = verbatim(tmp.path()).join("bin");
        fs::create_dir(&bin).expect("bin folder");
        bin.join("cornercase.exe")
    }

    fn running(exe: &Path, seed: &Path) -> Running {
        fs::copy(seed, exe).expect("the old binary");
        fs::write(exe.with_extension("hold"), "").expect("keep it running");
        Running::spawn(Command::new(exe).stdout(Stdio::null()))
    }

    #[test]
    fn the_tar_is_the_one_in_system32() {
        let tar = tar();

        assert!(tar.is_file(), "{}", tar.display());
        assert!(tar.ends_with(Path::new("System32").join("tar.exe")), "{}", tar.display());
    }

    #[test]
    fn a_flat_zip_is_unpacked_by_the_system_tar() {
        let tmp = TempDir::new();
        let seed = fake_program(tmp.path(), "seed");
        let staging = verbatim(tmp.path()).join("staging");
        fs::create_dir(&staging).expect("staging folder");

        let binary = unpack(&zipped(&tmp, &seed, &[("out", "cornercase 9.0.0\n")]), &staging).expect("unpack");

        assert_eq!(binary, staging.join("cornercase.exe"));
    }

    #[test]
    fn a_running_binary_is_replaced_and_its_old_copy_goes_once_it_ends() {
        let tmp = TempDir::new();
        let seed = fake_program(tmp.path(), "seed");
        let exe = bin(&tmp);
        let old = running(&exe, &seed);
        let id = stat(&exe).expect("the old binary").id();
        let bytes = zipped(&tmp, &seed, &[("out", "cornercase 9.0.0\n")]);

        replace(&bytes, &sums(&bytes), "9.0.0", &exe).expect("replace");

        let aside = exe.with_file_name("cornercase.old.exe");
        assert_eq!(stat(&aside).expect("the old binary, set aside").id(), id);
        assert_ne!(stat(&exe).expect("the new binary").id(), id);
        drop(old);
        wait_until("the old copy is removed", || {
            swap::remove_old(&exe);
            !aside.exists()
        });
    }

    #[test]
    fn a_new_binary_that_does_not_run_leaves_the_old_one() {
        let tmp = TempDir::new();
        let seed = fake_program(tmp.path(), "seed");
        let exe = bin(&tmp);
        fs::write(&exe, "old").expect("old binary");
        let bytes = zipped(&tmp, &seed, &[("err", "missing library"), ("code", "127")]);

        let err = replace(&bytes, &sums(&bytes), "9.0.0", &exe).err().map(|e| e.to_string());

        assert_eq!(err.as_deref(), Some("the new cornercase does not run here: missing library"));
        assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
        assert_eq!(fs::read_dir(exe.parent().expect("bin")).expect("bin").count(), 1);
    }
}
