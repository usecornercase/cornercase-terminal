use rstest::rstest;

use super::*;
use crate::test_util::{FakeHttp, TempDir, decoded, git, redirected};

const USER: &str = r#"{"id":7,"username":"ana","name":"Ana Pérez"}"#;
const ISSUES: &str = "GET /api/v4/projects/acme%2Fshop/issues";
const NODE: &str = r#"{"id":901,"iid":42,"title":"Login redirect loops","state":"opened",
        "labels":["bug","p1"],"assignees":[{"username":"luis","name":"Luis"}],
        "author":{"username":"ana","name":"Ana Pérez"},"updated_at":"2026-09-19T14:00:00.000Z",
        "web_url":"https://gitlab.com/acme/shop/-/issues/42"}"#;

fn repo(remotes: &[(&str, &str)]) -> TempDir {
    let repo = TempDir::new();
    git(repo.path(), &["init", "--quiet"]);
    for (name, url) in remotes {
        git(repo.path(), &["remote", "add", name, url]);
    }
    repo
}

fn api(server: &FakeHttp, repo: &TempDir) -> Api {
    Api { base: server.url(), token: "t0ken".into(), dir: repo.path().to_path_buf() }
}

fn on_the_server(server: &FakeHttp) -> TempDir {
    repo(&[("origin", &format!("{}/acme/shop.git", server.url()))])
}

fn listed(query: &Query, rows: &str) -> (Listed, FakeHttp) {
    let server = FakeHttp::start(vec![("GET /api/v4/user", 200, USER.to_string()), (ISSUES, 200, rows.to_string())]);
    let repo = on_the_server(&server);
    let listed = api(&server, &repo).list(query).expect("list");
    (listed, server)
}

fn list_error(server: &FakeHttp, repo: &TempDir) -> Option<String> {
    api(server, repo).list(&Query::default()).err().map(|e| e.to_string())
}

fn query_sent(query: &Query) -> String {
    let (_, server) = listed(query, "[]");
    decoded(&server.request(1))
}

#[test]
fn whoami_sends_the_token_and_names_the_server() {
    let server = FakeHttp::start(vec![("GET /api/v4/user", 200, USER)]);

    let account = api(&server, &TempDir::new()).whoami().expect("whoami");

    assert_eq!(account, Account { handle: "ana".into(), workspace: "127.0.0.1".into() });
    assert!(server.request(0).to_lowercase().contains("private-token: t0ken"), "{}", server.request(0));
}

#[rstest]
#[case::unauthorized(401, r#"{"message":"401 Unauthorized"}"#, REJECTED)]
#[case::missing_scope(403, r#"{"error":"insufficient_scope"}"#, REJECTED)]
#[case::rate_limited(429, "{}", "GitLab rate limit reached, try again in a minute")]
#[case::no_user(404, "{}", "no GitLab user was found for this token")]
#[case::message(500, r#"{"message":"500 Internal Server Error"}"#, "GitLab answered: 500 Internal Server Error")]
#[case::message_object(400, r#"{"message":{"state":["is invalid"]}}"#, r#"GitLab answered: {"state":["is invalid"]}"#)]
#[case::empty_body(502, "", "GitLab answered 502")]
fn whoami_failures_say_why(#[case] status: u16, #[case] body: &'static str, #[case] message: &str) {
    let server = FakeHttp::start(vec![("GET /api/v4/user", status, body)]);
    let err = api(&server, &TempDir::new()).whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some(message));
}

#[test]
fn a_redirect_is_not_followed_so_the_token_stays_on_the_server() {
    let (server, elsewhere) = redirected("GET /api/v4/user");

    let err = api(&server, &TempDir::new()).whoami().err().map(|e| e.to_string());

    assert_eq!(
        err.as_deref(),
        Some(
            "GitLab answered 302 with a redirect, which is not followed so the token is not sent elsewhere: use the \
             address it leads to"
        )
    );
    assert_eq!(elsewhere.requests(), Vec::<String>::new());
}

#[test]
fn something_that_is_not_gitlab_says_so() {
    let server = FakeHttp::start(vec![("GET /api/v4/user", 200, "{}")]);
    let err = api(&server, &TempDir::new()).whoami().err().map(|e| e.to_string());
    assert_eq!(err, Some(format!("{} does not look like a GitLab server", server.url())));
}

mod list {
    use super::*;

    #[test]
    fn turns_issues_of_the_remote_project_into_issues() {
        let (listed, server) = listed(&Query::default(), &format!("[{NODE}]"));

        let expected = Issue {
            source: Source::Gitlab,
            number: 42,
            key: "#42".into(),
            title: "Login redirect loops".into(),
            url: "https://gitlab.com/acme/shop/-/issues/42".into(),
            state: "open".into(),
            closed: false,
            labels: vec!["bug".into(), "p1".into()],
            assignees: vec!["luis".into()],
            author: "ana".into(),
            updated_at: "2026-09-19T14:00:00.000Z".into(),
        };
        assert_eq!(listed.issues, [expected]);
        assert_eq!(listed.account.map(|a| a.describe()).as_deref(), Some("@ana in 127.0.0.1"));
        assert!(server.request(1).to_lowercase().contains("private-token: t0ken"), "{}", server.request(1));
    }

    #[test]
    fn a_closed_issue_says_so() {
        let node = NODE.replace(r#""state":"opened""#, r#""state":"closed""#);
        let (listed, _server) = listed(&Query { closed: true, ..Query::default() }, &format!("[{node}]"));
        let issue = listed.issues.first().expect("one issue");
        assert_eq!((issue.closed, issue.state.as_str()), (true, "closed"));
    }

    #[test]
    fn leaves_out_rows_without_a_number() {
        let (listed, _server) = listed(&Query::default(), &format!(r#"[{{"title":"draft"}},{NODE}]"#));
        assert_eq!(listed.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [42]);
    }

    #[test]
    fn reads_the_old_single_assignee_and_label_objects() {
        let node = r#"{"iid":3,"assignee":{"username":"marta"},"labels":[{"name":"ux"}]}"#;
        let (listed, _server) = listed(&Query::default(), &format!("[{node}]"));
        let issue = listed.issues.first().expect("one issue");
        assert_eq!((issue.assignees.clone(), issue.labels.clone()), (vec!["marta".into()], vec!["ux".into()]));
    }

    #[test]
    fn asks_for_the_last_updated_first() {
        let sent = query_sent(&Query::default());
        for part in ["per_page=100", "order_by=updated_at", "sort=desc"] {
            assert!(sent.contains(part), "{part} in {sent}");
        }
    }

    #[rstest]
    #[case::open(Query::default(), true)]
    #[case::closed_too(Query { closed: true, ..Query::default() }, false)]
    fn asks_for_open_issues_unless_closed_ones_are_wanted(#[case] query: Query, #[case] open_only: bool) {
        assert_eq!(query_sent(&query).contains("state=opened"), open_only);
    }

    #[rstest]
    #[case::me_as_assignee([Who::Me, Who::Anyone], "assignee_username=ana")]
    #[case::someone_as_author([Who::Anyone, Who::Person("luis".into())], "author_username=luis")]
    fn people_go_into_the_query(#[case] people: [Who; 2], #[case] part: &str) {
        let sent = query_sent(&Query { people, ..Query::default() });
        assert!(sent.contains(part), "{part} in {sent}");
    }

    #[test]
    fn anyone_adds_nothing() {
        let sent = query_sent(&Query::default());
        assert!(!sent.contains("_username"), "{sent}");
    }

    #[test]
    fn stops_at_the_limit() {
        let rows: Vec<String> = (1..=LIMIT + 5).map(|n| format!(r#"{{"iid":{n}}}"#)).collect();
        let (listed, _server) = listed(&Query::default(), &format!("[{}]", rows.join(",")));
        assert_eq!(listed.issues.len(), LIMIT);
    }

    #[test]
    fn a_project_the_token_cannot_see_says_so() {
        let server = FakeHttp::start(vec![
            ("GET /api/v4/user", 200, USER.to_string()),
            (ISSUES, 404, r#"{"message":"404 Project Not Found"}"#.to_string()),
        ]);
        let err = list_error(&server, &on_the_server(&server));
        assert_eq!(err.as_deref(), Some("no GitLab project acme/shop on 127.0.0.1, or the token cannot see it"));
    }

    #[test]
    fn a_repository_without_a_remote_on_the_server_says_where_its_remotes_are() {
        let server = FakeHttp::start(vec![("GET /api/v4/user", 200, USER)]);
        let repo = repo(&[("origin", "git@github.com:acme/shop.git"), ("work", "git@gitlab.acme.dev:acme/shop.git")]);
        assert_eq!(
            list_error(&server, &repo).as_deref(),
            Some(
                "no remote of this repository is on 127.0.0.1 (origin is on github.com, work is on gitlab.acme.dev): \
                 set the GitLab URL in settings to https://gitlab.acme.dev"
            )
        );
    }

    #[test]
    fn a_repository_without_remotes_says_so() {
        let server = FakeHttp::start(vec![("GET /api/v4/user", 200, USER)]);
        let err = list_error(&server, &repo(&[]));
        assert_eq!(err.as_deref(), Some("this repository has no remote, so it has no project on 127.0.0.1"));
    }
}

#[test]
fn people_are_the_active_members_of_the_project() {
    let members = r#"[{"username":"marta","name":"Marta","state":"active"},
        {"username":"ana","name":"Ana Pérez","state":"active"},
        {"username":"gone","name":"Gone","state":"blocked"},
        {"username":"project_1_bot","name":"Bot","state":"active","bot":true}]"#;
    let server = FakeHttp::start(vec![("GET /api/v4/projects/acme%2Fshop/members/all", 200, members)]);
    let repo = on_the_server(&server);

    let people = api(&server, &repo).people().expect("people");

    assert_eq!(
        people,
        [
            Person { handle: "ana".into(), name: "Ana Pérez".into(), id: None },
            Person { handle: "marta".into(), name: "Marta".into(), id: None }
        ]
    );
    assert!(decoded(&server.request(0)).contains("page=1"), "{}", server.request(0));
}

#[test]
fn people_follow_full_pages() {
    let page: Vec<String> = (0..PAGE).map(|n| format!(r#"{{"username":"u{n:03}"}}"#)).collect();
    let server = FakeHttp::start(vec![
        ("GET /api/v4/projects/acme%2Fshop/members/all", 200, format!("[{}]", page.join(","))),
        ("GET /api/v4/projects/acme%2Fshop/members/all", 200, r#"[{"username":"zoe"}]"#.to_string()),
    ]);
    let repo = on_the_server(&server);

    let people = api(&server, &repo).people().expect("people");

    assert_eq!((people.len(), server.requests().len()), (PAGE + 1, 2));
    assert!(decoded(&server.request(1)).contains("page=2"), "{}", server.request(1));
}

#[test]
fn view_reads_the_description_and_the_comments() {
    let node = r#"{"iid":42,"state":"opened","confidential":true,"labels":["bug"],
        "milestone":{"title":"v2"},"due_date":"2026-10-01","assignees":[{"username":"luis"}],
        "author":{"username":"ana"},"description":"It **loops**.\n"}"#;
    let notes = r#"[{"body":"earlier","created_at":"2026-09-20T10:00:00Z","author":{"username":"ana"},"system":false},
        {"body":"changed the description","created_at":"2026-09-20T12:00:00Z","author":{"username":"ana"},"system":true},
        {"body":"  ","created_at":"2026-09-20T13:00:00Z","author":{"username":"ana"}},
        {"body":"later","created_at":"2026-09-21T10:00:00Z","author":{"username":"luis"},"system":false}]"#;
    let server = FakeHttp::start(vec![
        ("GET /api/v4/projects/acme%2Fshop/issues/42", 200, node.to_string()),
        ("GET /api/v4/projects/acme%2Fshop/issues/42/notes", 200, notes.to_string()),
    ]);
    let repo = on_the_server(&server);

    let detail = api(&server, &repo).view(42).expect("view");

    assert_eq!(
        detail.info,
        ["open", "confidential", "bug", "milestone v2", "due 2026-10-01", "assigned to @luis", "opened by @ana"]
            .map(String::from)
    );
    assert_eq!(detail.body, "It **loops**.");
    assert_eq!(
        detail.comments.iter().map(|c| (c.author.as_str(), c.body.as_str())).collect::<Vec<_>>(),
        [("ana", "earlier"), ("luis", "later")]
    );
    assert!(decoded(&server.request(1)).contains("sort=asc"), "{}", server.request(1));
}

#[test]
fn a_missing_issue_says_so() {
    let server = FakeHttp::start(vec![("GET /api/v4/projects/acme%2Fshop/issues/9", 404, "{}")]);
    let repo = on_the_server(&server);
    let err = api(&server, &repo).view(9).err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("issue #9 not found in acme/shop"));
}

mod remotes {
    use super::*;

    fn found(base: &str, remotes: &[(&str, &str)]) -> Option<String> {
        let remotes: Vec<Remote> = remotes.iter().filter_map(|(name, url)| remote(name, url)).collect();
        project_of(base, &remotes)
    }

    #[rstest]
    #[case::scp_like("git@gitlab.com:acme/shop.git", "acme/shop")]
    #[case::ssh_url("ssh://git@gitlab.com:2222/acme/shop.git", "acme/shop")]
    #[case::https("https://gitlab.com/acme/sub/shop.git", "acme/sub/shop")]
    #[case::https_with_a_login("https://oauth2:secret@gitlab.com/acme/shop", "acme/shop")]
    #[case::trailing_slash("https://gitlab.com/acme/shop/", "acme/shop")]
    #[case::uppercase_host("git@GitLab.com:acme/shop.git", "acme/shop")]
    #[case::alternate_ssh_host("ssh://git@altssh.gitlab.com:443/acme/shop.git", "acme/shop")]
    fn a_remote_on_the_server_names_its_project(#[case] url: &str, #[case] project: &str) {
        assert_eq!(found(CLOUD, &[("origin", url)]).as_deref(), Some(project));
    }

    #[rstest]
    #[case::another_host("git@github.com:acme/shop.git")]
    #[case::a_look_alike_host("https://notgitlab.com/acme/shop.git")]
    #[case::a_local_path("/srv/git/shop.git")]
    #[case::no_group("https://gitlab.com/shop.git")]
    fn other_remotes_name_nothing(#[case] url: &str) {
        assert_eq!(found(CLOUD, &[("origin", url)]), None);
    }

    #[test]
    fn a_server_under_a_path_drops_it_from_web_remotes_only() {
        let base = "https://acme.dev/gitlab/";
        assert_eq!(found(base, &[("origin", "https://acme.dev/gitlab/team/shop.git")]).as_deref(), Some("team/shop"));
        assert_eq!(found(base, &[("origin", "git@acme.dev:gitlab/shop.git")]).as_deref(), Some("gitlab/shop"));
    }

    #[test]
    fn origin_comes_first() {
        let output = "fork\tgit@gitlab.com:ana/shop.git (fetch)\nfork\tgit@gitlab.com:ana/shop.git (push)\n\
                      origin\tgit@gitlab.com:acme/shop.git (fetch)\norigin\tgit@gitlab.com:acme/shop.git (push)\n";
        let remotes = remotes(output);
        assert_eq!(remotes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["origin", "fork"]);
        assert_eq!(project_of(CLOUD, &remotes).as_deref(), Some("acme/shop"));
    }
}

#[rstest]
#[case::blank("  ", CLOUD)]
#[case::trimmed(" https://gitlab.acme.dev/ ", "https://gitlab.acme.dev")]
fn an_empty_server_is_gitlab_com(#[case] url: &str, #[case] expected: &str) {
    assert_eq!(server(url), expected);
}

#[rstest]
#[case::https("https://gitlab.acme.dev", false)]
#[case::http("http://gitlab.acme.dev/", true)]
#[case::http_with_a_port("http://10.0.0.5:8080", true)]
#[case::localhost("http://localhost:8080", false)]
#[case::loopback_address("http://127.0.0.1:3000", false)]
#[case::a_name_starting_like_one("http://127.example.com", true)]
#[case::ipv6_loopback("http://[::1]:3000", false)]
#[case::another_ipv6_address("http://[fd00::1]:3000", true)]
#[case::gitlab_com("", false)]
fn only_http_to_another_machine_is_unencrypted(#[case] url: &str, #[case] expected: bool) {
    assert_eq!(unencrypted(url), expected);
}

#[rstest]
#[case::blank("  ", Ok(""))]
#[case::https("https://gitlab.acme.dev/", Ok("https://gitlab.acme.dev"))]
#[case::no_scheme("gitlab.acme.dev", Err("a URL starts with https://, such as https://gitlab.acme.dev"))]
fn a_url_is_blank_or_http(#[case] input: &str, #[case] expected: std::result::Result<&str, &str>) {
    assert_eq!(check_url(input), expected.map(String::from));
}
