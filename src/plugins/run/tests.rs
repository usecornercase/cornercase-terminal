#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::plugins::registry::{Entry, Source};
use crate::test_util::TempDir;
#[cfg(unix)]
use crate::test_util::{wait_until, write_executable};

fn plugin(root: &Path) -> Plugin {
    let entry = Entry { id: "acme.notes".into(), source: Source::Link { path: root.to_path_buf() }, enabled: true };
    Plugin { entry, manifest: Err("not read in these tests".into()) }
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).expect("exists").permissions().mode() & 0o777
}

#[cfg(unix)]
#[test]
fn a_command_gets_its_folders_and_none_of_the_callers_identity() {
    let (home, root) = (TempDir::new(), TempDir::new());
    let places = Places::new(&home.path().join("config.json"), &home.path().join("session.json"));
    let script = "#!/bin/sh\necho \"id=$CORNERCASE_PLUGIN_ID root=$CORNERCASE_PLUGIN_ROOT config=$CORNERCASE_PLUGIN_CONFIG \
                  pane=${CORNERCASE_PANE-unset} server=${CORNERCASE_SERVER-unset} extra=$EXTRA\"\n";
    write_executable(&root.path().join("run"), script);
    let plugin = plugin(root.path());
    let mut command =
        places.command(&plugin, &["./run".into()], &[("EXTRA".into(), "yes".into())], None).expect("command");
    let status = command.status().expect("ran");
    let log = std::fs::read_to_string(places.log("acme.notes")).expect("log");
    let config = places.config("acme.notes");
    let expected = format!(
        "id=acme.notes root={} config={} pane=unset server=unset extra=yes\n",
        root.path().display(),
        config.display()
    );
    assert_eq!((status.success(), log), (true, expected));
    assert_eq!(
        (mode(&config), mode(&places.state("acme.notes")), mode(&places.log("acme.notes"))),
        (0o700, 0o700, 0o600)
    );
}

#[cfg(unix)]
#[test]
fn a_command_that_outlives_its_time_is_stopped_with_its_children() {
    let (home, root) = (TempDir::new(), TempDir::new());
    let places = Places::new(&home.path().join("config.json"), &home.path().join("session.json"));
    write_executable(&root.path().join("hang"), "#!/bin/sh\nsleep 30 &\necho $! > child\nwait\n");
    let child = places.spawn(&plugin(root.path()), &["./hang".into()], &[], None).expect("spawn");
    let pid_file = root.path().join("child");
    wait_until("the child starts", || std::fs::read_to_string(&pid_file).is_ok_and(|s| s.ends_with('\n')));
    let grandchild: i32 = std::fs::read_to_string(&pid_file).expect("pid").trim().parse().expect("a pid");
    assert_eq!(finish(child, Duration::from_millis(200)).expect("waited"), None);
    wait_until("the sleep is gone", || !crate::process::alive(grandchild));
}

#[cfg(unix)]
fn ran(root: &TempDir, script: &str, within: Duration) -> Ran {
    let home = TempDir::new();
    let places = Places::new(&home.path().join("config.json"), &home.path().join("session.json"));
    write_executable(&root.path().join("go"), script);
    let invocation = Invocation { argv: &["./go".into()], env: &[], cwd: None };
    run_to_end(&places, &plugin(root.path()), invocation, within)
}

#[cfg(unix)]
#[test]
fn a_command_that_succeeds_has_no_failure() {
    let root = TempDir::new();
    let ran = ran(&root, "#!/bin/sh\nexit 0\n", Duration::from_secs(30));
    assert!(matches!(ran.ended, Ended::Status(status) if status.success()));
    assert_eq!(ran.failure(), None);
}

#[cfg(unix)]
#[test]
fn a_command_that_exits_or_is_killed_says_how() {
    let root = TempDir::new();
    let exited = ran(&root, "#!/bin/sh\nexit 3\n", Duration::from_secs(30));
    assert_eq!(exited.failure().as_deref(), Some("exit status: 3"));
    let killed = ran(&root, "#!/bin/sh\nkill -9 $$\n", Duration::from_secs(30));
    assert!(
        matches!(&killed.ended, Ended::Status(status) if std::os::unix::process::ExitStatusExt::signal(status) == Some(9))
    );
    assert!(killed.failure().is_some_and(|why| why.contains("signal: 9")));
}

#[cfg(unix)]
#[test]
fn a_command_that_runs_too_long_times_out() {
    let root = TempDir::new();
    let ran = ran(&root, "#!/bin/sh\nexec sleep 30\n", Duration::from_secs(1));
    assert!(matches!(ran.ended, Ended::Timeout(_)));
    assert_eq!(ran.failure().as_deref(), Some("stopped after 1 s"));
    assert!(ran.took < Duration::from_secs(20));
}

#[test]
fn a_program_that_is_missing_cannot_start() {
    let (home, root) = (TempDir::new(), TempDir::new());
    let places = Places::new(&home.path().join("config.json"), &home.path().join("session.json"));
    let invocation = Invocation { argv: &["./absent".into()], env: &[], cwd: None };
    let ran = run_to_end(&places, &plugin(root.path()), invocation, Duration::from_secs(5));
    assert!(matches!(ran.ended, Ended::Failed(_)));
    assert!(ran.failure().is_some_and(|why| why.starts_with("cannot start ./absent: ")));
}

#[cfg(windows)]
mod on_windows {
    use super::*;
    use crate::script::Shell;

    fn places(home: &TempDir) -> Places {
        Places::new(&home.path().join("config.json"), &home.path().join("session.json"))
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn an_action_written_for_sh_runs_through_git_s_sh_with_its_arguments() {
        let (home, root) = (TempDir::new(), TempDir::new());
        let places = places(&home);
        let script = "#!/bin/sh\nprintf '%s|' \"$@\" \"$CORNERCASE_PLUGIN_ID\" \"$EXTRA\"\n";
        std::fs::write(root.path().join("run"), script).expect("write the action");
        let command = argv(&["./run", "open", "it's here", "*"]);
        let env = [("EXTRA".to_string(), "yes".to_string())];

        let launch = places.launch(&plugin(root.path()), &command, &env, None).expect("launch");
        let invocation = Invocation { argv: &command, env: &env, cwd: None };
        let ran = run_to_end(&places, &plugin(root.path()), invocation, Duration::from_secs(30));

        let Shell::Sh(sh) = Shell::here() else { panic!("Git for Windows' sh, which this test needs") };
        let log = std::fs::read_to_string(places.log("acme.notes")).expect("log");
        assert_eq!(launch.program, PathBuf::from(&sh.program));
        assert_eq!((ran.failure(), log.as_str()), (None, "open|it's here|*|acme.notes|yes|"));
    }

    #[test]
    fn an_action_that_fails_says_its_exit_code() {
        let (home, root) = (TempDir::new(), TempDir::new());
        std::fs::write(root.path().join("fail"), "#!/bin/sh\nexit 3\n").expect("write the action");
        let invocation = Invocation { argv: &argv(&["./fail"]), env: &[], cwd: None };

        let ran = run_to_end(&places(&home), &plugin(root.path()), invocation, Duration::from_secs(30));

        assert_eq!(ran.failure().as_deref(), Some("exit code: 3"));
    }

    #[test]
    fn a_program_is_started_as_it_is() {
        let (home, root) = (TempDir::new(), TempDir::new());
        let command = argv(&["cmd.exe", "/d", "/c", "exit 5"]);

        let launch = places(&home).launch(&plugin(root.path()), &command, &[], None).expect("launch");
        let invocation = Invocation { argv: &command, env: &[], cwd: None };
        let ran = run_to_end(&places(&home), &plugin(root.path()), invocation, Duration::from_secs(30));

        assert_eq!((launch.program, launch.args), (PathBuf::from("cmd.exe"), argv(&["/d", "/c", "exit 5"])));
        assert_eq!(ran.failure().as_deref(), Some("exit code: 5"));
    }
}
