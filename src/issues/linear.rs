use serde_json::{Value, json};

use super::http::{self, Service, text};
use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, checklist, join_body};
use crate::error::{Error, Result};

pub const TOKEN_ENV: &str = "LINEAR_API_KEY";
pub const API_ENV: &str = "CORNERCASE_LINEAR_API";
pub const DEFAULT_API: &str = "https://api.linear.app/graphql";
const REJECTED: &str = "Linear rejected the API key";
const SERVICE: Service = Service { name: "Linear", rejected: REJECTED };
const PAGE: usize = 50;
const CLOSED_TYPES: [&str; 2] = ["completed", "canceled"];
const LIST_FIELDS: &str = "id identifier number title url updatedAt state { name type } assignee { displayName } \
    creator { displayName } labels(first: 20) { nodes { name } }";
const VIEW_FIELDS: &str = "description priority priorityLabel estimate project { name } cycle { number name } \
    parent { identifier } children(first: 50) { nodes { identifier title state { type } } } \
    comments(first: 100) { nodes { body createdAt user { displayName } botActor { name } } }";

pub struct Api<'a> {
    pub url: &'a str,
    pub token: &'a str,
}

impl Api<'_> {
    fn request(&self, query: &str, variables: &Value, what: &str) -> Result<Value> {
        let body = json!({ "query": query, "variables": variables });
        let answer = http::post(&SERVICE, self.url, &[("Authorization", self.token)], &body)?;
        if let Some(first) = answer.json["errors"].as_array().and_then(|errors| errors.first()) {
            return Err(failure(first, what));
        }
        if !answer.ok() || answer.json["data"].is_null() {
            return Err(Error::Api(format!("Linear answered {}", answer.status)));
        }
        Ok(answer.json["data"].clone())
    }

    pub fn whoami(&self) -> Result<Account> {
        let data = self.request(
            "query Viewer { viewer { name displayName organization { name urlKey } } }",
            &json!({}),
            "viewer",
        )?;
        let viewer = &data["viewer"];
        let display = text(viewer, "/displayName");
        let handle = if display.is_empty() { text(viewer, "/name") } else { display };
        Ok(Account { handle, workspace: text(viewer, "/organization/urlKey") })
    }

    pub fn people(&self) -> Result<Vec<Person>> {
        let data = self.request(
            "query Users { users(first: 250) { nodes { displayName name active } } }",
            &json!({}),
            "users",
        )?;
        let mut people: Vec<Person> = data["users"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|u| u["active"].as_bool() != Some(false))
            .map(|u| Person { handle: text(u, "/displayName"), name: text(u, "/name"), id: None })
            .filter(|p| !p.handle.is_empty())
            .collect();
        people.sort_by_key(|p| p.handle.to_lowercase());
        Ok(people)
    }

    pub fn list(&self, query: &Query) -> Result<Listed> {
        let account = self.whoami()?;
        let request = format!(
            "query Issues($first: Int!, $after: String, $filter: IssueFilter) {{ issues(first: $first, after: $after, \
             filter: $filter, orderBy: updatedAt) {{ nodes {{ {LIST_FIELDS} }} pageInfo {{ hasNextPage endCursor }} }} }}"
        );
        let filter = filter(query);
        let mut nodes = Vec::new();
        let mut after = Value::Null;
        loop {
            let first = PAGE.min(LIMIT - nodes.len());
            let data =
                self.request(&request, &json!({ "first": first, "after": after, "filter": filter }), "issues")?;
            nodes.extend(data["issues"]["nodes"].as_array().cloned().unwrap_or_default());
            let page = &data["issues"]["pageInfo"];
            if !page["hasNextPage"].as_bool().unwrap_or(false) || nodes.len() >= LIMIT {
                break;
            }
            after = page["endCursor"].clone();
        }
        Ok(Listed { account: Some(account), issues: nodes.iter().map(issue).collect() })
    }

    pub fn view(&self, key: &str) -> Result<Detail> {
        let request = format!("query Issue($id: String!) {{ issue(id: $id) {{ {LIST_FIELDS} {VIEW_FIELDS} }} }}");
        let data = self.request(&request, &json!({ "id": key }), &format!("issue {key}"))?;
        if data["issue"].is_null() {
            return Err(Error::Api(format!("issue {key} not found")));
        }
        Ok(detail(&data["issue"]))
    }
}

fn failure(error: &Value, what: &str) -> Error {
    let code = text(error, "/extensions/code");
    let message = text(error, "/message");
    let lower = message.to_lowercase();
    if code == "AUTHENTICATION_ERROR" || lower.contains("authenticat") {
        return Error::Api(REJECTED.into());
    }
    if code == "RATELIMITED" {
        return Error::Api("Linear rate limit reached, try again in a few minutes".into());
    }
    if lower.contains("entity not found") || lower.contains("could not find") {
        return Error::Api(format!("{what} not found"));
    }
    let presentable = text(error, "/extensions/userPresentableMessage");
    let shown = if presentable.is_empty() { message } else { presentable };
    Error::Api(format!("Linear answered: {}", if shown.is_empty() { "unknown error".into() } else { shown }))
}

fn filter(query: &Query) -> Value {
    let mut parts = Vec::new();
    if !query.closed {
        parts.push(json!({ "state": { "type": { "nin": CLOSED_TYPES } } }));
    }
    for (field, who) in ["assignee", "creator"].iter().zip(&query.people) {
        let user = match who {
            Who::Anyone => continue,
            Who::Me => json!({ "isMe": { "eq": true } }),
            Who::Person(name) | Who::User { name, .. } => {
                json!({ "displayName": { "eq": name.trim_start_matches('@') } })
            }
        };
        parts.push(json!({ (*field): user }));
    }
    if parts.is_empty() { json!({}) } else { json!({ "and": parts }) }
}

fn is_closed(state: &Value) -> bool {
    CLOSED_TYPES.contains(&state["type"].as_str().unwrap_or_default())
}

fn issue(node: &Value) -> Issue {
    let assignee = text(node, "/assignee/displayName");
    Issue {
        source: Source::Linear,
        number: node["number"].as_u64().unwrap_or_default(),
        key: text(node, "/identifier"),
        title: text(node, "/title"),
        url: text(node, "/url"),
        state: text(node, "/state/name"),
        closed: is_closed(&node["state"]),
        labels: node["labels"]["nodes"].as_array().into_iter().flatten().map(|l| text(l, "/name")).collect(),
        assignees: if assignee.is_empty() { Vec::new() } else { vec![assignee] },
        author: text(node, "/creator/displayName"),
        updated_at: text(node, "/updatedAt"),
    }
}

fn detail(node: &Value) -> Detail {
    let issue = issue(node);
    let mut info = vec![issue.state.clone()];
    if node["priority"].as_u64().unwrap_or(0) > 0 {
        info.push(text(node, "/priorityLabel"));
    }
    let project = text(node, "/project/name");
    info.extend((!project.is_empty()).then(|| format!("project {project}")));
    if node["cycle"].is_object() {
        let name = text(node, "/cycle/name");
        info.push(if name.is_empty() { format!("Cycle {}", node["cycle"]["number"]) } else { name });
    }
    info.extend(node["estimate"].as_f64().map(|e| format!("estimate {e}")));
    let parent = text(node, "/parent/identifier");
    info.extend((!parent.is_empty()).then(|| format!("sub-issue of {parent}")));
    info.extend((!issue.author.is_empty()).then(|| format!("created by @{}", issue.author)));
    info.extend(issue.assignees.first().map(|a| format!("assigned to @{a}")));
    info.retain(|part| !part.is_empty());
    let children: Vec<(bool, String)> = node["children"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| (is_closed(&c["state"]), format!("{} {}", text(c, "/identifier"), text(c, "/title"))))
        .collect();
    let mut comments: Vec<Comment> = node["comments"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| !text(c, "/body").trim().is_empty())
        .map(|c| {
            let user = text(c, "/user/displayName");
            Comment {
                author: if user.is_empty() { text(c, "/botActor/name") } else { user },
                created_at: text(c, "/createdAt"),
                body: text(c, "/body"),
            }
        })
        .collect();
    comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Detail { info, body: join_body(&[&text(node, "/description"), &checklist("Sub-issues", &children)]), comments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::FakeHttp;

    const VIEWER: &str =
        r#"{"data":{"viewer":{"name":"Ana Pérez","displayName":"ana","organization":{"urlKey":"acme"}}}}"#;
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
}
