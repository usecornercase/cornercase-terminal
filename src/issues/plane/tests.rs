use rstest::rstest;

use super::*;
use crate::test_util::{FakeHttp, decoded};

const ME: &str = r#"{"id":"me-1","display_name":"Ana Pérez","email":"ana@acme.dev"}"#;
const MEMBERS: &str = r#"[{"id":"me-1","display_name":"Ana Pérez","email":"ana@acme.dev","is_active":true},
        {"id":"u-2","display_name":"Luis","email":"luis@acme.dev","is_active":true},
        {"id":"u-3","display_name":"Gone","email":"gone@acme.dev","is_active":false},
        {"id":"u-4","display_name":"Bot","email":"bot@acme.dev","is_bot":true}]"#;
const NODE: &str = r#"{"id":"w-1","identifier":"ENG-42","sequence_id":42,"name":"Login redirect loops",
        "state":{"id":"s-1","name":"In Progress","group":"started"},
        "assignees":[{"id":"u-2","display_name":"Luis"}],"labels":[{"id":"l-1","name":"bug"},{"id":"l-2","name":"p1"}],
        "created_by_id":"me-1","created_at":"2026-09-01T10:00:00.000000Z","updated_at":"2026-09-19T14:00:00.000000Z"}"#;

type Route = (&'static str, u16, String);

fn other_creator() -> String {
    let by_luis = NODE.replace(r#""created_by_id":"me-1""#, r#""created_by_id":"u-2""#);
    by_luis.replace("ENG-42", "ENG-43")
}

fn api(server: &FakeHttp, filter: &str) -> Api {
    Api {
        base: server.url(),
        app: "https://app.plane.so".into(),
        slug: "acme".into(),
        token: "k3y".into(),
        filter: filter.into(),
    }
}

fn page(nodes: &str, next: Option<&str>) -> String {
    match next {
        Some(cursor) => format!(
            r#"{{"data":[{nodes}],"pagination":{{"style":"cursor"}},"has_more":true,"next_cursor":"{cursor}"}}"#
        ),
        None => {
            format!(r#"{{"data":[{nodes}],"pagination":{{"style":"cursor"}},"has_more":false,"next_cursor":null}}"#)
        }
    }
}

fn listed(query: &Query, filter: &str, pages: Vec<Route>) -> (Listed, FakeHttp) {
    let mut routes = vec![
        ("GET /api/v1/users/me/", 200, ME.to_string()),
        ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS.to_string()),
    ];
    routes.extend(pages);
    let server = FakeHttp::start(routes);
    let listed = api(&server, filter).list(query).expect("list");
    (listed, server)
}

fn work_items(body: String) -> Vec<Route> {
    vec![("GET /api/v2/workspaces/acme/work-items/", 200, body)]
}

#[test]
fn whoami_sends_the_api_key_and_names_the_workspace() {
    let server = FakeHttp::start(vec![
        ("GET /api/v1/users/me/", 200, ME),
        ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS),
    ]);

    let account = api(&server, "").whoami().expect("whoami");

    assert_eq!(account, Account { handle: "Ana Pérez".into(), workspace: "acme".into() });
    assert!(server.request(0).to_lowercase().contains("x-api-key: k3y"), "{}", server.request(0));
}

#[rstest]
#[case::unauthorized(401, "{}", "Plane rejected the API key")]
#[case::forbidden(403, "{}", "Plane rejected the API key")]
#[case::rate_limited(429, "{}", "Plane rate limit reached, try again in a minute")]
#[case::no_user(404, "{}", "no Plane user was found for this API key")]
#[case::error_body(500, r#"{"error":"Something broke."}"#, "Plane answered: Something broke.")]
#[case::detail_body(400, r#"{"detail":"Bad filter."}"#, "Plane answered: Bad filter.")]
#[case::empty_body(502, "", "Plane answered 502")]
fn whoami_failures_say_why(#[case] status: u16, #[case] body: &'static str, #[case] message: &str) {
    let server = FakeHttp::start(vec![("GET /api/v1/users/me/", status, body)]);
    let err = api(&server, "").whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some(message));
}

#[test]
fn a_missing_workspace_says_so() {
    let server = FakeHttp::start(vec![
        ("GET /api/v1/users/me/", 200, ME.to_string()),
        ("GET /api/v1/workspaces/acme/members/", 404, "{}".to_string()),
    ]);
    let err = api(&server, "").whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("no Plane workspace named acme"));
}

#[test]
fn something_that_is_not_plane_says_so() {
    let server = FakeHttp::start(vec![("GET /api/v1/users/me/", 200, "{}")]);
    let err = api(&server, "").whoami().err().map(|e| e.to_string());
    assert_eq!(err, Some(format!("{} does not look like a Plane server", server.url())));
}

mod list {
    use super::*;

    fn query_sent(query: &Query, filter: &str) -> String {
        let (_, server) = listed(query, filter, work_items(page("", None)));
        decoded(&server.request(2))
    }

    #[test]
    fn turns_work_items_into_issues() {
        let (listed, _server) = listed(&Query::default(), "", work_items(page(NODE, None)));

        let expected = Issue {
            source: Source::Plane,
            number: 42,
            key: "ENG-42".into(),
            title: "Login redirect loops".into(),
            url: "https://app.plane.so/acme/browse/ENG-42/".into(),
            state: "In Progress".into(),
            closed: false,
            labels: vec!["bug".into(), "p1".into()],
            assignees: vec!["Luis".into()],
            author: "Ana Pérez".into(),
            updated_at: "2026-09-19T14:00:00.000000Z".into(),
        };
        assert_eq!(listed.issues, [expected]);
        assert_eq!(listed.account.map(|a| a.describe()).as_deref(), Some("@Ana Pérez in acme"));
    }

    #[rstest]
    #[case::started("started", false)]
    #[case::backlog("backlog", false)]
    #[case::completed("completed", true)]
    #[case::cancelled("cancelled", true)]
    fn a_finished_state_group_closes_the_issue(#[case] group: &str, #[case] closed: bool) {
        let node = format!(r#"{{"identifier":"ENG-1","sequence_id":1,"state":{{"name":"x","group":"{group}"}}}}"#);
        let query = Query { closed: true, ..Query::default() };
        let (listed, _server) = listed(&query, "", work_items(page(&node, None)));
        assert_eq!(listed.issues.first().map(|i| i.closed), Some(closed));
    }

    #[test]
    fn the_number_comes_from_the_key_when_the_sequence_is_missing() {
        let node = r#"{"identifier":"ENG-7","created_at":"2026-09-01T10:00:00Z"}"#;
        let (listed, _server) = listed(&Query::default(), "", work_items(page(node, None)));
        let issue = listed.issues.first().expect("one issue");
        assert_eq!((issue.number, issue.updated_at.as_str()), (7, "2026-09-01T10:00:00Z"));
    }

    #[test]
    fn leaves_out_rows_without_a_key() {
        let nodes = format!(r#"{{"id":"draft"}},{NODE}"#);
        let (listed, _server) = listed(&Query::default(), "", work_items(page(&nodes, None)));
        assert_eq!(listed.issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["ENG-42"]);
    }

    #[test]
    fn sends_the_api_key_and_the_page_parameters() {
        let (_, server) = listed(&Query::default(), "", work_items(page("", None)));
        let request = decoded(&server.request(2));
        assert!(server.request(2).to_lowercase().contains("x-api-key: k3y"), "{request}");
        for part in ["per_page=100", "expand=state,assignees,labels", "order_by=-updated_at", "paginate=cursor"] {
            assert!(request.contains(part), "{part} in {request}");
        }
    }

    #[rstest]
    #[case::open(Query::default(), "state_group__in=backlog,unstarted,started", true)]
    #[case::closed_too(Query { closed: true, ..Query::default() }, "state_group__in", false)]
    fn asks_for_open_issues_unless_closed_ones_are_wanted(
        #[case] query: Query,
        #[case] part: &str,
        #[case] present: bool,
    ) {
        assert_eq!(query_sent(&query, "").contains(part), present);
    }

    #[rstest]
    #[case::narrows_the_open_groups(false, "state_group__in=started,completed", "started")]
    #[case::closed_keeps_the_filter(true, "state_group__in=completed", "completed")]
    fn the_filter_cannot_list_finished_work_items_without_closed(
        #[case] closed: bool,
        #[case] filter: &str,
        #[case] groups: &str,
    ) {
        let sent = query_sent(&Query { closed, ..Query::default() }, filter);
        let found: Vec<&str> =
            sent.split(['?', '&', ' ']).filter_map(|pair| pair.strip_prefix("state_group__in=")).collect();
        assert_eq!(found, [groups], "{sent}");
    }

    #[test]
    fn a_filter_of_only_finished_groups_lists_nothing_while_closed_is_off() {
        let (listed, server) = listed(&Query::default(), "state_group__in=completed", work_items(page(NODE, None)));
        assert_eq!(listed.issues, Vec::<Issue>::new());
        assert_eq!(server.requests().len(), 2);
    }

    #[test]
    fn an_assignee_of_me_uses_the_id_of_the_account() {
        let query = Query { people: [Who::Me, Who::Anyone], ..Query::default() };
        assert!(query_sent(&query, "").contains("assignee_id=me-1"));
    }

    #[test]
    fn a_picked_assignee_uses_their_id() {
        let luis = Who::User { id: "u-2".into(), name: "Luis".into() };
        let query = Query { people: [luis, Who::Anyone], ..Query::default() };
        assert!(query_sent(&query, "").contains("assignee_id=u-2"));
    }

    #[test]
    fn the_filter_is_added_to_the_query() {
        let sent = query_sent(&Query::default(), " ?priority=high&label_id=l-1 ");
        assert!(sent.contains("priority=high") && sent.contains("label_id=l-1"), "{sent}");
    }

    #[test]
    fn the_creator_is_filtered_after_fetching() {
        let other = other_creator();
        let nodes = format!("{NODE},{other}");
        let luis = Who::User { id: "u-2".into(), name: "Luis".into() };
        let query = Query { people: [Who::Anyone, luis], ..Query::default() };

        let (listed, server) = listed(&query, "", work_items(page(&nodes, None)));

        assert_eq!(listed.issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["ENG-43"]);
        assert!(!decoded(&server.request(2)).contains("created_by"), "{}", server.request(2));
    }

    #[test]
    fn creator_of_me_uses_the_id_of_the_account() {
        let other = other_creator();
        let nodes = format!("{NODE},{other}");
        let query = Query { people: [Who::Anyone, Who::Me], ..Query::default() };

        let (listed, _server) = listed(&query, "", work_items(page(&nodes, None)));

        assert_eq!(listed.issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["ENG-42"]);
    }

    #[test]
    fn follows_the_next_cursor() {
        let a = r#"{"identifier":"ENG-1","sequence_id":1}"#;
        let b = r#"{"identifier":"ENG-2","sequence_id":2}"#;
        let pages = vec![
            ("GET /api/v2/workspaces/acme/work-items/", 200, page(a, Some("c-2"))),
            ("GET /api/v2/workspaces/acme/work-items/", 200, page(b, None)),
        ];

        let (listed, server) = listed(&Query::default(), "", pages);

        assert_eq!(listed.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [1, 2]);
        assert!(server.request(3).contains("cursor=c-2"), "{}", server.request(3));
        assert!(!server.request(2).contains("cursor="), "{}", server.request(2));
    }

    #[test]
    fn stops_at_the_limit() {
        let nodes: Vec<String> =
            (1..=LIMIT + 20).map(|n| format!(r#"{{"identifier":"ENG-{n}","sequence_id":{n}}}"#)).collect();
        let (listed, server) = listed(&Query::default(), "", work_items(page(&nodes.join(","), Some("more"))));
        assert_eq!((listed.issues.len(), server.requests().len()), (LIMIT, 3));
    }

    #[test]
    fn a_bad_filter_shows_what_plane_said() {
        let server = FakeHttp::start(vec![
            ("GET /api/v1/users/me/", 200, ME.to_string()),
            ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS.to_string()),
            ("GET /api/v2/workspaces/acme/work-items/", 400, r#"{"detail":"Unknown filter bogus."}"#.to_string()),
        ]);
        let err = api(&server, "bogus=1").list(&Query::default()).err().map(|e| e.to_string());
        assert_eq!(err.as_deref(), Some("Plane answered: Unknown filter bogus."));
    }
}

#[test]
fn people_are_the_active_members_by_name() {
    let server = FakeHttp::start(vec![("GET /api/v1/workspaces/acme/members/", 200, MEMBERS)]);

    let people = api(&server, "").people().expect("people");

    assert_eq!(
        people,
        [
            Person { handle: "Ana Pérez".into(), name: "ana@acme.dev".into(), id: Some("me-1".into()) },
            Person { handle: "Luis".into(), name: "luis@acme.dev".into(), id: Some("u-2".into()) }
        ]
    );
}

#[test]
fn view_reads_the_description_and_the_comments() {
    let node = r#"{"id":"w-1","project":"p-1","sequence_id":42,"name":"Login redirect loops","priority":"high",
            "target_date":"2026-10-01","created_by":"me-1",
            "state":{"name":"In Progress","group":"started"},"assignees":[{"id":"u-2","display_name":"Luis"}],
            "description_html":"<p>It <strong>loops</strong>.</p><ul><li><p>step</p></li></ul>"}"#;
    let comments = r#"{"results":[
            {"comment_html":"<p>later</p>","created_at":"2026-09-21T10:00:00Z","actor":"u-2"},
            {"comment_html":"","comment_stripped":"earlier","created_at":"2026-09-20T10:00:00Z","actor":"me-1"},
            {"comment_html":"<p></p>","created_at":"2026-09-22T10:00:00Z","actor":"me-1"}]}"#;
    let server = FakeHttp::start(vec![
        ("GET /api/v1/workspaces/acme/work-items/ENG-42/", 200, node.to_string()),
        ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS.to_string()),
        ("GET /api/v1/workspaces/acme/projects/p-1/work-items/w-1/comments/", 200, comments.to_string()),
    ]);

    let detail = api(&server, "").view("ENG-42").expect("view");

    assert_eq!(
        detail.info,
        ["In Progress", "high", "due 2026-10-01", "created by @Ana Pérez", "assigned to @Luis"].map(String::from)
    );
    assert_eq!(detail.body, "It **loops**.\n\n- step");
    assert_eq!(
        detail.comments.iter().map(|c| (c.author.as_str(), c.body.as_str())).collect::<Vec<_>>(),
        [("Ana Pérez", "earlier"), ("Luis", "later")]
    );
    assert!(decoded(&server.request(0)).contains("expand=state,assignees,labels"), "{}", server.request(0));
}

#[test]
fn view_falls_back_to_the_plain_description() {
    let node = r#"{"id":"w-1","project":"p-1","description_html":"","description_stripped":" plain text "}"#;
    let server = FakeHttp::start(vec![
        ("GET /api/v1/workspaces/acme/work-items/ENG-1/", 200, node.to_string()),
        ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS.to_string()),
        ("GET /api/v1/workspaces/acme/projects/p-1/work-items/w-1/comments/", 200, "[]".to_string()),
    ]);
    assert_eq!(api(&server, "").view("ENG-1").expect("view").body, "plain text");
}

#[test]
fn a_missing_work_item_says_so() {
    let server =
        FakeHttp::start(vec![("GET /api/v1/workspaces/acme/work-items/ENG-9/", 404, r#"{"error":"nope"}"#)]);
    let err = api(&server, "").view("ENG-9").err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("work item ENG-9 not found"));
}

#[rstest]
#[case::slug("acme", Ok("acme"))]
#[case::trimmed(" /Acme-Team/ ", Ok("acme-team"))]
#[case::empty("  ", Err("type your workspace, the part after app.plane.so/, such as acme"))]
#[case::not_a_slug("acme corp", Err("a workspace is lowercase letters, numbers and dashes, such as acme"))]
#[case::a_url(
    "https://app.plane.so/acme",
    Err("a workspace is lowercase letters, numbers and dashes, such as acme")
)]
fn a_workspace_is_a_slug(#[case] input: &str, #[case] expected: std::result::Result<&str, &str>) {
    assert_eq!(check_workspace(input), expected.map(String::from));
}

#[rstest]
#[case::blank("  ", Ok(""))]
#[case::https("https://plane.acme.dev/", Ok("https://plane.acme.dev"))]
#[case::http_with_port("http://localhost:8080", Ok("http://localhost:8080"))]
#[case::no_scheme("plane.acme.dev", Err("a URL starts with https://, such as https://plane.acme.dev"))]
#[case::other_scheme("ftp://plane.acme.dev", Err("a URL starts with https://, such as https://plane.acme.dev"))]
#[case::no_host("https://", Err("a URL starts with https://, such as https://plane.acme.dev"))]
fn a_url_is_blank_or_http(#[case] input: &str, #[case] expected: std::result::Result<&str, &str>) {
    assert_eq!(check_url(input), expected.map(String::from));
}

#[rstest]
#[case::cloud("", None, CLOUD_API, CLOUD_APP)]
#[case::cloud_with_a_test_server("", Some("http://127.0.0.1:9"), "http://127.0.0.1:9", CLOUD_APP)]
#[case::self_hosted(" https://plane.acme.dev/ ", None, "https://plane.acme.dev", "https://plane.acme.dev")]
fn cloud_and_self_hosted_servers_have_their_own_endpoints(
    #[case] url: &str,
    #[case] api: Option<&str>,
    #[case] base: &str,
    #[case] app: &str,
) {
    assert_eq!(endpoints(url, api), (base.to_string(), app.to_string()));
}

#[rstest]
#[case::pairs("a=1&b=two", &[("a", "1"), ("b", "two")])]
#[case::question_mark_and_spaces(" ?a = 1 & b=2 ", &[("a", "1"), ("b", "2")])]
#[case::lists("state_id__in=x,y", &[("state_id__in", "x,y")])]
#[case::junk_is_left_out("a&=b&&c=3", &[("c", "3")])]
#[case::empty("", &[])]
fn the_filter_is_raw_query_parameters(#[case] filter: &str, #[case] expected: &[(&str, &str)]) {
    let expected: Vec<(String, String)> = expected.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    assert_eq!(extra_params(filter), expected);
}
