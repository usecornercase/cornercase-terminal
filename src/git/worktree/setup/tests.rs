use super::*;
use crate::test_util::TempDir;

#[cfg(unix)]
fn fake_shell(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join("shell");
    std::fs::write(&path, "#!/bin/sh\necho \"shell:$CORNERCASE_BRANCH:$CORNERCASE_ROOT\"\nprintf 'args:'; for a in \"$@\"; do printf '%s|' \"$a\"; done; echo\n").expect("write");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

#[cfg(unix)]
fn run(script: &str) -> (String, Option<i32>) {
    run_with(script, &[])
}

#[cfg(unix)]
fn run_with(script: &str, shell_args: &[String]) -> (String, Option<i32>) {
    let dir = TempDir::new();
    let shell = fake_shell(dir.path()).display().to_string();
    let status = dir.path().join("status");
    let wrapper = Wrapper {
        script,
        status: &status,
        shell: &shell,
        shell_args,
        root: Path::new("/repo"),
        branch: "feat/x",
        worktree: dir.path(),
        home: true,
    };
    let out = line(&Shell::sh(), &wrapper)
        .command()
        .current_dir(dir.path())
        .env("CORNERCASE_BRANCH", "inherited")
        .output()
        .expect("run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), read_status(&status))
}

#[cfg(unix)]
mod wrapper {
    use super::*;

    #[test]
    fn success_is_status_zero_and_continues_to_the_shell() {
        let (out, status) = run("echo hi");
        assert_eq!(status, Some(0));
        assert!(out.contains("hi") && out.contains("✓ setup finished"), "{out}");
        assert!(out.contains("shell:inherited:"), "{out}");
    }

    #[test]
    fn the_shell_starts_with_the_arguments_that_were_chosen() {
        let args = ["-l".to_string(), "two words".to_string()];
        let (out, status) = run_with("echo hi", &args);
        assert_eq!(status, Some(0));
        assert!(out.contains("args:-l|two words|"), "{out}");
        assert!(run("echo hi").0.contains("args:\n"), "no arguments are given by default");
    }

    #[test]
    fn the_first_failing_command_stops_the_script() {
        let (out, status) = run("false\necho never");
        assert_eq!(status, Some(1));
        assert!(!out.contains("never") && out.contains("exit status 1"), "{out}");
    }

    #[test]
    fn exit_reports_its_status_and_the_shell_still_starts() {
        let (out, status) = run("exit 3");
        assert_eq!(status, Some(3));
        assert!(out.contains("shell:"), "{out}");
    }

    #[test]
    fn the_variables_reach_the_script_but_not_the_shell() {
        let (out, _) = run("echo \"script:$CORNERCASE_BRANCH:$CORNERCASE_ROOT:$CORNERCASE_WORKTREE\"");
        assert!(out.contains("script:feat/x:/repo:"), "{out}");
        assert!(out.contains("shell:inherited:"), "{out}");
    }
}

mod status_file {
    use super::*;

    #[test]
    fn it_parses_a_number_and_nothing_else() {
        let dir = TempDir::new();
        let file = status_file(dir.path(), 7);
        assert_eq!(read_status(&file), None);
        std::fs::write(&file, "").expect("write");
        assert_eq!(read_status(&file), None);
        std::fs::write(&file, "12\n").expect("write");
        assert_eq!(read_status(&file), Some(12));
        std::fs::write(&file, "x").expect("write");
        assert_eq!(read_status(&file), None);
    }

    #[test]
    fn a_sweep_removes_only_status_files() {
        let dir = TempDir::new();
        std::fs::write(status_file(dir.path(), 3), "0").expect("write");
        std::fs::write(dir.path().join("config.json"), "{}").expect("write");
        sweep(dir.path());
        assert!(!status_file(dir.path(), 3).exists() && dir.path().join("config.json").exists());
    }
}

#[cfg(unix)]
mod teardown {
    use super::*;

    #[test]
    fn success_says_nothing() {
        let dir = TempDir::new();
        assert_eq!(teardown("true", dir.path(), dir.path(), "b", TEARDOWN_WITHIN), None);
    }

    #[test]
    fn a_failure_names_the_status_and_the_last_line() {
        let dir = TempDir::new();
        let why = teardown("echo first >&2\necho boom >&2\nexit 4", dir.path(), dir.path(), "b", TEARDOWN_WITHIN);
        assert_eq!(why.as_deref(), Some("exit status 4: boom"));
    }

    #[test]
    fn it_runs_in_the_worktree_with_the_variables() {
        let dir = TempDir::new();
        let root = TempDir::new();
        let script =
            "echo \"$PWD $CORNERCASE_ROOT $CORNERCASE_WORKTREE $CORNERCASE_BRANCH\" > \"$CORNERCASE_ROOT/out\"";
        assert_eq!(teardown(script, dir.path(), root.path(), "feat", TEARDOWN_WITHIN), None);
        let out = std::fs::read_to_string(root.path().join("out")).expect("out");
        let (d, r) = (dir.path().display(), root.path().display());
        assert_eq!(out.trim(), format!("{d} {r} {d} feat"));
    }

    #[test]
    fn a_folder_that_is_gone_is_skipped_quietly() {
        let dir = TempDir::new();
        assert_eq!(teardown("false", &dir.path().join("gone"), dir.path(), "b", TEARDOWN_WITHIN), None);
    }

    #[test]
    fn it_gives_up_after_the_limit() {
        let dir = TempDir::new();
        let why = teardown("sleep 30", dir.path(), dir.path(), "b", Duration::from_millis(200));
        assert!(why.is_some_and(|w| w.starts_with("did not finish")));
    }
}

#[cfg(windows)]
mod on_windows {
    use std::io::Write as _;
    use std::process::Stdio;

    use super::*;

    const CMD: &str = r"C:\Windows\System32\cmd.exe";

    struct Ran {
        out: String,
        status: Option<i32>,
        dir: TempDir,
        root: TempDir,
    }

    fn git_sh() -> Shell {
        Shell::pick(Some(&script::git_sh().expect("Git for Windows' sh, which these tests need")), None, None)
    }

    fn windows_powershell() -> Shell {
        Shell::pick(None, None, crate::os::desktop::windows_folder().as_deref())
    }

    fn setup(interpreter: &Shell, script: &str, typed: &str) -> Ran {
        let (dir, root) = (TempDir::new(), TempDir::new());
        let status = dir.path().join("setup-1.status");
        let wrapper = Wrapper {
            script,
            status: &status,
            shell: CMD,
            shell_args: &[],
            root: root.path(),
            branch: "feat/it's",
            worktree: dir.path(),
            home: false,
        };
        let mut child = line(interpreter, &wrapper)
            .command()
            .current_dir(dir.path())
            .env("CORNERCASE_BRANCH", "inherited")
            .env_remove("HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start the setup");
        child.stdin.take().expect("its input").write_all(typed.as_bytes()).expect("type into it");
        let output = child.wait_with_output().expect("the setup ends");
        let out = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        Ran { out, status: read_status(&status), dir, root }
    }

    impl Ran {
        fn succeeded(&self) {
            let (dir, root) = (self.dir.path().display(), self.root.path().display());
            assert_eq!(self.status, Some(0), "{}", self.out);
            assert!(self.out.contains(&format!("script:feat/it's:{root}:{dir}")), "{}", self.out);
            assert!(self.out.contains(" setup finished"), "{}", self.out);
        }

        fn failed(&self) {
            assert_eq!(self.status, Some(1), "{}", self.out);
            assert!(!self.out.contains("never"), "{}", self.out);
            assert!(self.out.contains(" setup failed with exit status 1"), "{}", self.out);
        }
    }

    #[test]
    fn a_setup_runs_through_git_s_sh_then_starts_the_shell() {
        let script = "echo \"script:$CORNERCASE_BRANCH:$CORNERCASE_ROOT:$CORNERCASE_WORKTREE\"\nls -d . >/dev/null\ntest -n \"$HOME\"";

        let ran = setup(&git_sh(), script, "echo shell:%CORNERCASE_BRANCH%:%PWD%:%SHLVL%:%HOME%\r\nexit\r\n");

        ran.succeeded();
        assert!(ran.out.contains("✓ setup finished"), "{}", ran.out);
        assert!(ran.out.contains("shell:inherited:%PWD%:%SHLVL%:%HOME%"), "{}", ran.out);
    }

    #[test]
    fn a_setup_through_git_s_sh_stops_at_the_first_failing_command() {
        let ran = setup(&git_sh(), "false\necho never", "exit\r\n");

        ran.failed();
        assert!(ran.out.contains("✗ setup failed"), "{}", ran.out);
    }

    #[test]
    fn without_git_s_sh_a_setup_runs_through_powershell_then_stays_in_it() {
        let script =
            "Write-Output \"script:${env:CORNERCASE_BRANCH}:${env:CORNERCASE_ROOT}:${env:CORNERCASE_WORKTREE}\"";
        let powershell = windows_powershell();
        let typed =
            "Write-Output \"shell:${env:CORNERCASE_BRANCH}:$(Get-Location):$($null -eq $cornercase)\"\r\nexit\r\n";

        let ran = setup(&powershell, script, typed);

        ran.succeeded();
        assert!(ran.out.contains(&format!("shell::{}:True", ran.dir.path().display())), "{}", ran.out);
    }

    #[test]
    fn a_failing_setup_through_powershell_reports_its_status() {
        let ran = setup(&windows_powershell(), "Write-Error boom\nWrite-Output never", "exit\r\n");

        ran.failed();
    }

    #[test]
    fn a_teardown_runs_with_the_variables_and_names_its_last_line() {
        let (dir, root) = (TempDir::new(), TempDir::new());
        let script =
            "echo \"$CORNERCASE_WORKTREE $CORNERCASE_BRANCH\" > \"$CORNERCASE_ROOT/out\"\necho boom >&2\nexit 4";

        let why = teardown(script, dir.path(), root.path(), "feat", TEARDOWN_WITHIN);

        assert_eq!(why.as_deref(), Some("exit status 4: boom"));
        let out = std::fs::read_to_string(root.path().join("out")).expect("out");
        assert_eq!(out.trim(), format!("{} feat", dir.path().display()));
    }
}
