pub mod adf;

use serde_json::Value;

use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, checklist, join_body};
use crate::clipboard;
use crate::error::{Error, Result};
use crate::http::{self, Answer, Service, path_segment, text};
use crate::time::parse_time;

pub const TOKEN_ENV: &str = "JIRA_API_TOKEN";
pub const API_ENV: &str = "CORNERCASE_JIRA_API";
const REJECTED: &str = "Jira rejected the token";
const SERVICE: Service = Service { name: "Jira", rejected: REJECTED };
const PAGE: usize = 50;
const USERS: &str = "1000";
const LIST_FIELDS: &str = "summary,status,assignee,reporter,labels,updated";
const VIEW_FIELDS: &str = "summary,status,assignee,reporter,labels,updated,issuetype,priority,description,comment,\
    parent,subtasks,components,fixVersions";
const OPEN: &str = "statusCategory != Done";
const ANY: &str = "project IS NOT EMPTY";
const ORDER: &str = "ORDER BY updated DESC";

pub struct Api {
    pub base: String,
    pub site: String,
    pub email: String,
    pub token: String,
    pub jql: String,
}

impl Api {
    fn get(&self, path: &str, query: &[(&str, &str)], missing: &str) -> Result<Value> {
        let login = format!("{}:{}", self.email, self.token);
        let auth = format!("Basic {}", clipboard::base64(login.as_bytes()));
        let url = format!("{}{path}", self.base.trim_end_matches('/'));
        let answer = http::get(&SERVICE, &url, query, &[("Authorization", &auth)])?;
        http::into_json(answer, missing, failure)
    }

    pub fn whoami(&self) -> Result<Account> {
        let me = self.get("/rest/api/3/myself", &[], &format!("no Jira site at {}", self.site))?;
        let name = text(&me, "/displayName");
        if name.is_empty() && text(&me, "/accountId").is_empty() {
            return Err(Error::Api(format!("{} is not a Jira site", self.site)));
        }
        Ok(Account { handle: name, workspace: self.site.clone() })
    }

    pub fn people(&self) -> Result<Vec<Person>> {
        let users = self.get("/rest/api/3/users/search", &[("maxResults", USERS)], "users not found")?;
        let mut people: Vec<Person> = users
            .as_array()
            .into_iter()
            .flatten()
            .filter(|u| u["active"].as_bool() == Some(true) && text(u, "/accountType") == "atlassian")
            .map(|u| Person {
                handle: text(u, "/displayName"),
                name: text(u, "/emailAddress"),
                id: Some(text(u, "/accountId")),
            })
            .filter(|p| !p.handle.is_empty() && p.id.as_ref().is_some_and(|id| !id.is_empty()))
            .collect();
        people.sort_by_key(|p| p.handle.to_lowercase());
        Ok(people)
    }

    pub fn list(&self, query: &Query) -> Result<Listed> {
        let account = self.whoami()?;
        let jql = jql(query, &self.jql);
        let mut issues = Vec::new();
        let mut next = String::new();
        loop {
            let max = PAGE.min(LIMIT - issues.len()).to_string();
            let mut params = vec![("jql", jql.as_str()), ("fields", LIST_FIELDS), ("maxResults", max.as_str())];
            if !next.is_empty() {
                params.push(("nextPageToken", next.as_str()));
            }
            let page = self.get("/rest/api/3/search/jql", &params, "the search was not found")?;
            issues.extend(page["issues"].as_array().into_iter().flatten().map(|node| self.issue(node)));
            next = text(&page, "/nextPageToken");
            if next.is_empty() || page["isLast"].as_bool() == Some(true) || issues.len() >= LIMIT {
                break;
            }
        }
        issues.truncate(LIMIT);
        Ok(Listed { account: Some(account), issues })
    }

    pub fn view(&self, key: &str) -> Result<Detail> {
        let path = format!("/rest/api/3/issue/{}", path_segment(key));
        let node = self.get(&path, &[("fields", VIEW_FIELDS)], &format!("issue {key} not found"))?;
        Ok(self.detail(&node))
    }

    fn issue(&self, node: &Value) -> Issue {
        let key = text(node, "/key");
        let fields = &node["fields"];
        let assignee = text(fields, "/assignee/displayName");
        Issue {
            source: Source::Jira,
            number: key.rsplit('-').next().and_then(|n| n.parse().ok()).unwrap_or_default(),
            url: format!("https://{}/browse/{key}", self.site),
            title: text(fields, "/summary"),
            state: text(fields, "/status/name"),
            closed: is_done(&fields["status"]),
            labels: fields["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect(),
            assignees: if assignee.is_empty() { Vec::new() } else { vec![assignee] },
            author: text(fields, "/reporter/displayName"),
            updated_at: text(fields, "/updated"),
            key,
        }
    }

    fn detail(&self, node: &Value) -> Detail {
        let issue = self.issue(node);
        let fields = &node["fields"];
        let names = |pointer: &str| -> Vec<String> {
            fields.pointer(pointer).and_then(Value::as_array).into_iter().flatten().map(|v| text(v, "/name")).collect()
        };
        let mut info = vec![issue.state.clone(), text(fields, "/issuetype/name"), text(fields, "/priority/name")];
        let parent = text(fields, "/parent/key");
        if !parent.is_empty() {
            let epic = text(fields, "/parent/fields/issuetype/name").eq_ignore_ascii_case("epic");
            info.push(if epic { format!("epic {parent}") } else { format!("child of {parent}") });
        }
        let components = names("/components");
        info.extend((!components.is_empty()).then(|| format!("components {}", components.join(", "))));
        let versions = names("/fixVersions");
        info.extend((!versions.is_empty()).then(|| format!("fix version {}", versions.join(", "))));
        info.extend((!issue.author.is_empty()).then(|| format!("reported by @{}", issue.author)));
        info.extend(issue.assignees.first().map(|a| format!("assigned to @{a}")));
        info.retain(|part| !part.is_empty());
        let subtasks: Vec<(bool, String)> = fields["subtasks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| (is_done(&s["fields"]["status"]), format!("{} {}", text(s, "/key"), text(s, "/fields/summary"))))
            .collect();
        let mut comments: Vec<Comment> = fields["comment"]["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| Comment {
                author: text(c, "/author/displayName"),
                created_at: text(c, "/created"),
                body: adf::markdown(&c["body"]),
            })
            .filter(|c| !c.body.trim().is_empty())
            .collect();
        comments.sort_by_key(|c| parse_time(&c.created_at).unwrap_or(0));
        let description = adf::markdown(&fields["description"]);
        Detail { info, body: join_body(&[&description, &checklist("Sub-tasks", &subtasks)]), comments }
    }
}

fn is_done(status: &Value) -> bool {
    text(status, "/statusCategory/key") == "done"
}

fn failure(answer: &Answer) -> Error {
    let json = &answer.json;
    let messages: Vec<String> = json["errorMessages"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(json["errors"].as_object().into_iter().flat_map(|errors| errors.values()))
        .filter_map(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .map(String::from)
        .collect();
    if messages.is_empty() {
        Error::Api(format!("Jira answered {}", answer.status))
    } else {
        Error::Api(format!("Jira answered: {}", messages.join(" ")))
    }
}

pub fn check_site(input: &str) -> std::result::Result<String, &'static str> {
    let input = input.trim();
    let without_scheme = input.split_once("://").map_or(input, |(_, rest)| rest);
    let host = without_scheme.split('/').next().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err("type your site, such as acme.atlassian.net");
    }
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        return Err("a site is a host name, such as acme.atlassian.net");
    }
    Ok(if host.contains('.') { host } else { format!("{host}.atlassian.net") })
}

pub fn check_email(input: &str) -> std::result::Result<String, &'static str> {
    let email = input.trim();
    match email.split_once('@') {
        Some((user, domain)) if !user.is_empty() && !domain.is_empty() && !email.contains(char::is_whitespace) => {
            Ok(email.to_string())
        }
        _ => Err("type the email of your Atlassian account"),
    }
}

pub fn jql(query: &Query, extra: &str) -> String {
    let mut parts = Vec::new();
    let extra = without_order(extra);
    if !extra.is_empty() {
        parts.push(format!("({extra})"));
    }
    if !query.closed {
        parts.push(OPEN.to_string());
    }
    for (field, who) in Source::Jira.people_fields().iter().zip(&query.people) {
        match who {
            Who::Anyone => {}
            Who::Me => parts.push(format!("{field} = currentUser()")),
            Who::User { id, .. } | Who::Person(id) => parts.push(format!("{field} = {}", quoted(id))),
        }
    }
    if parts.is_empty() {
        parts.push(ANY.to_string());
    }
    format!("{} {ORDER}", parts.join(" AND "))
}

fn without_order(jql: &str) -> &str {
    let jql = jql.trim();
    match jql.to_ascii_lowercase().rfind("order by") {
        Some(i) => jql[..i].trim(),
        None => jql,
    }
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests;
