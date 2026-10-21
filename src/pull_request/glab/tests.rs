use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_glab, fixture};

const LOGGED_OUT_STATUS: &str = fixture!("glab/1.124.0/auth-status-logged-out.json");

const STATUS: &str = fixture!("glab/1.123.0/auth-status.json");

const NONE_STDOUT: &str = fixture!("glab/1.124.0/mr-view-none.stdout");

const NONE_STDERR: &str = fixture!("glab/1.124.0/mr-view-none.stderr");

fn output(stdout: &str, stderr: &str) -> Output {
    Output { status: ExitStatus::from_raw(256), stdout: stdout.into(), stderr: stderr.into() }
}

fn host(name: &str, ssh_host: &str) -> Host {
    Host { host: name.into(), ssh_host: ssh_host.into(), ..Host::default() }
}

mod said {
    use super::*;

    #[test]
    fn reads_the_json_envelope() {
        let said = said(&output(NONE_STDOUT, NONE_STDERR));

        assert_eq!(said, "no open merge request available for \"feat/x\"");
    }

    #[test]
    fn falls_back_to_fangs_stderr_without_the_header() {
        let said = said(&output("", NONE_STDERR));

        assert_eq!(said, "No open merge request available for \"feat/x\".");
    }

    #[test]
    fn skips_an_api_body_whose_error_is_a_string() {
        let said = said(&output(r#"{"error":"invalid_token"}"#, "glab: 401 Unauthorized (HTTP 401)"));

        assert_eq!(said, "glab: 401 Unauthorized (HTTP 401)");
    }

    #[test]
    fn falls_back_to_stdout_then_to_the_status() {
        assert_eq!(said(&output("  plain  ", "")), "plain");
        assert!(said(&output("", "")).starts_with("glab failed with"));
    }
}

mod hosts {
    use super::*;

    #[test]
    fn keeps_the_hosts_in_error() {
        let tmp = TempDir::new();
        let glab = fake_glab(tmp.path(), &format!("cat <<'EOF'\n{LOGGED_OUT_STATUS}\nEOF\nexit 1"));

        let found = hosts(&glab, tmp.path()).expect("hosts");

        assert_eq!(found.len(), 1);
        assert!(!found[0].signed_in());
        assert_eq!(found[0].why(), LOGGED_OUT);
    }

    #[test]
    fn lists_every_host_of_the_older_status() {
        let tmp = TempDir::new();
        let glab = fake_glab(tmp.path(), &format!("cat <<'EOF'\n{STATUS}\nEOF"));

        let found = hosts(&glab, tmp.path()).expect("hosts");

        assert_eq!(found.len(), 2);
        assert_eq!(found[1].why(), LOGGED_OUT);
    }

    #[test]
    fn a_missing_glab_is_missing() {
        let tmp = TempDir::new();

        assert_eq!(hosts(&tmp.path().join("no-glab"), tmp.path()), Err(Unlisted::Missing));
    }

    #[rstest]
    #[case::old("echo 'unknown flag: --output' >&2; exit 1", Unlisted::Old)]
    #[case::fang_old("printf '\\n  ERROR  \\n\\n  Unknown flag: --output.\\n' >&2; exit 1", Unlisted::Old)]
    #[case::none("echo 'no GitLab instances have been authenticated' >&2; exit 1", Unlisted::LoggedOut)]
    #[case::empty(r#"echo '{"hosts":[]}'"#, Unlisted::LoggedOut)]
    #[case::an_error_envelope(
        r#"echo '{"error":{"message":"dial tcp: i/o timeout"}}'; exit 1"#,
        Unlisted::Failed("dial tcp: i/o timeout".into())
    )]
    #[case::an_envelope_saying_nobody_logged_in(
        r#"echo '{"error":{"message":"no GitLab instances have been authenticated"}}'; exit 1"#,
        Unlisted::LoggedOut
    )]
    #[case::other("echo 'dial tcp: i/o timeout' >&2; exit 1", Unlisted::Failed("dial tcp: i/o timeout".into()))]
    fn says_why_nothing_is_listed(#[case] script: &str, #[case] expected: Unlisted) {
        let tmp = TempDir::new();

        assert_eq!(hosts(&fake_glab(tmp.path(), script), tmp.path()), Err(expected));
    }

    #[test]
    fn repeated_hosts_are_listed_once() {
        let tmp = TempDir::new();
        let json = r#"{"hosts":[{"host":"Gitlab.com","state":"success"},{"host":"gitlab.com","state":"error"}]}"#;

        let found = hosts(&fake_glab(tmp.path(), &format!("echo '{json}'")), tmp.path()).expect("hosts");

        assert_eq!(found.len(), 1);
        assert!(found[0].signed_in());
    }
}

mod unauthorized {
    use super::*;

    #[rstest]
    #[case::the_status("GET https://gitlab.com/api/v4/user: 401 {message: 401 Unauthorized}", true)]
    #[case::glabs_line("glab: 401 Unauthorized (HTTP 401)", true)]
    #[case::at_the_end("GET https://x/api/v4/user: 401", true)]
    #[case::a_project_id("GET https://x/api/v4/projects/140123/pipelines: 500", false)]
    #[case::a_merge_request("GET https://x/api/v4/projects/9/merge_requests/401: 500 oops", false)]
    #[case::a_port("dial tcp 10.0.0.1:4010: i/o timeout", false)]
    fn reads_the_status_not_the_digits(#[case] said: &str, #[case] expected: bool) {
        assert_eq!(unauthorized(said), expected);
    }
}

mod serving {
    use super::*;

    #[rstest]
    #[case::the_same("gitlab.com", "", "gitlab.com", true)]
    #[case::another_case("GitLab.com", "", "gitlab.COM", true)]
    #[case::a_subdomain("gitlab.com", "", "ssh.gitlab.com", true)]
    #[case::the_ssh_host("gitlab.com", "altssh.gitlab.com", "altssh.gitlab.com", true)]
    #[case::not_a_suffix("gitlab.com", "", "notgitlab.com", false)]
    #[case::another_host("gitlab.com", "", "bitbucket.org", false)]
    #[case::no_ssh_host("gitlab.com", "", "", false)]
    fn matches_the_host(#[case] own: &str, #[case] ssh: &str, #[case] asked: &str, #[case] serves: bool) {
        assert_eq!(host(own, ssh).serves(asked), serves);
    }

    #[rstest]
    #[case::no_token("none", "", LOGGED_OUT)]
    #[case::a_401("env", "GET https://x/api/v4/user: 401 Unauthorized", LOGGED_OUT)]
    #[case::no_error("env", "", LOGGED_OUT)]
    #[case::its_own_error("env", "dial tcp: lookup x: no such host", "dial tcp: lookup x: no such host")]
    #[case::a_number_that_holds_401("env", "GET https://x/projects/140123: 500", "GET https://x/projects/140123: 500")]
    fn explains_a_host_that_is_not_signed_in(#[case] source: &str, #[case] error: &str, #[case] why: &str) {
        let host = Host { token_source: source.into(), error: error.into(), ..Host::default() };

        assert_eq!(host.why(), why);
    }
}
