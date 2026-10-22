use rstest::rstest;

use super::*;

fn host(name: &str, ssh_host: &str, state: &str) -> Host {
    Host { host: name.into(), ssh_host: ssh_host.into(), state: state.into(), ..Host::default() }
}

fn listed(hosts: Vec<Host>) -> GlabHosts {
    GlabHosts { listed: Some(Ok(hosts)), ..GlabHosts::default() }
}

fn failed(why: &str) -> GlabHosts {
    GlabHosts { listed: Some(Err(why.into())), ..GlabHosts::default() }
}

fn gitlab_com() -> Vec<Host> {
    vec![host("gitlab.com", "altssh.gitlab.com", "success"), host("git.acme.dev", "git-ssh.acme.dev", "success")]
}

mod route {
    use super::*;

    #[rstest]
    #[case::no_remote(None)]
    #[case::github(Some("github.com"))]
    #[case::github_over_ssh(Some("ssh.github.com"))]
    fn github_com_and_no_host_never_wait_for_glab(#[case] host: Option<&str>) {
        assert_eq!(GlabHosts::default().route(host), Route::Ask(Provider::Github));
    }

    #[rstest]
    #[case::gitlab("gitlab.com")]
    #[case::enterprise("ghe.acme.dev")]
    fn another_host_waits_for_glabs_list(#[case] host: &str) {
        assert_eq!(GlabHosts::default().route(Some(host)), Route::Wait);
    }

    #[rstest]
    #[case::gitlab("gitlab.com")]
    #[case::ssh_alias("altssh.gitlab.com")]
    #[case::listed_by_its_ssh_host("git-ssh.acme.dev")]
    #[case::listed_by_its_name("git.acme.dev")]
    fn a_host_glab_is_signed_in_to_asks_glab(#[case] host: &str) {
        assert_eq!(listed(gitlab_com()).route(Some(host)), Route::Ask(Provider::Gitlab));
    }

    #[test]
    fn a_host_that_is_not_gitlab_asks_gh() {
        assert_eq!(listed(gitlab_com()).route(Some("ghe.acme.dev")), Route::Ask(Provider::Github));
        assert_eq!(failed("install glab").route(Some("ghe.acme.dev")), Route::Ask(Provider::Github));
    }

    #[test]
    fn a_gitlab_host_glab_is_logged_out_of_says_to_log_in() {
        let hosts = listed(vec![host("gitlab.com", "", "error")]);

        assert_eq!(hosts.route(Some("gitlab.com")), Route::Explain(glab::LOGGED_OUT.into()));
    }

    #[test]
    fn a_gitlab_host_glab_does_not_list_says_to_log_in() {
        assert_eq!(listed(gitlab_com()).route(Some("gitlab.acme.dev")), Route::Explain(glab::LOGGED_OUT.into()));
    }

    #[test]
    fn a_host_in_error_says_its_own_error() {
        let broken = Host {
            error: "dial tcp: no such host".into(),
            token_source: "env".into(),
            ..host("gitlab.com", "", "error")
        };

        assert_eq!(listed(vec![broken]).route(Some("gitlab.com")), Route::Explain("dial tcp: no such host".into()));
    }

    #[test]
    fn a_missing_glab_is_explained_for_a_gitlab_host() {
        let hosts = failed("install glab, GitLab's CLI, to see them");

        assert_eq!(hosts.route(Some("gitlab.com")), Route::Explain("install glab, GitLab's CLI, to see them".into()));
    }
}

mod due {
    use super::*;

    fn wanted(hosts: &[&str]) -> BTreeSet<String> {
        hosts.iter().map(ToString::to_string).collect()
    }

    fn asked(now: Instant, listed: Option<Listed>, hosts: &[&str]) -> GlabHosts {
        GlabHosts { listed, asked: Some(now), wanted: wanted(hosts), ..GlabHosts::default() }
    }

    #[test]
    fn the_first_time_is_due() {
        assert!(GlabHosts::default().due(Instant::now(), &wanted(&["gitlab.com"])));
    }

    #[rstest]
    #[case::signed_in_after_a_few_minutes(Some(Ok(gitlab_com())), 299, false)]
    #[case::signed_in_after_a_quarter_of_an_hour(Some(Ok(gitlab_com())), 900, true)]
    #[case::logged_out_waits_five_minutes(Some(Ok(vec![host("gitlab.com", "", "error")])), 299, false)]
    #[case::logged_out_asks_again_after_five(Some(Ok(vec![host("gitlab.com", "", "error")])), 300, true)]
    #[case::a_failure_asks_again_after_five(Some(Err("boom".into())), 300, true)]
    fn it_repeats_every_so_often(#[case] listed: Option<Listed>, #[case] seconds: u64, #[case] due: bool) {
        let now = Instant::now();

        let hosts = asked(now, listed, &["gitlab.com"]);

        assert_eq!(hosts.due(now + Duration::from_secs(seconds), &wanted(&["gitlab.com"])), due);
    }

    #[test]
    fn another_host_is_due_at_once_and_fewer_are_not() {
        let now = Instant::now();
        let hosts = asked(now, Some(Ok(gitlab_com())), &["gitlab.com", "git.acme.dev"]);

        assert!(hosts.due(now, &wanted(&["gitlab.com", "git.example.org"])));
        assert!(!hosts.due(now, &wanted(&["gitlab.com"])));
    }
}
