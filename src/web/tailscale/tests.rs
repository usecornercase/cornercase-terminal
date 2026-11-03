use rstest::rstest;

use super::*;
use crate::test_util::TempDir;
#[cfg(unix)]
use crate::test_util::write_executable;

fn status_json(state: &str, certs: &str) -> String {
    format!(
        r#"{{
  "BackendState": "{state}",
  "CertDomains": {certs},
  "Self": {{"DNSName": "mac.tail1234.ts.net.", "UserID": 42}},
  "User": {{"42": {{"ID": 42, "LoginName": "ada@example.com"}}}}
}}"#
    )
}

mod parse_status {
    use super::*;

    #[test]
    fn a_running_node_with_https_gives_its_name_and_owner() {
        let node = parse_status(&status_json("Running", r#"["mac.tail1234.ts.net"]"#)).expect("a node");

        assert_eq!(node, Node { host: "mac.tail1234.ts.net".into(), login: "ada@example.com".into() });
        assert_eq!(node.url(), "https://mac.tail1234.ts.net:8443");
    }

    #[rstest]
    #[case::logged_out("NeedsLogin", r#"["mac.tail1234.ts.net"]"#, Problem::NotConnected("NeedsLogin".into()))]
    #[case::stopped("Stopped", "null", Problem::NotConnected("Stopped".into()))]
    #[case::no_certificates("Running", "null", Problem::NoHttps)]
    #[case::certificates_for_another_name("Running", r#"["other.tail1234.ts.net"]"#, Problem::NoHttps)]
    fn says_what_is_missing(#[case] state: &str, #[case] certs: &str, #[case] problem: Problem) {
        assert_eq!(parse_status(&status_json(state, certs)), Err(problem));
    }

    #[test]
    fn output_that_is_not_json_is_a_failure() {
        assert!(matches!(parse_status("Tailscale is stopped."), Err(Problem::Failed(_))));
    }
}

mod finding {
    use super::*;

    fn placed(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).expect("folder");
        let path = dir.join(name);
        std::fs::write(&path, "").expect("program");
        path
    }

    #[rstest]
    #[case::unix(false, "tailscale")]
    #[case::windows(true, "tailscale.exe")]
    fn the_program_on_the_path_is_named_as_the_system_names_it(#[case] windows: bool, #[case] name: &str) {
        let dir = TempDir::new();
        let program = placed(&dir.path().join("bin"), name);
        let path = std::env::join_paths([dir.path().join("empty"), dir.path().join("bin")]).expect("path");

        assert_eq!(found(windows, Some(&path), None), Some(program));
    }

    #[test]
    fn windows_finds_it_in_program_files_when_the_path_misses_it() {
        let dir = TempDir::new();
        let program = placed(&dir.path().join("Tailscale"), "tailscale.exe");

        assert_eq!(found(true, None, Some(dir.path().as_os_str())), Some(program));
    }

    #[test]
    fn windows_without_it_finds_nothing() {
        let dir = TempDir::new();
        placed(&dir.path().join("bin"), "tailscale");

        assert_eq!(found(true, Some(dir.path().join("bin").as_os_str()), Some(dir.path().as_os_str())), None);
    }
}

mod commands {
    use super::*;

    #[cfg(unix)]
    fn fake(dir: &TempDir, script: &str) -> PathBuf {
        let path = dir.path().join("tailscale");
        write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
        path
    }

    #[cfg(unix)]
    #[test]
    fn start_serves_the_local_port_on_the_https_port() {
        let dir = TempDir::new();
        let log = dir.path().join("args");
        let status = status_json("Running", r#"["mac.tail1234.ts.net"]"#);
        let program = fake(
            &dir,
            &format!("echo \"$@\" >> '{}'\nif [ \"$1\" = status ]; then cat <<'EOF'\n{status}\nEOF\nfi", log.display()),
        );

        let node = start(&program, 4321).expect("started");

        assert_eq!(node.host, "mac.tail1234.ts.net");
        let calls = std::fs::read_to_string(&log).expect("calls");
        assert_eq!(calls, "status --json\nserve --bg --yes --https=8443 http://127.0.0.1:4321\n");
    }

    #[cfg(unix)]
    #[test]
    fn stop_turns_the_https_port_off() {
        let dir = TempDir::new();
        let log = dir.path().join("args");
        let program = fake(&dir, &format!("echo \"$@\" >> '{}'", log.display()));

        stop(&program).expect("stopped");

        assert_eq!(std::fs::read_to_string(&log).expect("calls"), "serve --https=8443 off\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_command_reports_its_last_line() {
        let dir = TempDir::new();
        let program = fake(&dir, "echo 'first' >&2\necho 'serve config denied' >&2\nexit 1");

        assert_eq!(stop(&program), Err(Problem::Failed("serve config denied".into())));
    }

    #[test]
    fn a_missing_program_is_not_installed() {
        assert_eq!(status(Path::new("/nonexistent/tailscale")), Err(Problem::NotInstalled));
    }
}
