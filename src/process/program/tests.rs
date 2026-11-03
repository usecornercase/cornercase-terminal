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
    use crate::test_util::{NPM_SCRIPT, NPM_SHIM, npm_install, under};

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
    fn powershell_runs_the_ps1_shim_before_the_cmd_in_a_folder() {
        let dir = npm_folder();
        let found = powershell_finds("claude", Some(&path_with(&dir)), None);
        assert_eq!(found, Some(dir.path().join("claude.ps1")));
    }

    #[test]
    fn powershell_runs_the_first_folder_that_has_the_program() {
        let (batch, script) = (TempDir::new(), TempDir::new());
        std::fs::write(batch.path().join("claude.cmd"), SHIM).expect("write the cmd shim");
        std::fs::write(script.path().join("claude.ps1"), "").expect("write the ps1 shim");
        let path = std::env::join_paths([batch.path(), script.path()]).expect("a path");
        assert_eq!(powershell_finds("claude", Some(&path), None), Some(batch.path().join("claude.cmd")));
        assert_eq!(powershell_finds("codex", Some(&path), None), None);
    }

    #[test]
    fn a_cmd_shim_alone_refuses_a_word_cmd_reads_and_a_ps1_beside_it_lets_it_through() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("claude.cmd"), SHIM).expect("write the cmd shim");
        let path = dir.path().as_os_str().to_owned();
        let runs = |program: &str| powershell_finds(program, Some(&path), None);
        let line = r"claude --add-dir 'C:\Users\me\R&D'";
        assert!(crate::agents::without_cmd(line, runs).is_err_and(|refused| refused.contains("claude.cmd")));
        std::fs::write(dir.path().join("claude.ps1"), "").expect("write the ps1 shim");
        assert_eq!(crate::agents::without_cmd(line, runs), Ok(line.to_string()));
    }

    #[test]
    fn an_npm_shim_alone_runs_with_the_node_powershell_finds_unless_that_is_a_batch_file() {
        let (npm, nodejs, nvm) = (TempDir::new(), TempDir::new(), TempDir::new());
        npm_install(npm.path(), NPM_SHIM, &[&NPM_SCRIPT]);
        std::fs::write(nodejs.path().join("node.exe"), "").expect("write node");
        std::fs::write(nvm.path().join("node.cmd"), SHIM).expect("write a node shim");
        let typed = |folders: &[&TempDir]| {
            let path = std::env::join_paths(folders.iter().map(|dir| dir.path())).expect("a path");
            crate::agents::without_cmd("claude 'R&D'", |program| powershell_finds(program, Some(&path), None))
        };
        let words = [nodejs.path().join("node.exe"), under(npm.path(), &NPM_SCRIPT)].map(|p| p.display().to_string());
        let line = typed(&[&npm, &nodejs]).expect("typed past the shim");
        let at = |word: &str| line.find(word).unwrap_or(usize::MAX);
        assert!(at(&words[0]) < at(&words[1]) && at(&words[1]) < line.len() && line.ends_with(" 'R&D'"), "{line}");
        assert!(typed(&[&npm, &nvm, &nodejs]).is_err_and(|refused| refused.contains("claude.cmd")));
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
