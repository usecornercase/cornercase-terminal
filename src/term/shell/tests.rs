#[cfg(windows)]
mod on_windows {
    use std::ffi::OsString;
    use std::path::Path;

    use rstest::rstest;

    use super::super::imp::{fallback, startable};
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
    fn a_shell_given_by_its_name_is_found_on_the_path() {
        let (empty, tools) = (TempDir::new(), TempDir::new());
        let nu = program(tools.path(), "nu.exe");

        assert_eq!(startable("nu", Some(&folders(&[empty.path(), tools.path()]))), Some(nu));
    }

    #[rstest]
    #[case::a_unix_path("/usr/bin/bash")]
    #[case::a_relative_path(r"bin\bash")]
    #[case::a_path_relative_to_a_drive("C:bash")]
    #[case::a_program_that_is_not_there(r"C:\nonexistent\bash.exe")]
    #[case::a_name_on_no_folder("nonexistent-shell")]
    fn a_shell_windows_cannot_start_is_skipped(#[case] shell: &str) {
        let dir = TempDir::new();
        let _ = program(dir.path(), "bash.exe");

        assert_eq!(startable(shell, Some(&folders(&[dir.path()]))), None);
    }

    #[test]
    fn pwsh_comes_before_windows_powershell() {
        let (old, new) = (TempDir::new(), TempDir::new());
        let _ = program(old.path(), "powershell.exe");
        let pwsh = program(new.path(), "pwsh.exe");

        assert_eq!(fallback(Some(&folders(&[old.path(), new.path()])), Some("cmd.exe".into())), pwsh);
    }

    #[test]
    fn windows_powershell_comes_before_comspec() {
        let dir = TempDir::new();
        let powershell = program(dir.path(), "powershell.exe");

        assert_eq!(fallback(Some(&folders(&[dir.path()])), Some("cmd.exe".into())), powershell);
    }

    #[rstest]
    #[case::comspec_without_powershell(Some(r"C:\Windows\system32\cmd.exe"), r"C:\Windows\system32\cmd.exe")]
    #[case::cmd_without_comspec(None, "cmd.exe")]
    #[case::cmd_with_an_empty_comspec(Some(""), "cmd.exe")]
    fn the_last_resort_is_the_command_interpreter(#[case] comspec: Option<&str>, #[case] expected: &str) {
        let dir = TempDir::new();

        assert_eq!(fallback(Some(&folders(&[dir.path()])), comspec.map(str::to_string)), expected);
    }
}
