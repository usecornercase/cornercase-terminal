use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn git(dir: &TempDir) -> Sh {
    Sh { program: dir.path().join("usr").join("bin").join("sh.exe").to_string_lossy().into_owned(), git: true }
}

fn file(dir: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, text).expect("write a file");
    path
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn env<'a>(line: &'a Line, key: &str) -> Option<&'a str> {
    line.env.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
}

mod picking {
    use super::*;

    #[test]
    fn git_s_sh_comes_first() {
        let dir = TempDir::new();
        let sh = dir.path().join("sh.exe");

        let program = sh.to_string_lossy().into_owned();
        let pwsh = dir.path().join("pwsh.exe");
        assert_eq!(Shell::pick(Some(&sh), Some(&pwsh), Some(dir.path())), Shell::Sh(Sh { program, git: true }));
    }

    #[test]
    fn without_it_powershell_7_runs_scripts() {
        let pwsh = PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe");
        let windows = Path::new(r"C:\Windows");

        let picked = Shell::pick(None, Some(&pwsh), Some(windows));

        assert_eq!(picked, Shell::PowerShell(pwsh.to_string_lossy().into_owned()));
    }

    #[test]
    fn without_either_windows_powershell_runs_from_the_windows_folder() {
        let windows = Path::new(r"C:\Windows");
        let expected = windows.join(r"System32\WindowsPowerShell\v1.0\powershell.exe");

        assert_eq!(Shell::pick(None, None, Some(windows)), Shell::PowerShell(expected.to_string_lossy().into_owned()));
    }

    #[test]
    fn only_a_windows_folder_nobody_could_read_leaves_the_bare_name() {
        assert_eq!(Shell::pick(None, None, None), Shell::PowerShell("powershell.exe".into()));
    }
}

mod running_a_script {
    use super::*;

    #[test]
    fn on_unix_is_sh_stopping_at_the_first_failure() {
        let line = Shell::sh().run("make\nmake test");

        assert_eq!(line, Line { program: "/bin/sh".into(), args: strings(&["-ec", "make\nmake test"]), env: vec![] });
    }

    #[test]
    fn through_git_s_sh_hands_the_script_over_in_the_environment() {
        let dir = TempDir::new();
        let sh = git(&dir);

        let line = Shell::Sh(sh.clone()).run("rm -rf 'it''s'");

        assert_eq!(line.program, sh.program);
        assert_eq!(line.args, strings(&["-ec", ADAPTER, "sh"]));
        assert_eq!(env(&line, SCRIPT_ENV), Some("rm -rf 'it''s'"));
        assert_eq!(env(&line, ARGS_ENV), Some(""));
    }

    #[test]
    fn through_git_s_sh_puts_its_folder_first_on_the_path() {
        let dir = TempDir::new();
        let sh = git(&dir);

        let line = Shell::Sh(sh.clone()).run("ls");

        let path = env(&line, "PATH").expect("a PATH");
        let first = std::env::split_paths(path).next().expect("a first folder");
        assert_eq!(Some(first.as_path()), Path::new(&sh.program).parent());
    }

    #[test]
    fn through_powershell_stops_at_the_first_error() {
        let line = Shell::PowerShell("pwsh.exe".into()).run("npm ci");

        let expected = strings(&["-NoProfile", "-Command", &format!("{STRICT}npm ci")]);
        assert_eq!(line, Line { program: "pwsh.exe".into(), args: expected, env: vec![] });
    }
}

mod a_line_for_sh {
    use super::*;

    #[test]
    fn on_unix_passes_its_arguments_as_they_are() {
        let Shell::Sh(sh) = Shell::sh() else { panic!("sh") };

        let line = sh.line("-c", "echo \"$1\"", &strings(&["name", "it's"]));

        assert_eq!((line.args, line.env), (strings(&["-c", "echo \"$1\"", "name", "it's"]), vec![]));
    }

    #[test]
    fn through_git_s_sh_quotes_each_argument_for_set() {
        let dir = TempDir::new();

        let line = git(&dir).line("-c", "echo \"$1\"", &strings(&["name", "a b", "it's", ""]));

        assert_eq!(line.args, strings(&["-c", ADAPTER, "name"]));
        assert_eq!(env(&line, ARGS_ENV), Some(r"'a b' 'it'\''s' ''"));
        assert_eq!(env(&line, SCRIPT_ENV), Some("echo \"$1\""));
    }
}

mod powershell_text {
    use super::*;

    #[rstest]
    #[case::plain("npm ci", "'npm ci'")]
    #[case::a_quote("it's", "'it''s'")]
    #[case::typographic_quotes("\u{2018}x\u{2019}", "'\u{2018}\u{2018}x\u{2019}\u{2019}'")]
    #[case::a_path(r"C:\Users\me\it's here", r"'C:\Users\me\it''s here'")]
    fn a_literal_doubles_every_single_quote(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(literal(text), expected);
    }

    #[test]
    fn an_encoded_command_is_utf16_in_base64() {
        assert_eq!(encoded("a"), "YQA=");
        assert_eq!(encoded("ñ"), "8QA=");
    }
}

mod plugin_programs {
    use super::*;

    #[test]
    fn a_script_runs_through_git_s_sh_with_its_arguments() {
        let dir = TempDir::new();
        let script = file(&dir, "run", "#!/bin/sh\necho hi\n");
        let sh = git(&dir);

        let line = Shell::Sh(sh.clone()).interpreted(&script, &strings(&["open", "a b"]), None).expect("interpreted");

        assert_eq!(line.args, strings(&["-c", ADAPTER, "sh"]));
        assert_eq!(env(&line, SCRIPT_ENV), Some(EXEC));
        let quoted = format!("'{}' 'open' 'a b'", ffi::plain(&script).display());
        assert_eq!(env(&line, ARGS_ENV), Some(quoted.as_str()));
        assert!(env(&line, "PATH").is_some());
    }

    #[test]
    fn a_script_with_a_log_opens_it_itself_for_appending() {
        let dir = TempDir::new();
        let (script, log) = (file(&dir, "run", "#!/bin/sh\n"), dir.path().join("it's.log"));

        let line = Shell::Sh(git(&dir)).interpreted(&script, &[], Some(&log)).expect("interpreted");

        let expected =
            format!(r#"exec "$@" >>'{}' 2>&1"#, ffi::plain(&log).display().to_string().replace('\'', r"'\''"));
        assert_eq!(env(&line, SCRIPT_ENV), Some(expected.as_str()));
    }

    #[test]
    fn a_program_is_started_as_it_is() {
        let dir = TempDir::new();
        let program = file(&dir, "run.exe", "MZ");

        assert_eq!(Shell::Sh(git(&dir)).interpreted(&program, &[], None), None);
    }

    #[test]
    fn on_unix_nothing_is_interpreted() {
        let dir = TempDir::new();
        let script = file(&dir, "run", "#!/bin/sh\n");

        assert_eq!(Shell::sh().interpreted(&script, &[], None), None);
        assert_eq!(Shell::sh().refuses(&script), None);
    }

    #[test]
    fn without_git_s_sh_a_script_is_refused_and_a_program_is_not() {
        let dir = TempDir::new();
        let (script, program) = (file(&dir, "run", "#!/bin/sh\n"), file(&dir, "run.exe", "MZ"));
        let powershell = Shell::PowerShell("powershell.exe".into());

        assert_eq!(powershell.interpreted(&script, &[], None), None);
        assert_eq!(powershell.refuses(&script), Some(NOT_RUNNABLE));
        assert_eq!(powershell.refuses(&program), None);
        assert_eq!(powershell.refuses(&dir.path().join("missing")), None);
    }
}

#[cfg(windows)]
mod on_windows {
    use std::process::Output;

    use super::*;

    fn through_git() -> Shell {
        let sh = git_sh().expect("Git for Windows' sh, which these tests need");
        Shell::pick(Some(&sh), None, None)
    }

    mod finding_git_s_sh {
        use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};

        use super::*;
        use crate::os::fs::{make_reparse_point, share_with_everyone};

        const PRIVILEGE_NOT_HELD: i32 = 1314;
        const INSTALL: [&str; 5] =
            [r"cmd\git.exe", r"bin\git.exe", r"mingw64\bin\git.exe", r"mingw64\libexec\git-core\git.exe", SH_EXE];
        const SH_EXE: &str = r"usr\bin\sh.exe";

        fn files(dir: &TempDir, names: &[&str]) {
            for name in names {
                let path = dir.path().join(name);
                std::fs::create_dir_all(path.parent().expect("its folder")).expect("its folder");
                std::fs::write(&path, "").expect("a file");
            }
        }

        #[test]
        fn git_s_exec_path_leads_to_its_install() {
            let dir = TempDir::new();
            files(&dir, &INSTALL);

            let root = exec_path_root(&dir.path().join(r"mingw64\libexec\git-core"));

            assert_eq!(root.as_deref().and_then(sh_in), Some(dir.path().join(SH_EXE)));
            assert_eq!(exec_path_root(dir.path()), None);
        }

        #[rstest]
        #[case::cmd(r"cmd\git.exe")]
        #[case::its_launcher(r"bin\git.exe")]
        #[case::mingw(r"mingw64\bin\git.exe")]
        fn git_s_own_folder_leads_to_its_install(#[case] git: &str) {
            let dir = TempDir::new();
            files(&dir, &INSTALL);

            let root = git_root(&dir.path().join(git));

            assert_eq!(root.as_deref().and_then(sh_in), Some(dir.path().join(SH_EXE)));
        }

        #[rstest]
        #[case::a_scoop_shim(r"scoop\shims\git.exe")]
        #[case::a_shim_folder_named_bin(r"chocolatey\bin\git.exe")]
        fn a_shim_never_leads_to_an_sh_above_it(#[case] shim: &str) {
            let dir = TempDir::new();
            files(&dir, &[shim, r"bin\sh.exe", SH_EXE]);

            assert_eq!(git_root(&dir.path().join(shim)), None);
        }

        #[test]
        fn the_launcher_is_taken_when_there_is_no_other() {
            let dir = TempDir::new();
            files(&dir, &[r"cmd\git.exe", r"bin\sh.exe"]);

            assert_eq!(sh_in(dir.path()), Some(dir.path().join(r"bin\sh.exe")));
        }

        #[test]
        fn an_sh_others_can_write_to_is_refused() {
            let dir = TempDir::new();
            files(&dir, &[r"cmd\git.exe", SH_EXE]);
            share_with_everyone(&dir.path().join(SH_EXE), FILE_GENERIC_READ | FILE_GENERIC_WRITE).expect("shared");

            assert_eq!(sh_in(dir.path()), None);
        }

        #[test]
        fn an_sh_that_is_a_reparse_point_is_refused() {
            let dir = TempDir::new();
            files(&dir, &[r"cmd\git.exe", SH_EXE]);
            assert_eq!(sh_in(dir.path()), Some(dir.path().join(SH_EXE)));

            make_reparse_point(&dir.path().join(SH_EXE)).expect("a reparse point");

            assert_eq!(sh_in(dir.path()), None);
        }

        fn writable_elsewhere(dir: &TempDir) -> PathBuf {
            files(dir, &[r"cmd\git.exe", r"elsewhere\sh.exe"]);
            let elsewhere = dir.path().join(r"elsewhere\sh.exe");
            share_with_everyone(&elsewhere, FILE_GENERIC_READ | FILE_GENERIC_WRITE).expect("shared");
            std::fs::create_dir_all(dir.path().join(r"usr\bin")).expect("its folder");
            elsewhere
        }

        #[test]
        fn a_hard_link_to_an_sh_others_can_write_to_is_refused() {
            let dir = TempDir::new();
            let elsewhere = writable_elsewhere(&dir);

            std::fs::hard_link(&elsewhere, dir.path().join(SH_EXE)).expect("a hard link");

            assert_eq!(sh_in(dir.path()), None);
        }

        #[test]
        fn a_symbolic_link_to_an_sh_others_can_write_to_is_refused() {
            let dir = TempDir::new();
            let elsewhere = writable_elsewhere(&dir);

            match std::os::windows::fs::symlink_file(&elsewhere, dir.path().join(SH_EXE)) {
                Err(e) if e.raw_os_error() == Some(PRIVILEGE_NOT_HELD) => {
                    eprintln!("skipped: a symbolic link needs Developer Mode or an administrator ({e})");
                }
                made => {
                    made.expect("a symbolic link");
                    assert_eq!(sh_in(dir.path()), None);
                }
            }
        }
    }

    #[test]
    fn windows_powershell_is_the_one_in_the_windows_folder() {
        let Shell::PowerShell(program) = Shell::pick(None, None, desktop::windows_folder().as_deref()) else {
            panic!("PowerShell")
        };

        assert!(Path::new(&program).is_absolute() && Path::new(&program).is_file(), "{program}");
    }

    fn ran(line: &Line, dir: &Path) -> Output {
        line.command().current_dir(dir).stdin(std::process::Stdio::null()).output().expect("run the line")
    }

    fn stdout(output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
    }

    #[test]
    fn git_s_sh_gets_every_argument_as_it_was() {
        let dir = TempDir::new();
        let Shell::Sh(sh) = through_git() else { panic!("Git's sh") };
        let tricky = [
            "it's",
            "say \"hi\"",
            r"a\\b",
            r"end\",
            r"C:\Users\x y\",
            "*",
            "@file",
            "{a,b}",
            "~",
            "$HOME",
            "`id`",
            "",
            "line\nbreak",
            "ñandú 😀",
            "%PATH%",
            "^&|<>",
        ];
        let positional: Vec<String> = std::iter::once("sh").chain(tricky).map(String::from).collect();

        let output = ran(&sh.line("-c", r#"for a; do printf '[%s]\n' "$a"; done"#, &positional), dir.path());

        let expected: String = tricky.iter().flat_map(|arg| ["[", arg, "]\n"]).collect();
        assert_eq!(stdout(&output), expected);
    }

    #[test]
    fn a_script_through_git_s_sh_finds_its_tools_and_stops_at_the_first_failure() {
        let dir = TempDir::new();

        let output = ran(&through_git().run("ls -d . >/dev/null && echo found\nfalse\necho never"), dir.path());

        assert_eq!(output.status.code(), Some(1));
        assert_eq!(stdout(&output), "found\n");
    }

    #[test]
    fn a_script_through_powershell_stops_at_the_first_error() {
        let dir = TempDir::new();
        let powershell = Shell::pick(None, None, desktop::windows_folder().as_deref());

        let failed = ran(&powershell.run("Write-Output 'it''s'\nWrite-Error boom\nWrite-Output never"), dir.path());
        let exited = ran(&powershell.run("exit 4"), dir.path());

        assert_eq!((failed.status.code(), stdout(&failed)), (Some(1), "it's\n".to_string()));
        assert_eq!(exited.status.code(), Some(4));
    }

    #[test]
    fn a_plugin_script_runs_through_git_s_sh_by_its_first_line() {
        let dir = TempDir::new();
        let script = file(&dir, "run", "#!/bin/sh\nprintf '%s|' \"$@\"\n");

        let line = through_git().interpreted(&script, &strings(&["open", "it's here"]), None).expect("interpreted");
        let output = ran(&line, dir.path());

        assert_eq!(stdout(&output), "open|it's here|");
    }

    #[test]
    fn a_windows_program_a_plugin_script_starts_gets_unix_looking_arguments_as_they_are() {
        let dir = TempDir::new();
        let script = file(&dir, "run", "#!/bin/sh\nexec cmd.exe /d /c echo \"$@\"\n");
        let args = strings(&["open", "/api/v1", "--dir=/tmp/x"]);

        let output = ran(&through_git().interpreted(&script, &args, None).expect("interpreted"), dir.path());

        assert_eq!(stdout(&output), "open /api/v1 --dir=/tmp/x\n");
    }
}
