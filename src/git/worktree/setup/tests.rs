use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use super::*;
use crate::test_util::TempDir;

fn fake_shell(dir: &Path) -> PathBuf {
    let path = dir.join("shell");
    std::fs::write(&path, "#!/bin/sh\necho \"shell:$CORNERCASE_BRANCH:$CORNERCASE_ROOT\"\n").expect("write");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

fn run(script: &str) -> (String, Option<i32>) {
    let dir = TempDir::new();
    let shell = fake_shell(dir.path());
    let status = dir.path().join("status");
    let out = Command::new("/bin/sh")
        .args(args(script, &status, &shell.display().to_string(), Path::new("/repo"), "feat/x"))
        .current_dir(dir.path())
        .env("CORNERCASE_BRANCH", "inherited")
        .output()
        .expect("run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), read_status(&status))
}

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
