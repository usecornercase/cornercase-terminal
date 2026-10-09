pub mod html;

use std::net::IpAddr;

use serde_json::Value;

use super::http::{self, Answer, Service, text};
use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, parse_time};
use crate::error::{Error, Result};

pub const TOKEN_ENV: &str = "PLANE_API_KEY";
pub const API_ENV: &str = "CORNERCASE_PLANE_API";
pub const DEFAULT_API: &str = "https://api.plane.so";
pub const DEFAULT_APP: &str = "https://app.plane.so";
const SERVICE: Service = Service { name: "Plane", rejected: "Plane rejected the API key" };
const EXPAND: &str = "state,assignees,labels";

pub struct Api {
    pub base: String,
    pub slug: String,
    pub token: String,
    pub filter: String,
    pub app_url: String,
}

impl Api {
    fn get(&self, path: &str, params: &[(&str, &str)], missing: &str) -> Result<Value> {
        let base = check_url(&self.base).map_err(|e| Error::Api(e.into()))?;
        let url = format!("{base}{path}");
        let answer = http::get_direct(&SERVICE, &url, params, &[("X-API-Key", &self.token)])?;
        http::checked(answer, missing, failure)
    }

    fn me(&self) -> Result<Value> {
        let me = self.get("/api/v1/users/me/", &[], "Plane user not found")?;
        if text(&me, "/id").is_empty() {
            return Err(Error::Api("the URL did not return a Plane user".into()));
        }
        Ok(me)
    }

    fn account(&self, me: &Value) -> Account {
        Account { handle: display_name(me), workspace: self.slug.clone() }
    }

    pub fn whoami(&self) -> Result<Account> {
        let me = self.me()?;
        self.people()?;
        Ok(self.account(&me))
    }

    pub fn people(&self) -> Result<Vec<Person>> {
        let path = format!("/api/v1/workspaces/{}/members/", http::path_segment(&self.slug));
        let page = self.get(&path, &[], "Plane workspace not found or not accessible")?;
        let mut people: Vec<Person> = nodes(&page)
            .iter()
            .map(|user| Person { handle: display_name(user), name: text(user, "/email"), id: Some(text(user, "/id")) })
            .filter(|p| !p.handle.is_empty() && p.id.as_ref().is_some_and(|id| !id.is_empty()))
            .collect();
        people.sort_by_key(|p| p.handle.to_lowercase());
        Ok(people)
    }

    fn pages(&self, path: &str, params: &[(String, String)], missing: &str) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut cursor = String::new();
        loop {
            let mut query: Vec<(&str, &str)> = params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            if !cursor.is_empty() {
                query.push(("cursor", &cursor));
            }
            let page = self.get(path, &query, missing)?;
            let rows = nodes(&page);
            items.extend(rows.iter().take(LIMIT - items.len()).cloned());
            let next = text(&page, "/next_cursor");
            let more = page["has_more"].as_bool().or_else(|| page["next_page_results"].as_bool()).unwrap_or(false);
            if items.len() >= LIMIT || rows.is_empty() || !more || next.is_empty() || next == cursor {
                break;
            }
            cursor = next;
        }
        Ok(items)
    }

    pub fn list(&self, query: &Query) -> Result<Listed> {
        let me = self.me()?;
        let people = self.people()?;
        let mut params = check_filter(&self.filter).map_err(|e| Error::Api(e.into()))?;
        params.extend([
            ("per_page".into(), LIMIT.to_string()),
            ("paginate".into(), "cursor".into()),
            ("order_by".into(), "-updated_at".into()),
            ("expand".into(), EXPAND.into()),
        ]);
        if !query.closed {
            params.push(("state_group__in".into(), "backlog,unstarted,started".into()));
        }
        if let Some(id) = user_id(&query.people[0], &me, &people) {
            params.push(("assignee_id".into(), id));
        }
        let creator = user_id(&query.people[1], &me, &people);
        let path = format!("/api/v2/workspaces/{}/work-items/", http::path_segment(&self.slug));
        let items = self.pages(
            &path,
            &params,
            "Plane workspace list not found: check the workspace and that this instance supports API v2",
        )?;
        let mut issues: Vec<Issue> = items
            .iter()
            .filter(|item| creator.as_ref().is_none_or(|id| created_by(item) == *id))
            .map(|item| self.issue(item, &people))
            .collect();
        issues.sort_by_key(|issue| std::cmp::Reverse(parse_time(&issue.updated_at).unwrap_or(0)));
        Ok(Listed { account: Some(self.account(&me)), issues })
    }

    fn issue(&self, node: &Value, people: &[Person]) -> Issue {
        let number = node["sequence_id"].as_u64().unwrap_or_default();
        let identifier = text(node, "/identifier");
        let key = if identifier.is_empty() {
            format!("{}-{number}", text(node, "/project/identifier"))
        } else if identifier.ends_with(&format!("-{number}")) {
            identifier
        } else {
            format!("{identifier}-{number}")
        };
        Issue {
            source: Source::Plane,
            number,
            url: format!(
                "{}/{}/browse/{}/",
                self.app_url.trim_end_matches('/'),
                http::path_segment(&self.slug),
                http::path_segment(&key)
            ),
            key,
            title: text(node, "/name"),
            state: text(node, "/state/name"),
            closed: matches!(text(node, "/state/group").as_str(), "completed" | "cancelled"),
            labels: nodes(&node["labels"]).iter().map(|label| text(label, "/name")).filter(|s| !s.is_empty()).collect(),
            assignees: nodes(&node["assignees"])
                .iter()
                .map(|user| person_name(user, people))
                .filter(|s| !s.is_empty())
                .collect(),
            author: fallback(person_name(&node["created_by"], people), || resolve_name(&created_by(node), people)),
            updated_at: text(node, "/updated_at"),
        }
    }

    pub fn view(&self, key: &str) -> Result<Detail> {
        let workspace = http::path_segment(&self.slug);
        let path = format!("/api/v1/workspaces/{workspace}/work-items/{}/", http::path_segment(key));
        let node =
            self.get(&path, &[("expand", "project,state,assignees,labels")], &format!("work item {key} not found"))?;
        let people = self.people()?;
        let project =
            fallback(text(&node, "/project/id"), || fallback(text(&node, "/project"), || text(&node, "/project_id")));
        let id = text(&node, "/id");
        if project.is_empty() || id.is_empty() {
            return Err(Error::Api("Plane did not return the work item's project and id".into()));
        }
        let path = format!(
            "/api/v1/workspaces/{workspace}/projects/{}/work-items/{}/comments/",
            http::path_segment(&project),
            http::path_segment(&id)
        );
        let rows = self.pages(&path, &[("per_page".into(), LIMIT.to_string())], "Plane comments not found")?;
        let mut comments: Vec<Comment> = rows
            .iter()
            .map(|c| Comment {
                author: fallback(person_name(&c["actor_detail"], &people), || {
                    fallback(person_name(&c["actor"], &people), || resolve_name(&created_by(c), &people))
                }),
                created_at: text(c, "/created_at"),
                body: body(c, "comment_html", "comment_stripped"),
            })
            .filter(|c| !c.body.trim().is_empty())
            .collect();
        comments.sort_by_key(|c| parse_time(&c.created_at).unwrap_or(0));
        let issue = self.issue(&node, &people);
        let mut info = vec![issue.state, text(&node, "/priority")];
        info.extend((!issue.author.is_empty()).then(|| format!("created by @{}", issue.author)));
        info.extend((!issue.assignees.is_empty()).then(|| format!("assigned to {}", issue.assignees.join(", "))));
        info.retain(|s| !s.is_empty());
        Ok(Detail { info, body: body(&node, "description_html", "description_stripped"), comments })
    }
}

fn fallback(value: String, otherwise: impl FnOnce() -> String) -> String {
    if value.is_empty() { otherwise() } else { value }
}

fn nodes(page: &Value) -> &[Value] {
    page.as_array()
        .or_else(|| page["data"].as_array())
        .or_else(|| page["results"].as_array())
        .map_or(&[], Vec::as_slice)
}

fn display_name(user: &Value) -> String {
    fallback(text(user, "/display_name"), || {
        fallback(format!("{} {}", text(user, "/first_name"), text(user, "/last_name")).trim().to_string(), || {
            text(user, "/email")
        })
    })
}

fn resolve_name(id: &str, people: &[Person]) -> String {
    people.iter().find(|p| p.id.as_deref() == Some(id)).map_or_else(String::new, |p| p.handle.clone())
}

fn person_name(user: &Value, people: &[Person]) -> String {
    if let Some(id) = user.as_str() { resolve_name(id, people) } else { display_name(user) }
}

fn created_by(item: &Value) -> String {
    fallback(text(item, "/created_by_id"), || fallback(text(item, "/created_by/id"), || text(item, "/created_by")))
}

fn user_id(who: &Who, me: &Value, people: &[Person]) -> Option<String> {
    match who {
        Who::Anyone => None,
        Who::Me => Some(text(me, "/id")),
        Who::User { id, .. } => Some(id.clone()),
        Who::Person(name) => {
            Some(people.iter().find(|p| p.handle == *name).and_then(|p| p.id.clone()).unwrap_or_else(|| name.clone()))
        }
    }
}

fn body(node: &Value, rich: &str, plain: &str) -> String {
    html::markdown(node[rich].as_str().unwrap_or_default())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text(node, &format!("/{plain}")))
}

fn failure(answer: &Answer) -> Error {
    let message = ["/detail", "/error"].iter().map(|path| text(&answer.json, path)).find(|s| !s.trim().is_empty());
    Error::Api(message.map_or_else(|| format!("Plane answered {}", answer.status), |s| format!("Plane answered: {s}")))
}

pub fn check_workspace(input: &str) -> std::result::Result<String, &'static str> {
    let slug = input.trim();
    if slug.is_empty() || !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
        return Err("type a Plane workspace slug using lowercase letters, numbers and hyphens");
    }
    Ok(slug.to_string())
}

pub fn check_url(input: &str) -> std::result::Result<String, &'static str> {
    let url = input.trim().trim_end_matches('/');
    if url.is_empty() {
        return Ok(String::new());
    }
    let parsed = url.parse::<ureq::http::Uri>().map_err(|_| "type a URL such as https://plane.example.com")?;
    if parsed.host().is_none() || parsed.query().is_some() || url.contains(['@', '#']) {
        return Err("type an instance URL without credentials, a query or a fragment");
    }
    let host = parsed.host().unwrap_or_default().trim_matches(['[', ']']);
    let loopback = host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if !(parsed.scheme_str() == Some("https") || (parsed.scheme_str() == Some("http") && loopback)) {
        return Err("use HTTPS for remote Plane instances; HTTP is allowed only on loopback hosts");
    }
    Ok(url.to_string())
}

pub fn check_filter(input: &str) -> std::result::Result<Vec<(String, String)>, &'static str> {
    let mut params = Vec::new();
    for part in input.trim().trim_start_matches('?').split('&').filter(|p| !p.is_empty()) {
        let (key, value) = part.split_once('=').ok_or("use query parameters, such as project_id=UUID&priority=high")?;
        let key = decode(key)?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            return Err("filter names use lowercase letters and underscores");
        }
        if [
            "per_page",
            "paginate",
            "cursor",
            "offset",
            "order_by",
            "expand",
            "fields",
            "assignee_id",
            "created_by_id",
            "state_group",
            "state_group__in",
        ]
        .contains(&key.as_str())
        {
            return Err("the filter cannot replace pagination, ordering, expansion or the people and closed toggles");
        }
        if params.iter().any(|(k, _)| *k == key) {
            return Err("each filter parameter can appear only once");
        }
        params.push((key, decode(value)?));
    }
    Ok(params)
}

fn decode(input: &str) -> std::result::Result<String, &'static str> {
    let mut bytes = Vec::new();
    let mut rest = input.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            let hex = tail
                .get(..2)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or("invalid percent encoding in the filter")?;
            bytes.push(hex);
            rest = &tail[2..];
        } else {
            bytes.push(if b == b'+' { b' ' } else { b });
            rest = tail;
        }
    }
    String::from_utf8(bytes).map_err(|_| "the filter must be UTF-8")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;

    use super::*;
    use crate::test_util::FakeHttp;

    const ME: &str = r#"{"id":"ana-id","display_name":"Ana"}"#;
    const MEMBERS: &str =
        r#"[{"id":"ana-id","display_name":"Ana","email":"ana@acme.dev"},{"id":"luis-id","display_name":"Luis"}]"#;
    const LIST: &str = "GET /api/v2/workspaces/acme/work-items/";

    fn api(server: &FakeHttp) -> Api {
        Api {
            base: server.url(),
            slug: "acme".into(),
            token: "secret".into(),
            filter: String::new(),
            app_url: DEFAULT_APP.into(),
        }
    }

    fn item(number: u64, group: &str, author: &str) -> Value {
        json!({
            "id": "item-id", "identifier": format!("ENG-{number}"), "sequence_id": number,
            "project_id": "project-id", "name": "Fix login", "created_by_id": author,
            "state": {"name": "In progress", "group": group}, "labels": [{"name":"bug"}],
            "assignees": [{"id":"luis-id","display_name":"Luis"}],
            "updated_at": "2026-10-01T12:00:00Z"
        })
    }

    fn server(routes: Vec<(&'static str, u16, String)>) -> FakeHttp {
        let mut all = vec![
            ("GET /api/v1/users/me/", 200, ME.into()),
            ("GET /api/v1/workspaces/acme/members/", 200, MEMBERS.into()),
        ];
        all.extend(routes);
        FakeHttp::start(all)
    }

    #[test]
    fn lists_expanded_items_with_auth_filters_and_member_names() {
        let server = server(vec![(
            LIST,
            200,
            json!({"data":[item(42, "started", "ana-id"),item(43,"started","luis-id")]}).to_string(),
        )]);
        let mut api = api(&server);
        api.filter = "project_id=project-id&priority=high".into();
        let query =
            Query { people: [Who::Me, Who::User { id: "ana-id".into(), name: "Ana".into() }], ..Query::default() };
        let listed = api.list(&query).expect("list");
        assert_eq!(listed.account.expect("account").describe(), "@Ana in acme");
        assert_eq!(listed.issues.len(), 1);
        let issue = &listed.issues[0];
        assert_eq!((&issue.key, issue.number, issue.author.as_str()), (&"ENG-42".to_string(), 42, "Ana"));
        assert_eq!(issue.labels, ["bug"]);
        assert_eq!(issue.assignees, ["Luis"]);
        assert_eq!(issue.url, "https://app.plane.so/acme/browse/ENG-42/");
        assert_eq!(super::super::branch(issue), "ENG-42-fix-login");
        let request = server.request(2);
        assert!(request.to_lowercase().contains("x-api-key: secret"));
        for param in [
            "assignee_id=ana-id",
            "project_id=project-id",
            "priority=high",
            "per_page=100",
            "state_group__in=",
            "expand=state",
            "order_by=-updated_at",
            "paginate=cursor",
        ] {
            assert!(request.contains(param), "{request}");
        }
        assert!(!request.contains("created_by"));
    }

    #[rstest]
    #[case::completed("completed", true)]
    #[case::cancelled("cancelled", true)]
    #[case::started("started", false)]
    fn includes_closed_items_when_the_toggle_is_on(#[case] group: &str, #[case] closed: bool) {
        let server = server(vec![(LIST, 200, json!({"data":[item(42,group,"ana-id")]}).to_string())]);
        let listed = api(&server).list(&Query { closed: true, ..Query::default() }).expect("list closed");
        assert_eq!(listed.issues[0].closed, closed);
        assert!(!server.request(2).contains("state_group"));
    }

    #[test]
    fn follows_cursors_and_stops_at_a_hundred_items() {
        let first = json!({"data":[item(1,"started","ana-id")], "next_cursor":"next:1", "has_more":true});
        let second = json!({"data":(2..=110).map(|n| item(n,"started","ana-id")).collect::<Vec<_>>(), "next_cursor":"unused", "has_more":true});
        let server = server(vec![(LIST, 200, first.to_string()), (LIST, 200, second.to_string())]);
        let listed = api(&server).list(&Query::default()).expect("list pages");
        assert_eq!(listed.issues.len(), LIMIT);
        assert!(server.request(3).contains("cursor=next%3A1"));
        assert_eq!(server.requests().len(), 4);
    }

    #[test]
    fn reads_html_and_comments_by_the_key_with_self_hosted_links() {
        let mut node = item(42, "started", "ana-id");
        node["project"] = json!({"id":"project-id","identifier":"ENG"});
        node["description_html"] = json!("<h2>Problem</h2><p>Fix login.</p>");
        let comments = json!({"results":[
            {"actor":"luis-id","created_at":"2026-10-02T00:00:00Z","comment_html":"<p>Later</p>"},
            {"actor_detail":{"display_name":"Ana"},"created_at":"2026-10-01T00:00:00Z","comment_html":"<p>Broken","comment_stripped":"Earlier"}
        ]});
        let server = server(vec![
            ("GET /api/v1/workspaces/acme/work-items/ENG-42/", 200, node.to_string()),
            ("GET /api/v1/workspaces/acme/projects/project-id/work-items/item-id/comments/", 200, comments.to_string()),
        ]);
        let mut api = api(&server);
        api.app_url = "https://plane.example.com/".into();
        let detail = api.view("ENG-42").expect("view");
        assert_eq!(detail.body, "## Problem\n\nFix login.");
        assert_eq!(
            detail.comments.iter().map(|c| (c.author.as_str(), c.body.as_str())).collect::<Vec<_>>(),
            [("Ana", "Earlier"), ("Luis", "Later")]
        );
        assert_eq!(api.issue(&node, &[]).url, "https://plane.example.com/acme/browse/ENG-42/");
    }

    #[rstest]
    #[case::unauthorized(401, "{}", "Plane rejected the API key")]
    #[case::rate_limited(429, "{}", "Plane rate limit reached")]
    #[case::missing(404, "{}", "work item ENG-42 not found")]
    #[case::detail(400, r#"{"detail":"Bad filter"}"#, "Plane answered: Bad filter")]
    #[case::error(400, r#"{"error":"Bad key"}"#, "Plane answered: Bad key")]
    #[case::plain(500, "not JSON", "Plane answered 500")]
    fn explains_failures(#[case] status: u16, #[case] body: &str, #[case] expected: &str) {
        let server = FakeHttp::start(vec![("GET /api/v1/workspaces/acme/work-items/ENG-42/", status, body)]);
        let error = api(&server).view("ENG-42").expect_err("request fails").to_string();
        assert!(error.contains(expected), "{error}");
    }

    #[test]
    fn checks_workspace_access_before_saving_a_key() {
        let server = FakeHttp::start(vec![
            ("GET /api/v1/users/me/", 200, ME),
            ("GET /api/v1/workspaces/acme/members/", 404, "{}"),
        ]);
        assert!(api(&server).whoami().expect_err("inaccessible workspace").to_string().contains("workspace not found"));
    }

    #[rstest]
    #[case::valid("acme-42", true)]
    #[case::empty("", false)]
    #[case::uppercase("Acme", false)]
    #[case::path("acme/other", false)]
    fn validates_workspace_slugs(#[case] input: &str, #[case] valid: bool) {
        assert_eq!(check_workspace(input).is_ok(), valid);
    }

    #[rstest]
    #[case::cloud("", true)]
    #[case::instance("https://plane.example.com/", true)]
    #[case::local("http://localhost:8000/plane", true)]
    #[case::ipv4_loopback("http://127.0.0.2:8000", true)]
    #[case::ipv6_loopback("http://[::1]:8000", true)]
    #[case::remote_http("http://plane.example.com", false)]
    #[case::lan_http("http://192.168.1.10:8000", false)]
    #[case::remote_ipv6("http://[2001:db8::1]", false)]
    #[case::lookalike("http://localhost.example.com", false)]
    #[case::credentials("https://user:secret@plane.example.com", false)]
    #[case::query("https://plane.example.com?key=x", false)]
    #[case::fragment("https://plane.example.com/#x", false)]
    #[case::scheme("ftp://plane.example.com", false)]
    fn validates_instance_urls(#[case] input: &str, #[case] valid: bool) {
        assert_eq!(check_url(input).is_ok(), valid);
    }

    #[test]
    fn rejects_remote_http_before_sending_the_key() {
        let api = Api {
            base: "http://plane.example.com".into(),
            slug: "acme".into(),
            token: "secret".into(),
            filter: String::new(),
            app_url: DEFAULT_APP.into(),
        };
        assert!(api.whoami().expect_err("insecure API URL").to_string().contains("use HTTPS"));
    }

    #[test]
    fn does_not_forward_the_key_to_a_redirect_target() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let target = FakeHttp::start(vec![("GET /redirect", 200, ME)]);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect server");
        let base = format!("http://{}", listener.local_addr().expect("redirect address"));
        let location = format!("{}/redirect", target.url());
        let redirect = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("redirect request");
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).expect("read request");
                assert!(n > 0, "complete headers");
                request.extend_from_slice(&buf[..n]);
            }
            write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .expect("write redirect");
        });
        let mut api = api(&target);
        api.base = base;
        assert!(api.whoami().expect_err("redirect rejected").to_string().contains("302"));
        redirect.join().expect("redirect server");
        assert_eq!(target.requests(), Vec::<String>::new());
    }

    #[test]
    fn decodes_filters_once_and_preserves_built_in_controls() {
        assert_eq!(
            check_filter("?search=fix+login%26logout&priority=high").expect("filter"),
            [("search".into(), "fix login&logout".into()), ("priority".into(), "high".into())]
        );
        for invalid in [
            "per_page=1000",
            "expand=project",
            "state_group=completed",
            "assignee_id=x",
            "priority=x&priority=y",
            "search=%ff",
            "search=%",
            "priority",
        ] {
            assert!(check_filter(invalid).is_err(), "{invalid}");
        }
    }
}
