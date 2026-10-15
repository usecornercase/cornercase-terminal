use serde_json::{Value, json};

use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, checklist, join_body};
use crate::error::{Error, Result};
use crate::http::{self, Service, text};

pub(crate) const TOKEN_ENV: &str = "LINEAR_API_KEY";
pub(crate) const API_ENV: &str = "CORNERCASE_LINEAR_API";
pub(crate) const DEFAULT_API: &str = "https://api.linear.app/graphql";
const REJECTED: &str = "Linear rejected the API key";
const SERVICE: Service = Service { name: "Linear", rejected: REJECTED };
const PAGE: usize = 50;
const CLOSED_TYPES: [&str; 2] = ["completed", "canceled"];
const LIST_FIELDS: &str = "id identifier number title url updatedAt state { name type } assignee { displayName } \
    creator { displayName } labels(first: 20) { nodes { name } }";
const VIEW_FIELDS: &str = "description priority priorityLabel estimate project { name } cycle { number name } \
    parent { identifier } children(first: 50) { nodes { identifier title state { type } } } \
    comments(first: 100) { nodes { body createdAt user { displayName } botActor { name } } }";

pub(crate) struct Api<'a> {
    pub(crate) url: &'a str,
    pub(crate) token: &'a str,
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

    pub(crate) fn whoami(&self) -> Result<Account> {
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

    pub(crate) fn people(&self) -> Result<Vec<Person>> {
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

    pub(crate) fn list(&self, query: &Query) -> Result<Listed> {
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

    pub(crate) fn find(&self, key: &str) -> Result<Issue> {
        Ok(issue(&self.node(key)?))
    }

    fn node(&self, key: &str) -> Result<Value> {
        let request = format!("query Issue($id: String!) {{ issue(id: $id) {{ {LIST_FIELDS} {VIEW_FIELDS} }} }}");
        let data = self.request(&request, &json!({ "id": key }), &format!("issue {key}"))?;
        if data["issue"].is_null() {
            return Err(Error::Api(format!("issue {key} not found")));
        }
        Ok(data["issue"].clone())
    }

    pub(crate) fn view(&self, key: &str) -> Result<Detail> {
        Ok(detail(&self.node(key)?))
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
mod tests;
