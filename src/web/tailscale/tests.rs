use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, write_executable};

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

mod commands {
    use super::*;

    fn fake(dir: &TempDir, script: &str) -> PathBuf {
        let path = dir.path().join("tailscale");
        write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
        path
    }

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

    #[test]
    fn stop_turns_the_https_port_off() {
        let dir = TempDir::new();
        let log = dir.path().join("args");
        let program = fake(&dir, &format!("echo \"$@\" >> '{}'", log.display()));

        stop(&program).expect("stopped");

        assert_eq!(std::fs::read_to_string(&log).expect("calls"), "serve --https=8443 off\n");
    }

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
