use super::*;
use crate::test_util::FakeHttp;

const VIEWER: &str = r#"{"data":{"viewer":{"name":"Ana Pérez","displayName":"ana","organization":{"urlKey":"acme"}}}}"#;
const NODE: &str = r#"{"id":"x","identifier":"ENG-123","number":123,"title":"Returns page crashes",
        "url":"https://linear.app/acme/issue/ENG-123/returns","updatedAt":"2026-09-19T12:00:00.000Z",
        "state":{"name":"In Progress","type":"started"},"assignee":{"displayName":"luis"},
        "creator":{"displayName":"ana"},"labels":{"nodes":[{"name":"Bug"}]}"#;

fn server(answers: Vec<String>) -> FakeHttp {
    FakeHttp::start(answers.into_iter().map(|a| ("POST /graphql", 200, a)).collect())
}

fn url(server: &FakeHttp) -> String {
    format!("{}/graphql", server.url())
}

#[test]
fn whoami_reads_the_display_name_and_the_workspace() {
    let server = server(vec![VIEWER.into()]);
    let url = url(&server);

    let account = Api { url: &url, token: "lin_k" }.whoami().expect("whoami");

    assert_eq!(account, Account { handle: "ana".into(), workspace: "acme".into() });
    assert!(server.request(0).to_lowercase().contains("authorization: lin_k"));
}

#[test]
fn an_authentication_error_says_the_key_was_rejected() {
    let server = server(vec![
        r#"{"errors":[{"message":"Authentication required","extensions":{"code":"AUTHENTICATION_ERROR"}}]}"#.into(),
    ]);
    let url = url(&server);
    let err = Api { url: &url, token: "bad" }.whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some(REJECTED));
}

mod list {
    use super::*;

    fn page(nodes: &str, next: Option<&str>) -> String {
        let info = match next {
            Some(cursor) => format!(r#"{{"hasNextPage":true,"endCursor":"{cursor}"}}"#),
            None => r#"{"hasNextPage":false,"endCursor":null}"#.into(),
        };
        format!(r#"{{"data":{{"issues":{{"nodes":[{nodes}],"pageInfo":{info}}}}}}}"#)
    }

    #[test]
    fn turns_nodes_into_issues() {
        let server = server(vec![VIEWER.into(), page(&format!("{NODE}}}"), None)]);
        let url = url(&server);

        let listed = Api { url: &url, token: "k" }.list(&Query::default()).expect("list");

        let expected = Issue {
            source: Source::Linear,
            number: 123,
            key: "ENG-123".into(),
            title: "Returns page crashes".into(),
            url: "https://linear.app/acme/issue/ENG-123/returns".into(),
            state: "In Progress".into(),
            closed: false,
            labels: vec!["Bug".into()],
            assignees: vec!["luis".into()],
            author: "ana".into(),
            updated_at: "2026-09-19T12:00:00.000Z".into(),
        };
        assert_eq!(listed.issues, [expected]);
    }

    #[test]
    fn follows_the_cursor() {
        let a = r#"{"identifier":"ENG-1","number":1,"state":{"type":"started"}}"#;
        let b = r#"{"identifier":"ENG-2","number":2,"state":{"type":"started"}}"#;
        let server = server(vec![VIEWER.into(), page(a, Some("c1")), page(b, None)]);
        let url = url(&server);

        let listed = Api { url: &url, token: "k" }.list(&Query::default()).expect("list");

        assert_eq!(listed.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [1, 2]);
        assert!(server.request(2).contains(r#""after":"c1""#), "{}", server.request(2));
    }

    #[test]
    fn leaves_out_completed_and_canceled_by_default() {
        let server = server(vec![VIEWER.into(), page("", None)]);
        let url = url(&server);
        Api { url: &url, token: "k" }.list(&Query::default()).expect("list");
        assert!(server.request(1).contains(r#"{"and":[{"state":{"type":{"nin":["completed","canceled"]}}}]}"#));
    }

    #[test]
    fn people_go_into_the_filter() {
        let server = server(vec![VIEWER.into(), page("", None)]);
        let url = url(&server);
        let query = Query { closed: true, people: [Who::Me, Who::Person("bo".into())] };
        Api { url: &url, token: "k" }.list(&query).expect("list");
        let expected =
            r#""filter":{"and":[{"assignee":{"isMe":{"eq":true}}},{"creator":{"displayName":{"eq":"bo"}}}]}"#;
        assert!(server.request(1).contains(expected), "{}", server.request(1));
    }

    #[test]
    fn people_are_the_active_users_by_display_name() {
        let users = r#"{"data":{"users":{"nodes":[{"displayName":"zoe","name":"Zoe","active":true},
                {"displayName":"gone","name":"Gone","active":false},{"displayName":"ana","name":"Ana","active":true}]}}}"#;
        let server = server(vec![users.into()]);
        let url = url(&server);

        let people = Api { url: &url, token: "k" }.people().expect("people");

        assert_eq!(people.iter().map(|p| p.handle.as_str()).collect::<Vec<_>>(), ["ana", "zoe"]);
    }
}

#[test]
fn view_reads_the_description_sub_issues_and_comments() {
    let node = format!(
        r#"{NODE},"description":"It **crashes**.","priority":2,"priorityLabel":"High","estimate":3,
            "project":{{"name":"Returns"}},"cycle":{{"number":12,"name":""}},"parent":{{"identifier":"ENG-100"}},
            "children":{{"nodes":[{{"identifier":"ENG-124","title":"Fix API","state":{{"type":"completed"}}}}]}},
            "comments":{{"nodes":[{{"body":"later","createdAt":"2026-09-21T00:00:00Z","user":null,"botActor":{{"name":"GitHub"}}}},
            {{"body":"first","createdAt":"2026-09-20T00:00:00Z","user":{{"displayName":"bo"}}}}]}}}}"#
    );
    let server = server(vec![format!(r#"{{"data":{{"issue":{node}}}}}"#)]);
    let url = url(&server);

    let detail = Api { url: &url, token: "k" }.view("ENG-123").expect("view");

    assert_eq!(
        detail.info,
        [
            "In Progress",
            "High",
            "project Returns",
            "Cycle 12",
            "estimate 3",
            "sub-issue of ENG-100",
            "created by @ana",
            "assigned to @luis"
        ]
        .map(String::from)
    );
    assert_eq!(detail.body, "It **crashes**.\n\n**Sub-issues**\n\n- [x] ENG-124 Fix API");
    assert_eq!(
        detail.comments.iter().map(|c| (c.author.as_str(), c.body.as_str())).collect::<Vec<_>>(),
        [("bo", "first"), ("GitHub", "later")]
    );
}

#[test]
fn a_missing_issue_says_so() {
    let server = server(vec![r#"{"errors":[{"message":"Entity not found: Issue"}]}"#.into()]);
    let url = url(&server);
    let err = Api { url: &url, token: "k" }.view("ENG-9").err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some("issue ENG-9 not found"));
}
