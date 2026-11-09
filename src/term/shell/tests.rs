#[cfg(unix)]
mod on_unix {
    use std::ffi::OsString;
    use std::path::Path;

    use super::super::imp::{listed, resolve_on};
    use super::super::{Found, resolve};
    use crate::test_util::{TempDir, write_executable};

    fn tool(dir: &Path, name: &str) -> String {
        let path = dir.join(name);
        write_executable(&path, "#!/bin/sh\n");
        path.to_string_lossy().into_owned()
    }

    fn data(dir: &Path, name: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, "").expect("write a plain file");
        path.to_string_lossy().into_owned()
    }

    fn path_of(dir: &Path) -> OsString {
        dir.as_os_str().to_owned()
    }

    #[test]
    fn the_listed_shells_skip_what_cannot_be_offered() {
        let dir = TempDir::new();
        let (zsh, fish) = (tool(dir.path(), "zsh"), tool(dir.path(), "fish"));
        let text = format!(
            "# a comment\n\n{zsh}\n{fish}\n{}\n{}\n{}\nrelative/bash\n{}/missing-bash\n",
            tool(dir.path(), "tmux"),
            tool(dir.path(), "rbash"),
            data(dir.path(), "bash"),
            dir.path().display()
        );

        let shells = listed(&text);

        let expected = [Found { value: zsh, note: "zsh".into() }, Found { value: fish, note: "fish".into() }];
        assert_eq!(shells, expected);
    }

    #[test]
    fn two_paths_to_one_file_are_listed_once() {
        let dir = TempDir::new();
        let bash = tool(dir.path(), "bash");
        let linked = dir.path().join("link");
        std::fs::create_dir(&linked).expect("make a folder");
        std::os::unix::fs::symlink(&bash, linked.join("bash")).expect("link the shell");

        let shells = listed(&format!("{bash}\n{}\n", linked.join("bash").display()));

        assert_eq!(shells, [Found { value: bash, note: "bash".into() }]);
    }

    #[test]
    fn an_absolute_executable_is_taken_as_it_is() {
        let dir = TempDir::new();
        let bash = tool(dir.path(), "bash");

        assert_eq!(resolve(&bash, None), Ok(bash.clone()));
        assert_eq!(resolve(&format!(" \"{bash}\" "), None), Ok(bash));
    }

    #[test]
    fn a_path_starting_with_a_tilde_follows_the_home() {
        let home = TempDir::new();
        let fish = tool(home.path(), "fish");

        assert_eq!(resolve("~/fish", Some(home.path())), Ok(fish));
    }

    #[test]
    fn a_name_is_found_on_the_given_path() {
        let (empty, tools) = (TempDir::new(), TempDir::new());
        let nu = tool(tools.path(), "nu");
        let path = std::env::join_paths([empty.path(), tools.path()]).expect("join the folders");

        assert_eq!(resolve_on("nu", Some(&path)), Ok(nu));
        assert_eq!(resolve_on("nu", Some(&path_of(empty.path()))), Err("it was not found on the PATH".into()));
    }

    #[test]
    fn what_cannot_start_says_why() {
        let dir = TempDir::new();
        let plain = data(dir.path(), "plain");

        assert_eq!(
            resolve("bin/bash", None),
            Err("use a full path, one starting with ~/, or a name on the PATH".into())
        );
        assert_eq!(resolve(&plain, None), Err("there is no program there".into()));
        assert_eq!(
            resolve(&format!("{}/missing", dir.path().display()), None),
            Err("there is no program there".into())
        );
        assert_eq!(resolve("", None), Err("no program was given".into()));
        assert!(resolve("ba\u{7}sh", None).is_err());
    }
}

#[cfg(windows)]
mod on_windows {
    use std::ffi::OsString;
    use std::path::Path;

    use rstest::rstest;

    use super::super::imp::{Env, fallback, found_in, resolve_on, startable};
    use crate::test_util::TempDir;

    fn program(dir: &Path, name: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, "").expect("write a program");
        path.to_string_lossy().into_owned()
    }

    fn folders(dirs: &[&Path]) -> OsString {
        std::env::join_paths(dirs).expect("join the folders")
    }

    #[test]
    fn a_shell_given_by_its_path_is_taken_with_or_without_its_exe() {
        let dir = TempDir::new();
        let bash = program(dir.path(), "bash.exe");

        let found = [startable(&bash, None), startable(bash.trim_end_matches(".exe"), None)];

        assert_eq!(found, [Some(bash.clone()), Some(bash)]);
    }

    #[test]
    fn a_quoted_shell_is_taken_without_its_quotes() {
        let dir = TempDir::new();
        let bash = program(dir.path(), "bash.exe");

        assert_eq!(startable(&format!(" \"{bash}\" "), None), Some(bash));
    }

    #[test]
    fn a_shell_given_by_its_name_is_found_on_the_path() {
        let (empty, tools) = (TempDir::new(), TempDir::new());
        let nu = program(tools.path(), "nu.exe");

        assert_eq!(startable("nu", Some(&folders(&[empty.path(), tools.path()]))), Some(nu));
    }

    #[test]
    fn a_shim_without_a_program_extension_is_passed_over() {
        let (shims, tools) = (TempDir::new(), TempDir::new());
        let _ = program(shims.path(), "nu");
        let _ = program(tools.path(), "nu");
        let nu = program(tools.path(), "nu.exe");

        assert_eq!(startable("nu", Some(&folders(&[shims.path(), tools.path()]))), Some(nu));
    }

    #[rstest]
    #[case::a_unix_path("/usr/bin/bash")]
    #[case::a_relative_path(r"bin\bash")]
    #[case::a_path_relative_to_a_drive("C:bash")]
    #[case::a_program_that_is_not_there(r"C:\nonexistent\bash.exe")]
    #[case::a_name_on_no_folder("nonexistent-shell")]
    #[case::a_batch_file("bash.cmd")]
    fn a_shell_windows_cannot_start_is_skipped(#[case] shell: &str) {
        let dir = TempDir::new();
        let _ = [program(dir.path(), "bash.exe"), program(dir.path(), "bash.cmd")];

        assert_eq!(startable(shell, Some(&folders(&[dir.path()]))), None);
    }

    #[test]
    fn pwsh_comes_before_windows_powershell() {
        let (old, new) = (TempDir::new(), TempDir::new());
        let _ = program(old.path(), "powershell.exe");
        let pwsh = program(new.path(), "pwsh.exe");

        assert_eq!(fallback(Some(&folders(&[old.path(), new.path()])), Some("cmd.exe")), pwsh);
    }

    #[test]
    fn windows_powershell_comes_before_comspec() {
        let dir = TempDir::new();
        let powershell = program(dir.path(), "powershell.exe");

        assert_eq!(fallback(Some(&folders(&[dir.path()])), Some("cmd.exe")), powershell);
    }

    #[rstest]
    #[case::comspec_without_powershell(Some(r"C:\Windows\system32\cmd.exe"), r"C:\Windows\system32\cmd.exe")]
    #[case::a_quoted_comspec(Some(r#""C:\Windows\system32\cmd.exe""#), r"C:\Windows\system32\cmd.exe")]
    #[case::cmd_for_a_comspec_that_is_not_there(Some(r"C:\nonexistent\cmd.exe"), "cmd.exe")]
    #[case::cmd_without_comspec(None, "cmd.exe")]
    #[case::cmd_with_an_empty_comspec(Some(""), "cmd.exe")]
    fn the_last_resort_is_the_command_interpreter(#[case] comspec: Option<&str>, #[case] expected: &str) {
        let dir = TempDir::new();

        assert_eq!(fallback(Some(&folders(&[dir.path()])), comspec), expected);
    }

    #[test]
    fn found_lists_the_shells_of_this_computer_and_leaves_out_what_is_missing() {
        let (system, git, programs) = (TempDir::new(), TempDir::new(), TempDir::new());
        std::fs::create_dir_all(git.path().join("cmd")).expect("make cmd");
        std::fs::create_dir_all(git.path().join("bin")).expect("make bin");
        let _ = program(&git.path().join("cmd"), "git.exe");
        let bash = program(&git.path().join("bin"), "bash.exe");
        let pwsh = program(programs.path(), "pwsh.exe");
        let cmd = program(system.path(), "cmd.exe");
        let path = folders(&[programs.path(), system.path(), &git.path().join("cmd")]);

        let shells = found_in(&Env { path: Some(&path), comspec: None, program_files: None, local_app_data: None });

        let values: Vec<&str> = shells.iter().map(|shell| shell.value.as_str()).collect();
        assert_eq!(values, ["pwsh", "cmd", bash.as_str()]);
        assert!(shells[0].note.contains(&pwsh) && shells[1].note.contains(&cmd), "{shells:?}");
    }

    #[test]
    fn git_bash_is_found_in_the_usual_folder_without_git_on_the_path() {
        let files = TempDir::new();
        let root = files.path().join("Git");
        std::fs::create_dir_all(root.join("cmd")).expect("make cmd");
        std::fs::create_dir_all(root.join("bin")).expect("make bin");
        let _ = program(&root.join("cmd"), "git.exe");
        let bash = program(&root.join("bin"), "bash.exe");
        let program_files = files.path().as_os_str().to_owned();

        let shells =
            found_in(&Env { path: None, comspec: None, program_files: Some(&program_files), local_app_data: None });

        assert_eq!(shells.len(), 1);
        assert_eq!((shells[0].value.as_str(), shells[0].note.as_str()), (bash.as_str(), "Git Bash"));
    }

    #[test]
    fn a_batch_file_a_network_path_and_wsl_are_refused_with_their_reasons() {
        let (dir, system) = (TempDir::new(), TempDir::new());
        let batch = program(dir.path(), "bash.cmd");
        let wsl = program(dir.path(), "wsl.exe");
        std::fs::create_dir_all(system.path().join("System32")).expect("make System32");
        let system_bash = program(&system.path().join("System32"), "bash.exe");
        let own = program(dir.path(), "bash.exe");

        assert_eq!(resolve_on(&batch, None, None), Err("Windows starts only an .exe or a .com".into()));
        assert!(resolve_on(r"\\host\share\x.exe", None, None).is_err_and(|why| why.contains("network")));
        assert!(resolve_on(&wsl, None, None).is_err_and(|why| why.contains("WSL")));
        assert!(resolve_on(&system_bash, None, Some(system.path())).is_err_and(|why| why.contains("WSL")));
        assert_eq!(resolve_on(&own, None, Some(system.path())), Ok(own));
    }
}
