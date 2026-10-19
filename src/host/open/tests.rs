use std::sync::mpsc;

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, write_executable};

const NONE: [Opener; 0] = [];

fn env_of(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
    move |name| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.clone())
}

fn bin_with(names: &[&str]) -> TempDir {
    let bin = TempDir::new();
    for name in names {
        write_executable(&bin.path().join(name), "#!/bin/sh\nexit 0\n");
    }
    bin
}

fn found(os: Os, bin: &TempDir, extra: Vec<(&'static str, String)>, apps: &[PathBuf]) -> Openers {
    let mut pairs = vec![("PATH", bin.path().display().to_string()), ("DISPLAY", ":0".to_string())];
    pairs.extend(extra);
    Openers::find(os, &env_of(pairs), apps)
}

fn open(opener: Opener, path: &str, line: Option<u32>) -> Open {
    Open { opener, path: path.into(), line }
}

fn argv(steps: &[Step]) -> Vec<Vec<String>> {
    steps
        .iter()
        .map(|step| {
            step.iter()
                .enumerate()
                .map(|(i, part)| {
                    let part = part.to_string_lossy().into_owned();
                    if i == 0 { part.rsplit('/').next().unwrap_or_default().to_string() } else { part }
                })
                .collect()
        })
        .collect()
}

mod detection {
    use super::*;

    #[test]
    fn linux_finds_the_file_manager_the_default_app_and_editors_in_menu_order() {
        let bin = bin_with(&["xdg-open", "subl", "code", "cursor"]);
        let openers = found(Os::Linux, &bin, vec![], &[]);
        assert_eq!(openers.ids(), [Opener::Files, Opener::Vscode, Opener::Cursor, Opener::Sublime, Opener::Default]);
    }

    #[test]
    fn macos_finds_finder_and_the_default_app_through_open() {
        let bin = bin_with(&["open"]);
        let openers = found(Os::Macos, &bin, vec![], &[]);
        assert_eq!(openers.ids(), [Opener::Finder, Opener::Default]);
    }

    #[test]
    fn a_window_over_ssh_finds_nothing() {
        let bin = bin_with(&["xdg-open", "code"]);
        assert_eq!(found(Os::Linux, &bin, vec![("SSH_CONNECTION", "1 2 3 4".into())], &[]).ids(), NONE);
        assert_eq!(found(Os::Linux, &bin, vec![("SSH_TTY", "/dev/pts/1".into())], &[]).ids(), NONE);
    }

    #[test]
    fn an_empty_ssh_variable_counts_as_local() {
        let bin = bin_with(&["xdg-open"]);
        assert_ne!(found(Os::Linux, &bin, vec![("SSH_CONNECTION", String::new())], &[]).ids(), NONE);
    }

    #[test]
    fn linux_needs_a_graphical_session() {
        let bin = bin_with(&["xdg-open", "code"]);
        let env = env_of(vec![("PATH", bin.path().display().to_string())]);
        assert_eq!(Openers::find(Os::Linux, &env, &[]).ids(), NONE);
        let wayland = env_of(vec![("PATH", bin.path().display().to_string()), ("WAYLAND_DISPLAY", "wayland-0".into())]);
        assert_ne!(Openers::find(Os::Linux, &wayland, &[]).ids(), NONE);
    }

    #[test]
    fn a_zed_command_outside_zed_app_is_ignored_but_zeditor_counts() {
        let zfs = bin_with(&["xdg-open", "zed"]);
        assert!(!found(Os::Linux, &zfs, vec![], &[]).ids().contains(&Opener::Zed));
        let packaged = bin_with(&["xdg-open", "zeditor"]);
        assert!(found(Os::Linux, &packaged, vec![], &[]).ids().contains(&Opener::Zed));
    }

    #[test]
    fn a_zed_command_inside_zed_app_counts() {
        let tmp = TempDir::new();
        let bin = tmp.path().join("zed.app").join("bin");
        std::fs::create_dir_all(&bin).expect("folder");
        write_executable(&bin.join("zed"), "#!/bin/sh\nexit 0\n");
        write_executable(&tmp.path().join("xdg-open"), "#!/bin/sh\nexit 0\n");
        let path = format!("{}:{}", bin.display(), tmp.path().display());
        let openers = Openers::find(Os::Linux, &env_of(vec![("PATH", path), ("DISPLAY", ":0".into())]), &[]);
        assert!(openers.ids().contains(&Opener::Zed));
    }

    #[test]
    fn macos_finds_an_editor_by_its_bundle_and_a_command_beats_it() {
        let apps = TempDir::new();
        std::fs::create_dir(apps.path().join("Cursor.app")).expect("bundle");
        std::fs::create_dir(apps.path().join("Visual Studio Code.app")).expect("bundle");
        let bin = bin_with(&["open", "code"]);
        let openers = found(Os::Macos, &bin, vec![], &[apps.path().to_path_buf()]);
        assert_eq!(openers.ids(), [Opener::Finder, Opener::Vscode, Opener::Cursor, Opener::Default]);
        let steps = |opener| openers.steps(&open(opener, "/work/web", None), Some(Kind::Folder)).expect("steps");
        assert_eq!(argv(&steps(Opener::Vscode)), [["code", "/work/web"]]);
        let cursor = argv(&steps(Opener::Cursor));
        assert_eq!(cursor[0][..2], ["open".to_string(), "-a".to_string()]);
        assert!(cursor[0][2].ends_with("Cursor.app"));
    }

    #[test]
    fn a_bundle_needs_open() {
        let apps = TempDir::new();
        std::fs::create_dir(apps.path().join("Zed.app")).expect("bundle");
        let bin = bin_with(&[]);
        assert_eq!(found(Os::Macos, &bin, vec![], &[apps.path().to_path_buf()]).ids(), NONE);
    }
}

mod commands {
    use super::*;

    fn openers(os: Os, names: &[&str]) -> (TempDir, Openers) {
        let bin = bin_with(names);
        let openers = found(os, &bin, vec![], &[]);
        (bin, openers)
    }

    #[rstest]
    #[case::vscode_folder(Opener::Vscode, "/work/web", None, Kind::Folder, vec![vec!["code", "/work/web"]])]
    #[case::vscode_file(Opener::Vscode, "/work/web/a.rs", None, Kind::File, vec![vec!["code", "/work/web/a.rs"]])]
    #[case::vscode_line(Opener::Vscode, "/work/web/a.rs", Some(11), Kind::File, vec![vec!["code", "-g", "/work/web/a.rs:11"]])]
    #[case::cursor_line(Opener::Cursor, "/work/web/a.rs", Some(3), Kind::File, vec![vec!["cursor", "-g", "/work/web/a.rs:3"]])]
    #[case::zed_line(Opener::Zed, "/work/web/a.rs", Some(7), Kind::File, vec![vec!["zeditor", "/work/web/a.rs:7"]])]
    #[case::sublime_line(Opener::Sublime, "/work/web/a.rs", Some(2), Kind::File, vec![vec!["subl", "/work/web/a.rs:2"]])]
    #[case::a_folder_takes_no_line(Opener::Vscode, "/work/web", Some(2), Kind::Folder, vec![vec!["code", "/work/web"]])]
    #[case::files_folder(Opener::Files, "/work/web", None, Kind::Folder, vec![vec!["xdg-open", "/work/web"]])]
    #[case::default_file(Opener::Default, "/work/web/a.png", None, Kind::File, vec![vec!["xdg-open", "/work/web/a.png"]])]
    fn linux(
        #[case] opener: Opener,
        #[case] path: &str,
        #[case] line: Option<u32>,
        #[case] kind: Kind,
        #[case] expected: Vec<Vec<&str>>,
    ) {
        let (_bin, openers) = openers(Os::Linux, &["xdg-open", "code", "cursor", "zeditor", "subl"]);
        let steps = openers.steps(&open(opener, path, line), Some(kind)).expect("steps");
        let expected: Vec<Vec<String>> = expected.iter().map(|s| s.iter().map(ToString::to_string).collect()).collect();
        assert_eq!(argv(&steps), expected);
    }

    #[test]
    fn a_file_in_the_linux_file_manager_is_revealed_through_dbus_then_its_folder() {
        let (_bin, openers) = openers(Os::Linux, &["xdg-open", "dbus-send"]);
        let steps = openers.steps(&open(Opener::Files, "/work/my web/a,b.rs", None), Some(Kind::File)).expect("steps");
        let steps = argv(&steps);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0][0], "dbus-send");
        assert!(steps[0].contains(&"array:string:file:///work/my%20web/a%2Cb.rs".to_string()));
        assert_eq!(steps[0].last().map(String::as_str), Some("string:"));
        assert_eq!(steps[1], ["xdg-open", "/work/my web"]);
    }

    #[test]
    fn without_dbus_send_the_file_manager_opens_the_files_folder() {
        let (_bin, openers) = openers(Os::Linux, &["xdg-open"]);
        let steps = openers.steps(&open(Opener::Files, "/work/web/a.rs", None), Some(Kind::File)).expect("steps");
        assert_eq!(argv(&steps), [["xdg-open", "/work/web"]]);
    }

    #[rstest]
    #[case::folder("/work/web", Kind::Folder, &["open", "/work/web"])]
    #[case::a_package_folder_is_revealed("/work/Tool.app", Kind::Folder, &["open", "-R", "/work/Tool.app"])]
    #[case::a_folder_named_like_a_site_is_opened("/work/usecornercase.dev", Kind::Folder, &["open", "/work/usecornercase.dev"])]
    #[case::an_xcode_project_is_revealed("/work/Shop.xcodeproj", Kind::Folder, &["open", "-R", "/work/Shop.xcodeproj"])]
    #[case::file("/work/web/a.rs", Kind::File, &["open", "-R", "/work/web/a.rs"])]
    #[case::program("/work/web/run", Kind::Program, &["open", "-R", "/work/web/run"])]
    fn finder(#[case] path: &str, #[case] kind: Kind, #[case] expected: &[&str]) {
        let (_bin, openers) = openers(Os::Macos, &["open"]);
        let steps = openers.steps(&open(Opener::Finder, path, None), Some(kind)).expect("steps");
        assert_eq!(argv(&steps), [expected.iter().map(ToString::to_string).collect::<Vec<_>>()]);
    }

    #[rstest]
    #[case::relative(open(Opener::Files, "work/web", None), Some(Kind::Folder), "the path is not absolute")]
    #[case::missing_opener(open(Opener::Zed, "/work/web", None), Some(Kind::Folder), "Zed is not on this computer")]
    #[case::gone(open(Opener::Files, "/work/web", None), None, "it is gone")]
    #[case::default_for_a_folder(
        open(Opener::Default, "/work/web", None),
        Some(Kind::Folder),
        "the default app opens only plain files"
    )]
    #[case::default_for_a_program(
        open(Opener::Default, "/work/web/run", None),
        Some(Kind::Program),
        "the default app opens only plain files"
    )]
    fn refusals(#[case] order: Open, #[case] kind: Option<Kind>, #[case] reason: &str) {
        let (_bin, openers) = openers(Os::Linux, &["xdg-open"]);
        assert_eq!(openers.steps(&order, kind).expect_err("refused"), reason);
    }
}

mod uris {
    use super::*;

    #[rstest]
    #[case::plain("/a/b-c_d.e~f", "file:///a/b-c_d.e~f")]
    #[case::space("/a b", "file:///a%20b")]
    #[case::comma("/a,b", "file:///a%2Cb")]
    #[case::non_ascii("/é", "file:///%C3%A9")]
    fn are_percent_encoded(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(uri(path), expected);
    }
}

mod running {
    use super::*;

    fn run_one(script: &str) -> Result<(), String> {
        let tmp = TempDir::new();
        let program = tmp.path().join("xdg-open");
        write_executable(&program, script);
        run(&[vec![program.into_os_string()]])
    }

    #[test]
    fn a_failing_opener_reports_its_exit_status() {
        assert_eq!(run_one("#!/bin/sh\nexit 3\n"), Err("exit status 3".to_string()));
    }

    #[test]
    fn a_working_opener_reports_nothing() {
        assert_eq!(run_one("#!/bin/sh\nexit 0\n"), Ok(()));
    }

    #[test]
    fn a_missing_program_reports_the_spawn_error() {
        let steps = [vec![OsString::from("/nonexistent/opener")]];
        assert!(run(&steps).is_err());
    }

    #[test]
    fn a_window_without_openers_ignores_the_order() {
        let (tx, rx) = mpsc::channel();
        Openers::default().open(&open(Opener::Files, "/", None), move |failed| {
            let _ = tx.send(failed);
        });
        assert!(rx.recv().is_err());
    }

    #[test]
    fn a_relative_path_is_refused_by_the_client() {
        let (tx, rx) = mpsc::channel();
        let openers = Openers {
            found: vec![(Opener::Files, Reach::Launcher)],
            launcher: Some(PathBuf::from("/bin/true")),
            dbus: None,
        };
        openers.open(&open(Opener::Files, "relative", None), move |failed| {
            let _ = tx.send(failed);
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).expect("failed").reason, "the path is not absolute");
    }
}
