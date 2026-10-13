use rstest::rstest;

use super::*;
use crate::test_util::{FakeHttp, decoded};

const MYSELF: &str = r#"{"accountId":"me-1","displayName":"Ana Pérez","emailAddress":"ana@acme.dev"}"#;
const NODE: &str = r#"{"key":"SHOP-482","fields":{"summary":"Returns page crashes",
        "status":{"name":"In Progress","statusCategory":{"key":"indeterminate"}},
        "assignee":{"displayName":"Luis"},"reporter":{"displayName":"Ana Pérez"},
        "labels":["backend","p1"],"updated":"2026-09-19T14:00:00.000+0200"}}"#;

type Route = (&'static str, u16, String);

fn api(server: &FakeHttp, jql: &str) -> Api {
    Api {
        base: server.url(),
        site: "acme.atlassian.net".into(),
        email: "ana@acme.dev".into(),
        token: "t0k".into(),
        jql: jql.into(),
    }
}

fn search_page(nodes: &str, next: Option<&str>) -> String {
    match next {
        Some(token) => format!(r#"{{"issues":[{nodes}],"nextPageToken":"{token}","isLast":false}}"#),
        None => format!(r#"{{"issues":[{nodes}],"isLast":true}}"#),
    }
}

fn listed(query: &Query, jql: &str, pages: Vec<Route>) -> (Listed, FakeHttp) {
    let mut routes = vec![("GET /rest/api/3/myself", 200, MYSELF.to_string())];
    routes.extend(pages);
    let server = FakeHttp::start(routes);
    let listed = api(&server, jql).list(query).expect("list");
    (listed, server)
}

#[test]
fn whoami_sends_the_email_and_token_and_names_the_site() {
    let server = FakeHttp::start(vec![("GET /rest/api/3/myself", 200, MYSELF)]);

    let account = api(&server, "").whoami().expect("whoami");

    assert_eq!(account, Account { handle: "Ana Pérez".into(), workspace: "acme.atlassian.net".into() });
    let basic = STANDARD.encode(b"ana@acme.dev:t0k");
    assert!(server.request(0).to_lowercase().contains(&format!("authorization: basic {}", basic.to_lowercase())));
}

#[rstest]
#[case::unauthorized(401, "{}", "Jira rejected the token")]
#[case::forbidden(403, "{}", "Jira rejected the token")]
#[case::rate_limited(429, "{}", "Jira rate limit reached, try again in a minute")]
#[case::missing_site(404, "{}", "no Jira site at acme.atlassian.net")]
#[case::other(500, r#"{"errorMessages":["Something broke."],"errors":{}}"#, "Jira answered: Something broke.")]
#[case::not_jira(200, "{}", "acme.atlassian.net is not a Jira site")]
fn whoami_failures_say_why(#[case] status: u16, #[case] body: &'static str, #[case] message: &str) {
    let server = FakeHttp::start(vec![("GET /rest/api/3/myself", status, body)]);
    let err = api(&server, "").whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some(message));
}

mod list {
    use super::*;

    fn jql_sent(query: &Query, extra: &str) -> String {
        let (_, server) = listed(query, extra, vec![("GET /rest/api/3/search/jql", 200, search_page("", None))]);
        decoded(&server.request(1))
    }

    #[test]
    fn turns_issues_into_issues() {
        let (listed, _server) =
            listed(&Query::default(), "", vec![("GET /rest/api/3/search/jql", 200, search_page(NODE, None))]);

        let expected = Issue {
            source: Source::Jira,
            number: 482,
            key: "SHOP-482".into(),
            title: "Returns page crashes".into(),
            url: "https://acme.atlassian.net/browse/SHOP-482".into(),
            state: "In Progress".into(),
            closed: false,
            labels: vec!["backend".into(), "p1".into()],
            assignees: vec!["Luis".into()],
            author: "Ana Pérez".into(),
            updated_at: "2026-09-19T14:00:00.000+0200".into(),
        };
        assert_eq!(listed.issues, [expected]);
        assert_eq!(listed.account.map(|a| a.describe()).as_deref(), Some("@Ana Pérez in acme.atlassian.net"));
    }

    #[test]
    fn follows_the_next_page_token() {
        let a = r#"{"key":"SHOP-1","fields":{}}"#;
        let b = r#"{"key":"SHOP-2","fields":{}}"#;
        let pages = vec![
            ("GET /rest/api/3/search/jql", 200, search_page(a, Some("tok-2"))),
            ("GET /rest/api/3/search/jql", 200, search_page(b, None)),
        ];

        let (listed, server) = listed(&Query::default(), "", pages);

        assert_eq!(listed.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [1, 2]);
        assert!(server.request(2).contains("nextPageToken=tok-2"), "{}", server.request(2));
        assert!(!server.request(1).contains("nextPageToken"), "{}", server.request(1));
    }

    #[test]
    fn asks_only_for_the_fields_the_list_shows() {
        let (_, server) =
            listed(&Query::default(), "", vec![("GET /rest/api/3/search/jql", 200, search_page("", None))]);
        assert!(decoded(&server.request(1)).contains(&format!("fields={LIST_FIELDS}&")));
    }

    #[rstest]
    #[case::open(Query::default(), "", "statusCategory != Done ORDER BY updated DESC")]
    #[case::closed_alone(Query { closed: true, ..Query::default() }, "", "project IS NOT EMPTY ORDER BY updated DESC")]
    #[case::me_and_someone(
        Query { closed: true, people: [Who::Me, Who::User { id: "acc-\"2".into(), name: "Bo".into() }] },
        "",
        r#"assignee = currentUser() AND reporter = "acc-\"2" ORDER BY updated DESC"#
    )]
    #[case::extra(
        Query::default(),
        " project = SHOP order by created ",
        "(project = SHOP) AND statusCategory != Done ORDER BY updated DESC"
    )]
    fn sends_the_jql(#[case] query: Query, #[case] extra: &str, #[case] expected: &str) {
        assert!(jql_sent(&query, extra).contains(&format!("jql={expected}&")), "{}", jql_sent(&query, extra));
    }

    #[test]
    fn a_bad_query_shows_what_jira_said() {
        let error = r#"{"errorMessages":["Error in the JQL Query: Expecting a field name."],"errors":{}}"#;
        let server = FakeHttp::start(vec![
            ("GET /rest/api/3/myself", 200, MYSELF.to_string()),
            ("GET /rest/api/3/search/jql", 400, error.to_string()),
        ]);
        let err = api(&server, "=").list(&Query::default()).err().map(|e| e.to_string());
        assert_eq!(err.as_deref(), Some("Jira answered: Error in the JQL Query: Expecting a field name."));
    }
}

#[test]
fn people_are_the_active_atlassian_accounts_by_name() {
    let users = r#"[{"accountId":"z","accountType":"atlassian","active":true,"displayName":"Zoe"},
            {"accountId":"g","accountType":"atlassian","active":false,"displayName":"Gone"},
            {"accountId":"b","accountType":"app","active":true,"displayName":"Bot"},
            {"accountId":"c","accountType":"customer","active":true,"displayName":"Customer"},
            {"accountId":"a","accountType":"atlassian","active":true,"displayName":"ana","emailAddress":"ana@acme.dev"}]"#;
    let server = FakeHttp::start(vec![("GET /rest/api/3/users/search", 200, users)]);

    let people = api(&server, "").people().expect("people");

    assert_eq!(
        people,
        [
            Person { handle: "ana".into(), name: "ana@acme.dev".into(), id: Some("a".into()) },
            Person { handle: "Zoe".into(), name: String::new(), id: Some("z".into()) }
        ]
    );
    assert!(server.request(0).contains("maxResults=1000"), "{}", server.request(0));
}

#[test]
fn view_reads_the_description_sub_tasks_and_comments() {
    let doc = |text: &str| {
        format!(
            r#"{{"type":"doc","version":1,"content":[{{"type":"paragraph","content":[{{"type":"text","text":"{text}","marks":[{{"type":"strong"}}]}}]}}]}}"#
        )
    };
    let node = format!(
        r#"{{"key":"SHOP-482","fields":{{"summary":"Returns page crashes",
            "status":{{"name":"In Progress","statusCategory":{{"key":"indeterminate"}}}},
            "assignee":{{"displayName":"Luis"}},"reporter":{{"displayName":"Ana"}},"labels":[],
            "issuetype":{{"name":"Bug"}},"priority":{{"name":"High"}},
            "parent":{{"key":"SHOP-1","fields":{{"summary":"Returns","issuetype":{{"name":"Epic"}}}}}},
            "components":[{{"name":"web"}},{{"name":"api"}}],"fixVersions":[{{"name":"2.4"}}],
            "subtasks":[{{"key":"SHOP-483","fields":{{"summary":"Fix API","status":{{"statusCategory":{{"key":"done"}}}}}}}}],
            "description":{},
            "comment":{{"comments":[
                {{"author":{{"displayName":"Luis"}},"created":"2026-09-20T10:00:00.000+0200","body":{}}},
                {{"author":{{"displayName":"Ana"}},"created":"2026-09-20T09:30:00.000+0000","body":{}}},
                {{"author":{{"displayName":"Ana"}},"created":"2026-09-21T00:00:00.000+0000","body":{{"type":"doc","content":[]}}}}
            ]}}}}}}"#,
        doc("It crashes."),
        doc("earlier"),
        doc("later")
    );
    let server = FakeHttp::start(vec![("GET /rest/api/3/issue/SHOP-482", 200, node)]);

    let detail = api(&server, "").view("SHOP-482").expect("view");

    assert_eq!(
        detail.info,
        [
            "In Progress",
            "Bug",
            "High",
            "epic SHOP-1",
            "components web, api",
            "fix version 2.4",
            "reported by @Ana",
            "assigned to @Luis"
        ]
        .map(String::from)
    );
    assert_eq!(detail.body, "**It crashes.**\n\n**Sub-tasks**\n\n- [x] SHOP-483 Fix API");
    assert_eq!(
        detail.comments.iter().map(|c| (c.author.as_str(), c.body.as_str())).collect::<Vec<_>>(),
        [("Luis", "**earlier**"), ("Ana", "**later**")]
    );
    assert!(decoded(&server.request(0)).contains(&format!("fields={VIEW_FIELDS}")), "{}", server.request(0));
}

#[test]
fn a_missing_issue_says_so() {
    let server = FakeHttp::start(vec![(
        "GET /rest/api/3/issue/SHOP-9",
        404,
        r#"{"errorMessages":["Issue does not exist or you do not have permission to see it."]}"#,
    )]);
    let err = api(&server, "").view("SHOP-9").err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("issue SHOP-9 not found"));
}

#[rstest]
#[case::host("acme.atlassian.net", Ok("acme.atlassian.net"))]
#[case::url("https://Acme.atlassian.net/jira/your-work", Ok("acme.atlassian.net"))]
#[case::name_only(" acme ", Ok("acme.atlassian.net"))]
#[case::custom_domain("jira.acme.dev", Ok("jira.acme.dev"))]
#[case::empty("  ", Err("type your site, such as acme.atlassian.net"))]
#[case::not_a_host("acme corp", Err("a site is a host name, such as acme.atlassian.net"))]
fn a_site_is_a_host_name(#[case] input: &str, #[case] expected: std::result::Result<&str, &str>) {
    assert_eq!(check_site(input), expected.map(String::from));
}

#[rstest]
#[case::fine(" ana@acme.dev ", Ok("ana@acme.dev"))]
#[case::no_at("ana", Err("type the email of your Atlassian account"))]
#[case::no_domain("ana@", Err("type the email of your Atlassian account"))]
fn an_email_needs_an_at(#[case] input: &str, #[case] expected: std::result::Result<&str, &str>) {
    assert_eq!(check_email(input), expected.map(String::from));
}
