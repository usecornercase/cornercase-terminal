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
    steps.iter().map(arguments).collect()
}

fn arguments(step: &Step) -> Vec<String> {
    let Step::Run(step) = step else { panic!("{step:?} runs no program") };
    let name = |part: &OsString| Path::new(part).file_name().map(|name| name.to_string_lossy().into_owned());
    let program = step.first().and_then(name).into_iter();
    program.chain(step.iter().skip(1).map(|part| part.to_string_lossy().into_owned())).collect()
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
        run(&[Step::Run(vec![program.into_os_string()])])
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
        let steps = [Step::Run(vec![OsString::from("/nonexistent/opener")])];
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

#[cfg(windows)]
mod on_windows {
    use super::*;

    const ALL: [&str; 6] = ["explorer.exe", "code.cmd", "cursor.cmd", "zed.exe", "subl.exe", "code"];

    fn bin_of(names: &[&str]) -> TempDir {
        let bin = TempDir::new();
        for name in names {
            std::fs::write(bin.path().join(name), "").expect("a fake program");
        }
        bin
    }

    fn windows(names: &[&str], extra: Vec<(&'static str, String)>) -> (TempDir, Openers) {
        let bin = bin_of(names);
        let mut pairs = vec![("PATH", bin.path().display().to_string())];
        pairs.extend(extra);
        let openers = Openers::find(Os::Windows, &env_of(pairs), &[]);
        (bin, openers)
    }

    fn shell(verb: Option<&'static str>, file: impl Into<OsString>, parameters: Option<&str>) -> Step {
        Step::Shell(Shell { verb, file: file.into(), parameters: parameters.map(OsString::from) })
    }

    mod detection {
        use super::*;

        #[test]
        fn finds_explorer_the_default_app_and_editors_through_pathext() {
            let (_bin, openers) = windows(&ALL, vec![]);
            let expected =
                [Opener::Explorer, Opener::Vscode, Opener::Cursor, Opener::Zed, Opener::Sublime, Opener::Default];
            assert_eq!(openers.ids(), expected);
        }

        #[test]
        fn a_command_without_an_extension_is_not_one() {
            let (_bin, openers) = windows(&["explorer.exe", "code"], vec![]);
            assert_eq!(openers.ids(), [Opener::Explorer, Opener::Default]);
        }

        #[test]
        fn pathext_decides_which_files_are_commands() {
            let (_bin, openers) = windows(&["explorer.exe", "code.cmd"], vec![("PATHEXT", ".EXE".into())]);
            assert_eq!(openers.ids(), [Opener::Explorer, Opener::Default]);
        }

        #[test]
        fn without_explorer_only_editors_are_found() {
            let (_bin, openers) = windows(&["code.cmd"], vec![]);
            assert_eq!(openers.ids(), [Opener::Vscode]);
        }

        #[test]
        fn needs_no_display_but_finds_nothing_over_ssh() {
            let (_bin, openers) = windows(&ALL, vec![("SSH_CONNECTION", "10.0.0.2 51000 10.0.0.1 22".into())]);
            assert_eq!(openers.ids(), NONE);
        }

        #[test]
        fn a_mac_bundle_is_never_an_editor() {
            let (apps, bin) = (TempDir::new(), bin_of(&["explorer.exe"]));
            std::fs::create_dir(apps.path().join("Cursor.app")).expect("bundle");
            let env = env_of(vec![("PATH", bin.path().display().to_string())]);
            let openers = Openers::find(Os::Windows, &env, &[apps.path().to_path_buf()]);
            assert_eq!(openers.ids(), [Opener::Explorer, Opener::Default]);
        }
    }

    mod commands {
        use super::*;

        fn steps(opener: Opener, path: &str, line: Option<u32>, kind: Kind) -> (TempDir, Vec<Step>) {
            let (bin, openers) = windows(&ALL, vec![]);
            let steps = openers.steps(&open(opener, path, line), Some(kind)).expect("steps");
            (bin, steps)
        }

        fn explorer_in(bin: &TempDir) -> PathBuf {
            bin.path().join("explorer.exe")
        }

        #[rstest]
        #[case::vscode_folder(Opener::Vscode, r"C:\work\web", None, Kind::Folder, &["code.cmd", r"C:\work\web"])]
        #[case::vscode_line(Opener::Vscode, r"C:\work\web\a.rs", Some(11), Kind::File, &["code.cmd", "-g", r"C:\work\web\a.rs:11"])]
        #[case::cursor_line(Opener::Cursor, r"C:\work\web\a.rs", Some(3), Kind::File, &["cursor.cmd", "-g", r"C:\work\web\a.rs:3"])]
        #[case::zed_line(Opener::Zed, r"C:\work\web\a.rs", Some(7), Kind::File, &["zed.exe", r"C:\work\web\a.rs:7"])]
        #[case::sublime_line(Opener::Sublime, r"C:\work\web\a.rs", Some(2), Kind::File, &["subl.exe", r"C:\work\web\a.rs:2"])]
        #[case::a_verbatim_path(Opener::Vscode, r"\\?\C:\work\web\a.rs", Some(4), Kind::File, &["code.cmd", "-g", r"C:\work\web\a.rs:4"])]
        fn editors(
            #[case] opener: Opener,
            #[case] path: &str,
            #[case] line: Option<u32>,
            #[case] kind: Kind,
            #[case] expected: &[&str],
        ) {
            let (_bin, steps) = steps(opener, path, line, kind);
            assert_eq!(argv(&steps), [expected.iter().map(ToString::to_string).collect::<Vec<_>>()]);
        }

        #[test]
        fn explorer_selects_a_file_in_its_folder_with_the_path_quoted() {
            let (bin, steps) = steps(Opener::Explorer, r"C:\work\my web\a,b.rs", None, Kind::File);
            assert_eq!(steps, [shell(None, explorer_in(&bin), Some(r#"/select,"C:\work\my web\a,b.rs""#))]);
        }

        #[test]
        fn explorer_selects_a_file_on_a_share_without_the_verbatim_prefix() {
            let (bin, steps) = steps(Opener::Explorer, r"\\?\UNC\server\share\a.rs", None, Kind::Program);
            assert_eq!(steps, [shell(None, explorer_in(&bin), Some(r#"/select,"\\server\share\a.rs""#))]);
        }

        #[test]
        fn explorer_explores_a_folder() {
            let (_bin, steps) = steps(Opener::Explorer, r"C:\", None, Kind::Folder);
            assert_eq!(steps, [shell(Some("explore"), r"C:\", None)]);
        }

        #[test]
        fn the_default_app_is_the_shells_own() {
            let (_bin, steps) = steps(Opener::Default, r"C:\work\web\a.png", None, Kind::File);
            assert_eq!(steps, [shell(None, r"C:\work\web\a.png", None)]);
        }

        #[test]
        fn the_default_app_never_runs_a_program() {
            let (_bin, openers) = windows(&ALL, vec![]);
            let refused = openers.steps(&open(Opener::Default, r"C:\work\web\run.exe", None), Some(Kind::Program));
            assert_eq!(refused.expect_err("refused"), "the default app opens only plain files");
        }

        #[test]
        fn a_port_opens_in_the_browser_through_the_shell() {
            let (_bin, openers) = windows(&ALL, vec![]);
            let url = Browse { ip: None, port: 3000 }.url();
            assert_eq!(openers.default_app(url.into()), Some(shell(None, "http://localhost:3000", None)));
        }
    }

    mod kinds {
        use super::*;

        #[rstest]
        #[case::a_program("run.exe", Some(Kind::Program))]
        #[case::a_script("build.CMD", Some(Kind::Program))]
        #[case::an_installer("Setup.msi", Some(Kind::Program))]
        #[case::a_shortcut("Docs.lnk", Some(Kind::Program))]
        #[case::text("notes.txt", Some(Kind::File))]
        #[case::no_extension("LICENSE", Some(Kind::File))]
        #[case::missing("gone.txt", None)]
        fn come_from_the_extension(#[case] name: &str, #[case] expected: Option<Kind>) {
            let tmp = TempDir::new();
            if name != "gone.txt" {
                std::fs::write(tmp.path().join(name), "").expect("write");
            }
            assert_eq!(Kind::of(&tmp.path().join(name)), expected);
        }

        #[test]
        fn a_folder_is_a_folder() {
            let tmp = TempDir::new();
            assert_eq!(Kind::of(tmp.path()), Some(Kind::Folder));
        }
    }

    mod running {
        use super::*;

        fn batch(script: &str) -> (TempDir, PathBuf) {
            let tmp = TempDir::new();
            let program = tmp.path().join("opener.cmd");
            std::fs::write(&program, script).expect("write");
            let program = PathBuf::from(plain(&program.to_string_lossy()).into_owned());
            (tmp, program)
        }

        #[test]
        fn a_failing_batch_file_reports_its_exit_status() {
            let (_tmp, program) = batch("@exit /b 3\r\n");
            assert_eq!(run(&[step(&program, &[r"C:\work\a b.rs".into()])]), Err("exit status 3".to_string()));
        }

        #[test]
        fn a_batch_file_gets_its_arguments_whole() {
            let (tmp, program) = batch("@echo %1> \"%~dp0said.txt\"\r\n");
            assert_eq!(run(&[step(&program, &[r"C:\work\my web\a&b %PATH%.rs:3".into()])]), Ok(()));
            let said = std::fs::read_to_string(tmp.path().join("said.txt")).expect("the batch file wrote");
            assert_eq!(said.trim_end(), r#""C:\work\my web\a&b %PATH%.rs:3""#);
        }

        #[test]
        fn a_missing_program_reports_the_spawn_error() {
            assert!(run(&[step(Path::new(r"C:\nonexistent\opener.exe"), &[])]).is_err());
        }
    }
}
