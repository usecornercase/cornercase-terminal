use rstest::rstest;

use super::*;

mod probe {
    use super::*;

    #[rstest]
    #[case::cmd("cornercase-shell-\"x\"-$true\r\n", Some(RemoteShell::Cmd))]
    #[case::powershell("cornercase-shell-x-True\r\n", Some(RemoteShell::PowerShell))]
    #[case::sh("cornercase-shell-x-\n", Some(RemoteShell::Posix))]
    #[case::fish_drops_the_word("\n", None)]
    #[case::a_motd("Welcome to devbox\n", None)]
    #[case::something_else("cornercase-shell-x-yes\n", None)]
    fn each_shell_answers_in_its_own_way(#[case] line: &str, #[case] shell: Option<RemoteShell>) {
        assert_eq!(heard(line), shell);
    }

    #[test]
    fn runs_nothing_but_echo() {
        assert!(PROBE.starts_with("echo ") && !PROBE.contains(['`', '(', ';', '&', '|', '<', '>', '\'']), "{PROBE}");
    }
}

mod cmd {
    use super::*;

    #[test]
    fn looks_for_cornercase_where_the_installers_put_it_and_says_when_it_is_missing() {
        assert_eq!(
            cmd_line(None, &["proxy"], false).expect("a line"),
            r#"set "PATH=%PATH%;%USERPROFILE%\.local\bin;%USERPROFILE%\.cargo\bin" && ("%SystemRoot%\System32\where.exe" /q cornercase || exit 127) && cornercase proxy"#
        );
    }

    #[test]
    fn a_tied_command_sets_the_variable_first() {
        let line = cmd_line(None, &["restart", "--when-idle"], true).expect("a line");

        assert!(line.contains(r#"&& set "CORNERCASE_TIED=1" && ("#), "{line}");
        assert!(line.ends_with("&& cornercase restart --when-idle"), "{line}");
    }

    #[test]
    fn never_starts_with_a_quote_which_cmd_would_strip() {
        assert!(
            !cmd_line(Some(r"C:\Program Files\cc\cornercase.exe"), &["proxy"], false).expect("a line").starts_with('"')
        );
    }

    #[rstest]
    #[case::a_path_with_a_space(
        r"C:\Program Files\cc\cornercase.exe",
        r#""C:\Program Files\cc\:cornercase.exe""#,
        r#""C:\Program Files\cc\cornercase.exe" proxy"#
    )]
    #[case::forward_slashes("C:/cc/cornercase.exe", r#""C:\cc\:cornercase.exe""#, r"C:\cc\cornercase.exe proxy")]
    #[case::the_profile_folder(
        "~/bin/cornercase.exe",
        r#""%USERPROFILE%\bin\:cornercase.exe""#,
        r#""%USERPROFILE%\bin\cornercase.exe" proxy"#
    )]
    #[case::a_name("cc", "/q cc ||", "&& cc proxy")]
    fn runs_the_given_program_and_looks_for_it_first(#[case] given: &str, #[case] found: &str, #[case] run: &str) {
        let line = cmd_line(Some(given), &["proxy"], false).expect("a line");

        assert!(line.contains(found) && line.ends_with(run), "{line}");
    }

    #[rstest]
    #[case::a_space("a b", r#""a b""#)]
    #[case::metacharacters("x&y|z<w>v^u(t)", r#""x&y|z<w>v^u(t)""#)]
    #[case::a_trailing_backslash(r"C:\my dir\", r#""C:\my dir\\""#)]
    #[case::plain("--lines", "--lines")]
    fn quotes_a_word_for_cmd_and_the_c_runtime(#[case] word: &str, #[case] quoted: &str) {
        let line = cmd_line(None, &[word], false).expect("a line");

        assert!(line.ends_with(&format!("cornercase {quoted}")), "{line}");
    }

    #[rstest]
    #[case::a_percent("50%")]
    #[case::a_bang("hi!")]
    #[case::a_quote("say \"hi\"")]
    #[case::a_new_line("a\nb")]
    fn refuses_what_cmd_would_read(#[case] word: &str) {
        assert!(cmd_line(None, &[word], false).is_err());
        assert!(cmd_line(Some(word), &["proxy"], false).is_err());
    }
}

mod powershell {
    use super::*;

    #[test]
    fn looks_for_cornercase_where_the_installers_put_it_and_says_when_it_is_missing() {
        assert_eq!(
            powershell_line(None, &["proxy"], false).expect("a line"),
            "$env:PATH = $env:PATH + ';' + $env:USERPROFILE + '\\.local\\bin' + ';' + $env:USERPROFILE + \
             '\\.cargo\\bin'; try { & 'cornercase' 'proxy' } catch [System.Management.Automation.\
             CommandNotFoundException] { exit 127 }; exit $LASTEXITCODE"
        );
    }

    #[test]
    fn a_tied_command_sets_the_variable_first() {
        let line = powershell_line(None, &["restart"], true).expect("a line");

        assert!(line.contains("; $env:CORNERCASE_TIED = '1'; try {"), "{line}");
    }

    #[rstest]
    #[case::a_path_with_a_quote(r"C:\O'Brien\cornercase.exe", r"& 'C:\O''Brien\cornercase.exe' 'proxy'")]
    #[case::forward_slashes("C:/cc/cornercase.exe", r"& 'C:\cc\cornercase.exe' 'proxy'")]
    #[case::the_profile_folder("~\\bin\\cornercase.exe", r"& ($env:USERPROFILE + '\bin\cornercase.exe') 'proxy'")]
    fn runs_the_given_program(#[case] given: &str, #[case] run: &str) {
        let line = powershell_line(Some(given), &["proxy"], false).expect("a line");

        assert!(line.contains(run), "{line}");
    }

    #[test]
    fn a_word_with_a_typographic_quote_stays_one_word() {
        let line = powershell_line(None, &["it\u{2019}s"], false).expect("a line");

        assert!(line.contains("'it\u{2019}\u{2019}s'"), "{line}");
    }

    #[rstest]
    #[case::empty("")]
    #[case::stop_parsing("--%")]
    #[case::a_quote("say \"hi\"")]
    #[case::a_space_and_a_trailing_backslash(r"C:\my dir\")]
    #[case::two_spaces("a  b")]
    #[case::a_new_line("a\nb")]
    fn refuses_what_windows_powershell_would_split(#[case] word: &str) {
        assert!(powershell_line(None, &[word], false).is_err());
    }
}

#[cfg(windows)]
mod on_windows {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use super::*;
    use crate::test_util::{TempDir, fake_program};

    #[derive(Clone, Copy)]
    enum Session {
        Plain,
        Terminal,
    }

    fn windows_folder() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"))
    }

    fn powershell() -> PathBuf {
        windows_folder().join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
    }

    fn like_sshd<'a>(command: &'a mut Command, home: &Path) -> &'a mut Command {
        let windows = windows_folder();
        let path = std::env::join_paths([windows.join("System32"), windows.clone(), powershell().with_file_name("")])
            .expect("a PATH");
        command.env("PATH", path).env("USERPROFILE", home).env_remove(TIED_ENV).current_dir(home)
    }

    fn through(shell: RemoteShell, line: &str, home: &Path, session: Session) -> Output {
        let mut command = match shell {
            RemoteShell::Cmd => Command::new(windows_folder().join(r"System32\cmd.exe")),
            _ => Command::new(powershell()),
        };
        match (shell, session) {
            (RemoteShell::Cmd, Session::Plain) => command.raw_arg(format!("/c \"{line}\"")),
            (RemoteShell::Cmd, Session::Terminal) => command.raw_arg(format!("/c {line}")),
            (_, Session::Plain) => command.args(["-NoProfile", "-NonInteractive", "-c", line]),
            (_, Session::Terminal) => command.raw_arg(format!("-NoProfile -NonInteractive -c {line}")),
        };
        like_sshd(&mut command, home).output().expect("run the shell")
    }

    fn line(shell: RemoteShell, given: Option<&str>, args: &[&str], tied: bool) -> String {
        let line =
            if shell == RemoteShell::Cmd { cmd_line(given, args, tied) } else { powershell_line(given, args, tied) };
        line.expect("a line")
    }

    fn installed(home: &Path, folder: &str) -> PathBuf {
        let folder = home.join(folder);
        std::fs::create_dir_all(&folder).expect("the install folder");
        fake_program(&folder, PROGRAM)
    }

    fn recorded(program: &Path) -> Vec<String> {
        let args = std::fs::read_to_string(program.with_extension("args")).unwrap_or_default();
        args.lines().map(str::to_string).collect()
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd)]
    #[case::powershell(RemoteShell::PowerShell)]
    fn the_probe_tells_the_shell_apart(#[case] shell: RemoteShell) {
        let home = TempDir::new();

        let out = through(shell, PROBE, home.path(), Session::Plain);

        let said = String::from_utf8_lossy(&out.stdout);
        assert_eq!(said.lines().find_map(heard), Some(shell), "{out:?}");
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd, Session::Plain)]
    #[case::cmd_in_a_terminal(RemoteShell::Cmd, Session::Terminal)]
    #[case::powershell(RemoteShell::PowerShell, Session::Plain)]
    #[case::powershell_in_a_terminal(RemoteShell::PowerShell, Session::Terminal)]
    fn cornercase_is_found_where_the_installer_puts_it(#[case] shell: RemoteShell, #[case] session: Session) {
        let home = TempDir::new();
        let program = installed(home.path(), r".local\bin");

        let out = through(shell, &line(shell, None, &["proxy"], false), home.path(), session);

        assert!(out.status.success(), "{out:?}");
        assert_eq!(recorded(&program), ["proxy"]);
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd)]
    #[case::powershell(RemoteShell::PowerShell)]
    fn the_exit_status_comes_through(#[case] shell: RemoteShell) {
        let home = TempDir::new();
        let program = installed(home.path(), r".cargo\bin");
        std::fs::write(program.with_extension("code"), "3").expect("the exit code");

        let out = through(shell, &line(shell, None, &["logs"], false), home.path(), Session::Plain);

        assert_eq!(out.status.code(), Some(3), "{out:?}");
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd, None)]
    #[case::cmd_given_a_path(RemoteShell::Cmd, Some(r"C:\nonexistent\cornercase.exe"))]
    #[case::powershell(RemoteShell::PowerShell, None)]
    #[case::powershell_given_a_path(RemoteShell::PowerShell, Some(r"C:\nonexistent\cornercase.exe"))]
    fn a_missing_cornercase_exits_as_sh_does(#[case] shell: RemoteShell, #[case] given: Option<&str>) {
        let home = TempDir::new();

        let out = through(shell, &line(shell, given, &["proxy"], false), home.path(), Session::Plain);

        assert_eq!(out.status.code(), Some(NOT_FOUND), "{out:?}");
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd, r"C:\my dir\")]
    #[case::powershell(RemoteShell::PowerShell, r"C:\dir\")]
    fn words_reach_the_program_as_they_were(#[case] shell: RemoteShell, #[case] folder: &str) {
        let home = TempDir::new();
        let program = installed(home.path(), "my tools");
        let given = program.display().to_string();
        let words = ["x&y|z", "(a)", "caret^", "<lt>", "it's", "a b", folder];

        let out = through(shell, &line(shell, Some(&given), &words, false), home.path(), Session::Plain);

        assert!(out.status.success(), "{out:?}");
        assert_eq!(recorded(&program), [words.join(" ")]);
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd)]
    #[case::powershell(RemoteShell::PowerShell)]
    fn the_profile_folder_is_written_with_a_tilde(#[case] shell: RemoteShell) {
        let home = TempDir::new();
        let program = installed(home.path(), "bin");

        let out =
            through(shell, &line(shell, Some("~/bin/cornercase.exe"), &["proxy"], false), home.path(), Session::Plain);

        assert!(out.status.success(), "{out:?}");
        assert_eq!(recorded(&program), ["proxy"]);
    }

    #[rstest]
    #[case::cmd(RemoteShell::Cmd)]
    #[case::powershell(RemoteShell::PowerShell)]
    fn a_tied_command_finds_the_variable(#[case] shell: RemoteShell) {
        let home = TempDir::new();
        let cmd = windows_folder().join(r"System32\cmd.exe").display().to_string();
        let shown = |tied| {
            let out = through(
                shell,
                &line(shell, Some(&cmd), &["/d", "/c", "set", TIED_ENV], tied),
                home.path(),
                Session::Plain,
            );
            String::from_utf8_lossy(&out.stdout).into_owned()
        };

        assert!(shown(true).contains(&format!("{TIED_ENV}=1")));
        assert!(!shown(false).contains(&format!("{TIED_ENV}=1")));
    }
}
