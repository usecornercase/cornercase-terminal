use std::collections::HashMap;

use serde_json::Value;

use super::http::{self, Service, text};
use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, checklist, join_body};
use crate::error::{Error, Result};

pub const TOKEN_ENV: &str = "SHORTCUT_API_TOKEN";
pub const API_ENV: &str = "CORNERCASE_SHORTCUT_API";
pub const DEFAULT_API: &str = "https://api.app.shortcut.com/api/v3";
const SERVICE: Service = Service { name: "Shortcut", rejected: "Shortcut rejected the token" };
const OPEN_QUERY: &str = "!is:done !is:archived";
const ANY_QUERY: &str = "!is:archived";
const PAGE_SIZE: &str = "25";

pub struct Api<'a> {
    pub base: &'a str,
    pub token: &'a str,
}

#[derive(Default)]
struct Lookups {
    members: HashMap<String, String>,
    states: HashMap<i64, String>,
}

impl Lookups {
    fn member(&self, id: &Value) -> String {
        id.as_str().and_then(|id| self.members.get(id)).cloned().unwrap_or_default()
    }
}

impl Api<'_> {
    fn request(&self, url: &str, query: &[(&str, &str)], what: &str) -> Result<Value> {
        let answer = http::get(&SERVICE, url, query, &[("Shortcut-Token", self.token)])?;
        if answer.status == 404 {
            return Err(Error::Api(format!("{what} not found")));
        }
        if !answer.ok() {
            let detail = text(&answer.json, "/message");
            let detail = if detail.is_empty() { String::new() } else { format!(": {detail}") };
            return Err(Error::Api(format!("Shortcut answered {}{detail}", answer.status)));
        }
        Ok(answer.json)
    }

    fn path(&self, path: &str) -> String {
        format!("{}{path}", self.base.trim_end_matches('/'))
    }

    pub fn whoami(&self) -> Result<Account> {
        let member = self.request(&self.path("/member"), &[], "member")?;
        Ok(Account { handle: text(&member, "/mention_name"), workspace: text(&member, "/workspace2/url_slug") })
    }

    fn lookups(&self) -> Result<Lookups> {
        let mut lookups = Lookups::default();
        let members = self.request(&self.path("/members"), &[], "members")?;
        for member in members.as_array().into_iter().flatten() {
            let mention = text(member, "/profile/mention_name");
            let name = if mention.is_empty() { text(member, "/profile/name") } else { mention };
            lookups.members.insert(text(member, "/id"), name);
        }
        let workflows = self.request(&self.path("/workflows"), &[], "workflows")?;
        for state in
            workflows.as_array().into_iter().flatten().flat_map(|w| w["states"].as_array().into_iter().flatten())
        {
            if let Some(id) = state["id"].as_i64() {
                lookups.states.insert(id, text(state, "/name"));
            }
        }
        Ok(lookups)
    }

    pub fn people(&self) -> Result<Vec<Person>> {
        let members = self.request(&self.path("/members"), &[], "members")?;
        let mut people: Vec<Person> = members
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| {
                !m["disabled"].as_bool().unwrap_or(false) && !m["profile"]["deactivated"].as_bool().unwrap_or(false)
            })
            .map(|m| Person { handle: text(m, "/profile/mention_name"), name: text(m, "/profile/name"), id: None })
            .filter(|p| !p.handle.is_empty())
            .collect();
        people.sort_by_key(|p| p.handle.to_lowercase());
        Ok(people)
    }

    pub fn list(&self, query: &Query) -> Result<Listed> {
        let account = self.whoami()?;
        let lookups = self.lookups()?;
        let search = search_query(query, &account.handle);
        let mut stories = Vec::new();
        let params = [("query", search.as_str()), ("page_size", PAGE_SIZE), ("detail", "slim")];
        let mut page = self.request(&self.path("/search/stories"), &params, "search")?;
        loop {
            stories.extend(page["data"].as_array().cloned().unwrap_or_default());
            let next = text(&page, "/next");
            if next.is_empty() || stories.len() >= LIMIT {
                break;
            }
            page = self.request(&self.next_url(&next)?, &[], "search")?;
        }
        stories.truncate(LIMIT);
        let issues = stories.iter().map(|story| issue(story, &lookups)).collect();
        Ok(Listed { account: Some(account), issues })
    }

    fn next_url(&self, next: &str) -> Result<String> {
        let origin = http::origin(self.base);
        if next.starts_with('/') {
            return Ok(format!("{origin}{next}"));
        }
        if next.starts_with(&format!("{origin}/")) {
            return Ok(next.to_string());
        }
        Err(Error::Api(format!("refusing to send the Shortcut token to {}", http::origin(next))))
    }

    pub fn view(&self, id: u64) -> Result<Detail> {
        let lookups = self.lookups()?;
        let story = self.request(&self.path(&format!("/stories/{id}")), &[], &format!("story sc-{id}"))?;
        Ok(detail(&story, &lookups))
    }
}

fn mention(name: &str) -> String {
    name.trim_start_matches('@').chars().filter(|c| c.is_alphanumeric() || "._-".contains(*c)).collect()
}

fn search_query(query: &Query, me: &str) -> String {
    let mut search = if query.closed { ANY_QUERY } else { OPEN_QUERY }.to_string();
    for (field, who) in ["owner", "requester"].iter().zip(&query.people) {
        let name = match who {
            Who::Anyone => continue,
            Who::Me => mention(me),
            Who::Person(handle) | Who::User { name: handle, .. } => mention(handle),
        };
        if !name.is_empty() {
            search.push(' ');
            search.push_str(field);
            search.push(':');
            search.push_str(&name);
        }
    }
    search
}

fn issue(story: &Value, lookups: &Lookups) -> Issue {
    let number = story["id"].as_u64().unwrap_or_default();
    let completed = story["completed"].as_bool().unwrap_or(false);
    let archived = story["archived"].as_bool().unwrap_or(false);
    let state = if archived {
        "Archived".to_string()
    } else {
        story["workflow_state_id"]
            .as_i64()
            .and_then(|id| lookups.states.get(&id).cloned())
            .unwrap_or_else(|| if completed { "Done".into() } else { String::new() })
    };
    let updated = text(story, "/updated_at");
    Issue {
        source: Source::Shortcut,
        number,
        key: format!("sc-{number}"),
        title: text(story, "/name"),
        url: text(story, "/app_url"),
        state,
        closed: completed || archived,
        labels: story["labels"].as_array().into_iter().flatten().map(|l| text(l, "/name")).collect(),
        assignees: story["owner_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|id| lookups.member(id))
            .filter(|m| !m.is_empty())
            .collect(),
        author: lookups.member(&story["requested_by_id"]),
        updated_at: if updated.is_empty() { text(story, "/created_at") } else { updated },
    }
}

fn detail(story: &Value, lookups: &Lookups) -> Detail {
    let issue = issue(story, lookups);
    let mut info = vec![issue.state.clone(), text(story, "/story_type")];
    info.extend(story["estimate"].as_f64().map(|e| format!("estimate {e}")));
    info.extend((!issue.author.is_empty()).then(|| format!("requested by @{}", issue.author)));
    info.extend((!issue.assignees.is_empty()).then(|| {
        let owners: Vec<String> = issue.assignees.iter().map(|a| format!("@{a}")).collect();
        format!("owned by {}", owners.join(" "))
    }));
    info.retain(|part| !part.is_empty());
    let tasks: Vec<(bool, String)> = story["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| (t["complete"].as_bool().unwrap_or(false), text(t, "/description")))
        .filter(|(_, text)| !text.is_empty())
        .collect();
    let mut comments: Vec<Comment> = story["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| !c["deleted"].as_bool().unwrap_or(false) && !text(c, "/text").trim().is_empty())
        .map(|c| Comment {
            author: lookups.member(&c["author_id"]),
            created_at: text(c, "/created_at"),
            body: text(c, "/text"),
        })
        .collect();
    comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Detail { info, body: join_body(&[&text(story, "/description"), &checklist("Tasks", &tasks)]), comments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::FakeHttp;

    const MEMBER: &str = r#"{"mention_name":"ana","workspace2":{"url_slug":"acme"}}"#;
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
        let server = FakeHttp::start(vec![("GET /api/v3/member", 200, MEMBER)]);
        let base = api_url(&server);

        let account = Api { base: &base, token: "t0k" }.whoami().expect("whoami");

        assert_eq!(account, Account { handle: "ana".into(), workspace: "acme".into() });
        assert!(server.request(0).to_lowercase().contains("shortcut-token: t0k"));
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
            let mut routes = vec![("GET /api/v3/member", 200, MEMBER.to_string())];
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
            let (_, server) =
                listed(&Query::default(), vec![("GET /api/v3/search/stories", 200, r#"{"data":[]}"#.into())]);
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
}
