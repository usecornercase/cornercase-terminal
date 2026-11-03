use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

#[cfg(unix)]
mod on_unix {
    use super::*;
    use crate::test_util::write_executable;

    fn bin() -> TempDir {
        let dir = TempDir::new();
        write_executable(&dir.path().join("codex"), "#!/bin/sh\n");
        std::fs::write(dir.path().join("notes"), "").expect("write a plain file");
        dir
    }

    fn found(program: &str, path: Option<&OsStr>) -> bool {
        find(false, program, path, &[]).is_some()
    }

    #[test]
    fn a_program_is_looked_up_in_the_path() {
        let dir = bin();
        let path = std::env::join_paths(["/nonexistent", dir.path().to_str().expect("utf-8")]).expect("a path");
        assert_eq!(
            [found("codex", Some(&path)), found("claude", Some(&path)), found("notes", Some(&path))],
            [true, false, false]
        );
    }

    #[test]
    fn a_program_with_a_slash_is_checked_as_is() {
        let dir = bin();
        let codex = dir.path().join("codex").display().to_string();
        assert_eq!([found(&codex, None), found("/nonexistent/codex", None)], [true, false]);
    }

    #[test]
    fn without_a_path_nothing_is_found() {
        assert!(!found("sh", None));
    }

    #[test]
    fn a_name_is_started_as_it_is() {
        assert_eq!(startable("claude", None, &own_extensions()), OsString::from("claude"));
    }
}

mod for_windows {
    use super::*;

    fn npm_folder() -> TempDir {
        let dir = TempDir::new();
        for (name, text) in [("claude", "#!/bin/sh\n"), ("claude.ps1", "#!/usr/bin/env pwsh\n"), ("claude.cmd", SHIM)] {
            std::fs::write(dir.path().join(name), text).expect("write a shim");
        }
        dir
    }

    const SHIM: &str = "@echo off\r\necho shim %*\r\n";

    fn path_with(dir: &TempDir) -> OsString {
        std::env::join_paths([dir.path().join("nonexistent"), dir.path().to_path_buf()]).expect("a path")
    }

    #[rstest]
    #[case::unset(None, &[".com", ".exe", ".bat", ".cmd"])]
    #[case::only_what_command_runs(Some(".VBS;.CMD;.PS1;.EXE"), &[".cmd", ".exe"])]
    #[case::nothing_runnable(Some(".PS1"), &[".com", ".exe", ".bat", ".cmd"])]
    fn reads_the_runnable_extensions_of_pathext(#[case] pathext: Option<&str>, #[case] expected: &[&str]) {
        assert_eq!(extensions(pathext), expected);
    }

    #[test]
    fn an_npm_shim_is_found_through_pathext_and_never_its_sh_or_ps1_shim() {
        let dir = npm_folder();
        let path = path_with(&dir);
        let extensions = extensions(Some(".COM;.EXE;.BAT;.CMD;.PS1"));
        let find = |program: &str| find(true, program, Some(&path), &extensions);

        assert_eq!(find("claude"), Some(dir.path().join("claude.cmd")));
        assert_eq!(find("claude.cmd"), Some(dir.path().join("claude.cmd")));
        assert_eq!((find("claude.ps1"), find("codex")), (None, None));
    }

    #[test]
    fn a_path_without_its_extension_finds_the_shim_beside_it() {
        let dir = npm_folder();
        let bare = dir.path().join("claude").display().to_string();
        assert_eq!(find(true, &bare, None, &extensions(None)), Some(dir.path().join("claude.cmd")));
    }

    #[test]
    fn a_program_that_is_nowhere_keeps_its_name() {
        let dir = npm_folder();
        assert_eq!(startable("codex", Some(&path_with(&dir)), &extensions(None)), OsString::from("codex"));
    }

    #[cfg(windows)]
    #[test]
    fn an_npm_shim_starts_through_cmd_with_every_argument() {
        let dir = npm_folder();
        let program = startable("claude", Some(&path_with(&dir)), &extensions(None));
        assert_eq!(program, dir.path().join("claude.cmd").into_os_string());

        let output = Command::new(program).args(["-p", "--setting-sources", "", "--verbose"]).output().expect("run");
        let printed = String::from_utf8_lossy(&output.stdout);

        assert!(output.status.success(), "{output:?}");
        assert_eq!(printed.trim_end(), r#"shim -p --setting-sources "" --verbose"#);
    }

    #[cfg(windows)]
    #[test]
    fn a_path_with_forward_slashes_finds_the_shim() {
        let dir = npm_folder();
        let bare = dir.path().join("claude").display().to_string().replace('\\', "/");
        assert_eq!(find(true, &bare, None, &extensions(None)), Some(dir.path().join("claude.cmd")));
    }
}
