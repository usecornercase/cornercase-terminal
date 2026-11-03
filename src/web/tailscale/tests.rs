use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_program};

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

    #[test]
    fn windows_finds_tailscale_exe_in_the_absolute_folders_of_the_path() {
        let dir = TempDir::new();
        let program = placed(&dir.path().join("bin"), "tailscale.exe");
        let folders = [PathBuf::from("bin"), PathBuf::new(), dir.path().join("empty"), dir.path().join("bin")];
        let path = std::env::join_paths(folders).expect("path");

        assert_eq!(found(true, Some(&path), None), Some(program));
    }

    #[test]
    fn finds_the_program_on_the_path_as_this_system_names_it() {
        let dir = TempDir::new();
        let program = fake_program(dir.path(), "tailscale");

        assert_eq!(found(cfg!(windows), Some(dir.path().as_os_str()), None), Some(program));
    }

    #[test]
    fn windows_finds_it_in_program_files_when_the_path_misses_it() {
        let dir = TempDir::new();
        let program = placed(&dir.path().join("Tailscale"), "tailscale.exe");

        assert_eq!(found(true, None, Some(dir.path().as_os_str())), Some(program));
    }

    #[test]
    fn windows_takes_program_files_only_as_an_absolute_folder() {
        assert_eq!(installed(true, Some(OsStr::new("Program Files"))), None);
        assert_eq!(installed(true, None), None);
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

    fn fake(dir: &TempDir, out: &str, err: &str, code: i32) -> PathBuf {
        let program = fake_program(dir.path(), "tailscale");
        for (extension, text) in [("out", out), ("err", err), ("code", &code.to_string())] {
            std::fs::write(program.with_extension(extension), text).expect("write the fake's answer");
        }
        program
    }

    fn calls(program: &Path) -> String {
        std::fs::read_to_string(program.with_extension("args")).expect("calls")
    }

    #[test]
    fn start_serves_the_local_port_on_the_https_port() {
        let dir = TempDir::new();
        let program = fake(&dir, &status_json("Running", r#"["mac.tail1234.ts.net"]"#), "", 0);

        let node = start(&program, 4321).expect("started");

        assert_eq!(node.host, "mac.tail1234.ts.net");
        assert_eq!(calls(&program), "status --json\nserve --bg --yes --https=8443 http://127.0.0.1:4321\n");
    }

    #[test]
    fn stop_turns_the_https_port_off() {
        let dir = TempDir::new();
        let program = fake(&dir, "", "", 0);

        stop(&program).expect("stopped");

        assert_eq!(calls(&program), "serve --https=8443 off\n");
    }

    #[test]
    fn a_failing_command_reports_its_last_line() {
        let dir = TempDir::new();
        let program = fake(&dir, "", "first\nserve config denied\n", 1);

        assert_eq!(stop(&program), Err(Problem::Failed("serve config denied".into())));
    }

    #[test]
    fn a_missing_program_is_not_installed() {
        assert_eq!(status(Path::new("/nonexistent/tailscale")), Err(Problem::NotInstalled));
    }
}
