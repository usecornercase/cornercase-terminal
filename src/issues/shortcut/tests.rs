use super::*;
use crate::test_util::FakeHttp;

const MEMBERS: &str = r#"[{"id":"u1","profile":{"mention_name":"ana","name":"Ana"}},
        {"id":"u2","profile":{"mention_name":"","name":"Luis"}}]"#;
const WORKFLOWS: &str = r#"[{"states":[{"id":500,"name":"In Review","type":"started"}]}]"#;
const STORY: &str = r#"{"id":482,"name":"Returns page crashes","app_url":"https://app.shortcut.com/acme/story/482",
        "completed":false,"archived":false,"workflow_state_id":500,"story_type":"bug","estimate":3,
        "labels":[{"name":"backend"}],"owner_ids":["u2"],"requested_by_id":"u1",
        "updated_at":"2026-09-19T12:00:00Z","created_at":"2026-09-01T12:00:00Z",
        "description":"It **crashes**.","tasks":[{"description":"Reproduce","complete":true}],
        "comments":[{"author_id":"u2","text":"later","created_at":"2026-09-21T00:00:00Z"},
                    {"author_id":"u1","text":"first","created_at":"2026-09-20T00:00:00Z"},
                    {"author_id":"u1","text":"gone","created_at":"2026-09-20T00:00:00Z","deleted":true}]}"#;

type Route = (&'static str, u16, String);

fn lookup_routes() -> Vec<Route> {
    vec![("GET /api/v3/members", 200, MEMBERS.into()), ("GET /api/v3/workflows", 200, WORKFLOWS.into())]
}

fn api_url(server: &FakeHttp) -> String {
    format!("{}/api/v3", server.url())
}

#[test]
fn whoami_reads_the_mention_and_the_workspace() {
    let server = FakeHttp::start(vec![("GET /api/v3/member", 200, crate::test_util::SHORTCUT_MEMBER)]);
    let base = api_url(&server);

    let account = Api { base: &base, token: "t0k" }.whoami().expect("whoami");

    assert_eq!(account, Account { handle: "ana".into(), workspace: "acme".into() });
    assert!(server.request(0).to_lowercase().contains("shortcut-token: t0k"));
}

#[test]
fn a_redirect_is_not_followed_so_the_token_stays_on_the_server() {
    let (server, elsewhere) = crate::test_util::redirected("GET /api/v3/member");
    let base = api_url(&server);
    let err = Api { base: &base, token: "t0k" }.whoami().err().map(|e| e.to_string());
    assert!(err.as_deref().is_some_and(|e| e.starts_with("Shortcut answered 302 with a redirect")), "{err:?}");
    assert_eq!(elsewhere.requests(), Vec::<String>::new());
}

#[test]
fn a_rejected_token_says_so() {
    let server = FakeHttp::start(vec![("GET /api/v3/member", 401, "{}")]);
    let base = api_url(&server);
    let err = Api { base: &base, token: "bad" }.whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("Shortcut rejected the token"));
}

mod list {
    use super::*;

    fn listed(query: &Query, pages: Vec<Route>) -> (Listed, FakeHttp) {
        let mut routes = vec![("GET /api/v3/member", 200, crate::test_util::SHORTCUT_MEMBER.to_string())];
        routes.extend(lookup_routes());
        routes.extend(pages);
        let server = FakeHttp::start(routes);
        let base = api_url(&server);
        let listed = Api { base: &base, token: "t0k" }.list(query).expect("list");
        (listed, server)
    }

    fn search_request(server: &FakeHttp) -> String {
        server.requests().into_iter().find(|r| r.starts_with("GET /api/v3/search")).expect("a search")
    }

    #[test]
    fn turns_stories_into_issues() {
        let page = format!(r#"{{"data":[{STORY}],"next":null}}"#);
        let (listed, _server) = listed(&Query::default(), vec![("GET /api/v3/search/stories", 200, page)]);

        let expected = Issue {
            source: Source::Shortcut,
            number: 482,
            key: "sc-482".into(),
            title: "Returns page crashes".into(),
            url: "https://app.shortcut.com/acme/story/482".into(),
            state: "In Review".into(),
            closed: false,
            labels: vec!["backend".into()],
            assignees: vec!["Luis".into()],
            author: "ana".into(),
            updated_at: "2026-09-19T12:00:00Z".into(),
        };
        assert_eq!((listed.issues, listed.account.map(|a| a.handle)), (vec![expected], Some("ana".into())));
    }

    #[test]
    fn follows_the_next_pages() {
        let first = r#"{"data":[{"id":1,"name":"a"}],"next":"/api/v3/search/stories?next=abc"}"#;
        let second = r#"{"data":[{"id":2,"name":"b"}],"next":null}"#;
        let pages = vec![
            ("GET /api/v3/search/stories", 200, first.to_string()),
            ("GET /api/v3/search/stories", 200, second.to_string()),
        ];

        let (listed, _server) = listed(&Query::default(), pages);

        assert_eq!(listed.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn searches_the_open_stories() {
        let (_, server) = listed(&Query::default(), vec![("GET /api/v3/search/stories", 200, r#"{"data":[]}"#.into())]);
        let request = search_request(&server);
        assert!(request.contains("query=!is%3Adone%20!is%3Aarchived&"), "{request}");
    }

    #[test]
    fn people_and_closed_change_the_search() {
        let query = Query { closed: true, people: [Who::Me, Who::Person("@bo".into())] };
        let (_, server) = listed(&query, vec![("GET /api/v3/search/stories", 200, r#"{"data":[]}"#.into())]);
        let request = search_request(&server);
        assert!(request.contains("query=!is%3Aarchived%20owner%3Aana%20requester%3Abo&"), "{request}");
    }
}

#[test]
fn people_are_the_active_members_by_mention() {
    let members = r#"[{"id":"1","profile":{"mention_name":"zoe","name":"Zoe"}},
            {"id":"2","disabled":true,"profile":{"mention_name":"gone","name":"Gone"}},
            {"id":"3","profile":{"mention_name":"ana","name":"Ana"}}]"#;
    let server = FakeHttp::start(vec![("GET /api/v3/members", 200, members)]);
    let base = api_url(&server);

    let people = Api { base: &base, token: "t0k" }.people().expect("people");

    assert_eq!(
        people,
        [
            Person { handle: "ana".into(), name: "Ana".into(), id: None },
            Person { handle: "zoe".into(), name: "Zoe".into(), id: None }
        ]
    );
}

#[test]
fn next_links_to_another_host_are_refused() {
    let api = Api { base: "https://api.app.shortcut.com/api/v3", token: "t" };
    assert!(api.next_url("https://evil.example/x").is_err());
}

#[test]
fn view_reads_the_description_tasks_and_comments() {
    let mut routes = lookup_routes();
    routes.push(("GET /api/v3/stories/482", 200, STORY.into()));
    let server = FakeHttp::start(routes);
    let base = api_url(&server);

    let detail = Api { base: &base, token: "t0k" }.view(482).expect("view");
    assert_eq!(Api { base: &base, token: "t0k" }.find(482).expect("find for queue").title, "Returns page crashes");

    assert_eq!(
        detail.info,
        ["In Review", "bug", "estimate 3", "requested by @ana", "owned by @Luis"].map(String::from)
    );
    assert_eq!(detail.body, "It **crashes**.\n\n**Tasks**\n\n- [x] Reproduce");
    assert_eq!(detail.comments.iter().map(|c| c.body.as_str()).collect::<Vec<_>>(), ["first", "later"]);
}

#[test]
fn a_missing_story_says_so() {
    let mut routes = lookup_routes();
    routes.push(("GET /api/v3/stories/9", 404, "{}".into()));
    let server = FakeHttp::start(routes);
    let base = api_url(&server);
    let err = Api { base: &base, token: "t0k" }.view(9).err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("story sc-9 not found"));
}
