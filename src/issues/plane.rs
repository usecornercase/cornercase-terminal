pub(crate) mod html;

use std::collections::HashMap;

use serde_json::Value;

use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, join_body};
use crate::error::{Error, Result};
use crate::http::{self, Answer, Service, into_json, path_segment, text};
use crate::time::parse_time;

pub(crate) const TOKEN_ENV: &str = "PLANE_API_KEY";
pub(crate) const API_ENV: &str = "CORNERCASE_PLANE_API";
pub(crate) const CLOUD_API: &str = "https://api.plane.so";
pub(crate) const CLOUD_APP: &str = "https://app.plane.so";
const REJECTED: &str = "Plane rejected the API key";
const SERVICE: Service = Service { name: "Plane", rejected: REJECTED };
const PAGE: &str = "100";
const PAGES: usize = 10;
const EXPAND: &str = "state,assignees,labels";
const OPEN: &str = "backlog,unstarted,started";
const ORDER: &str = "-updated_at";

pub(crate) struct Api {
    pub(crate) base: String,
    pub(crate) app: String,
    pub(crate) slug: String,
    pub(crate) token: String,
    pub(crate) filter: String,
}

pub(crate) fn endpoints(url: &str, api: Option<&str>) -> (String, String) {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        (api.unwrap_or(CLOUD_API).to_string(), CLOUD_APP.to_string())
    } else {
        (api.unwrap_or(url).to_string(), url.to_string())
    }
}

impl Api {
    fn get(&self, path: &str, query: &[(&str, &str)], missing: &str) -> Result<Value> {
        let url = format!("{}{path}", self.base.trim_end_matches('/'));
        let answer = http::get(&SERVICE, &url, query, &[("X-API-Key", &self.token)])?;
        into_json(answer, missing, failure)
    }

    fn workspace_path(&self, rest: &str) -> String {
        format!("/api/v1/workspaces/{}{rest}", path_segment(&self.slug))
    }

    fn me(&self) -> Result<Value> {
        let me = self.get("/api/v1/users/me/", &[], "no Plane user was found for this API key")?;
        if text(&me, "/id").is_empty() {
            return Err(Error::Api(format!("{} does not look like a Plane server", self.base)));
        }
        Ok(me)
    }

    fn members(&self) -> Result<Vec<Value>> {
        let missing = format!("no Plane workspace named {}", self.slug);
        let members = self.get(&self.workspace_path("/members/"), &[], &missing)?;
        let rows = members.as_array().or_else(|| members["results"].as_array());
        Ok(rows.cloned().unwrap_or_default())
    }

    pub(crate) fn whoami(&self) -> Result<Account> {
        let me = self.me()?;
        self.members()?;
        Ok(Account { handle: display(&me), workspace: self.slug.clone() })
    }

    pub(crate) fn people(&self) -> Result<Vec<Person>> {
        let mut people: Vec<Person> = self
            .members()?
            .iter()
            .filter(|m| m["is_active"].as_bool() != Some(false) && m["is_bot"].as_bool() != Some(true))
            .map(|m| Person { handle: display(m), name: text(m, "/email"), id: Some(text(m, "/id")) })
            .filter(|p| !p.handle.is_empty() && p.id.as_ref().is_some_and(|id| !id.is_empty()))
            .collect();
        people.sort_by_key(|p| p.handle.to_lowercase());
        Ok(people)
    }

    pub(crate) fn list(&self, query: &Query) -> Result<Listed> {
        let me = self.me()?;
        let names = names(&self.members()?);
        let account = Account { handle: display(&me), workspace: self.slug.clone() };
        let mine = text(&me, "/id");
        let assignee = person(&query.people[0], &mine);
        let creator = person(&query.people[1], &mine);
        let Some(params) = params(query.closed, assignee.as_deref(), &self.filter) else {
            return Ok(Listed { account: Some(account), issues: Vec::new() });
        };
        let path = format!("/api/v2/workspaces/{}/work-items/", path_segment(&self.slug));
        let mut issues = Vec::new();
        let mut cursor = String::new();
        for _ in 0..PAGES {
            let mut pairs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            if !cursor.is_empty() {
                pairs.push(("cursor", cursor.as_str()));
            }
            let page = self.get(&path, &pairs, "the work items were not found")?;
            issues.extend(
                page["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|node| creator.as_deref().is_none_or(|id| text(node, "/created_by_id") == id))
                    .filter_map(|node| self.issue(node, &names)),
            );
            cursor = text(&page, "/next_cursor");
            if issues.len() >= LIMIT || cursor.is_empty() || page["has_more"].as_bool() != Some(true) {
                break;
            }
        }
        issues.truncate(LIMIT);
        Ok(Listed { account: Some(account), issues })
    }

    pub(crate) fn view(&self, key: &str) -> Result<Detail> {
        let path = self.workspace_path(&format!("/work-items/{}/", path_segment(key)));
        let node = self.get(&path, &[("expand", EXPAND)], &format!("work item {key} not found"))?;
        let names = names(&self.members().unwrap_or_default());
        let comments = self.comments(&node, &names)?;
        Ok(detail(&node, &names, comments))
    }

    fn comments(&self, node: &Value, names: &HashMap<String, String>) -> Result<Vec<Comment>> {
        let (project, id) = (text(node, "/project"), text(node, "/id"));
        if project.is_empty() || id.is_empty() {
            return Ok(Vec::new());
        }
        let path = self.workspace_path(&format!(
            "/projects/{}/work-items/{}/comments/",
            path_segment(&project),
            path_segment(&id)
        ));
        let page = self.get(&path, &[("per_page", PAGE)], "the comments were not found")?;
        let rows = page.as_array().or_else(|| page["results"].as_array());
        let mut comments: Vec<Comment> = rows
            .into_iter()
            .flatten()
            .map(|c| Comment {
                author: ["/actor", "/created_by"]
                    .iter()
                    .find_map(|pointer| names.get(&text(c, pointer)).cloned())
                    .unwrap_or_default(),
                created_at: text(c, "/created_at"),
                body: body(c, "/comment_html", "/comment_stripped"),
            })
            .filter(|c| !c.body.trim().is_empty())
            .collect();
        comments.sort_by_key(|c| parse_time(&c.created_at).unwrap_or(0));
        Ok(comments)
    }

    fn issue(&self, node: &Value, names: &HashMap<String, String>) -> Option<Issue> {
        let key = text(node, "/identifier");
        if key.is_empty() {
            return None;
        }
        let number = node["sequence_id"]
            .as_u64()
            .or_else(|| key.rsplit('-').next().and_then(|n| n.parse().ok()))
            .unwrap_or_default();
        let updated_at = ["/updated_at", "/created_at"].iter().map(|p| text(node, p)).find(|t| !t.is_empty());
        Some(Issue {
            source: Source::Plane,
            number,
            url: format!("{}/{}/browse/{key}/", self.app.trim_end_matches('/'), self.slug),
            title: text(node, "/name"),
            state: text(node, "/state/name"),
            closed: matches!(text(node, "/state/group").as_str(), "completed" | "cancelled"),
            labels: labels(node),
            assignees: assignees(node, names),
            author: names.get(&text(node, "/created_by_id")).cloned().unwrap_or_default(),
            updated_at: updated_at.unwrap_or_default(),
            key,
        })
    }
}

fn display(user: &Value) -> String {
    ["/display_name", "/email"].iter().map(|p| text(user, p)).find(|n| !n.is_empty()).unwrap_or_default()
}

fn names(members: &[Value]) -> HashMap<String, String> {
    members
        .iter()
        .map(|m| (text(m, "/id"), display(m)))
        .filter(|(id, name)| !id.is_empty() && !name.is_empty())
        .collect()
}

fn person(who: &Who, me: &str) -> Option<String> {
    match who {
        Who::Anyone => None,
        Who::Me => Some(me.to_string()),
        Who::User { id, .. } | Who::Person(id) => Some(id.clone()),
    }
}

fn labels(node: &Value) -> Vec<String> {
    node["labels"].as_array().into_iter().flatten().map(|l| text(l, "/name")).filter(|n| !n.is_empty()).collect()
}

fn assignees(node: &Value, names: &HashMap<String, String>) -> Vec<String> {
    node["assignees"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| a.as_str().map_or_else(|| display(a), |id| names.get(id).cloned().unwrap_or_default()))
        .filter(|n| !n.is_empty())
        .collect()
}

fn body(node: &Value, html: &str, plain: &str) -> String {
    let markdown = html::markdown(&text(node, html));
    if markdown.is_empty() { text(node, plain).trim().to_string() } else { markdown }
}

fn detail(node: &Value, names: &HashMap<String, String>, comments: Vec<Comment>) -> Detail {
    let priority = text(node, "/priority");
    let target = text(node, "/target_date");
    let author = names.get(&text(node, "/created_by")).cloned().unwrap_or_default();
    let mut info = vec![text(node, "/state/name"), if priority == "none" { String::new() } else { priority }];
    info.extend((!target.is_empty()).then(|| format!("due {target}")));
    info.extend((!author.is_empty()).then(|| format!("created by @{author}")));
    info.extend(assignees(node, names).first().map(|a| format!("assigned to @{a}")));
    info.retain(|part| !part.is_empty());
    Detail { info, body: join_body(&[&body(node, "/description_html", "/description_stripped")]), comments }
}

fn failure(answer: &Answer) -> Error {
    let message =
        ["/error", "/detail", "/message"].iter().map(|p| text(&answer.json, p)).find(|m| !m.trim().is_empty());
    match message {
        Some(message) => Error::Api(format!("Plane answered: {}", message.trim())),
        None => Error::Api(format!("Plane answered {}", answer.status)),
    }
}

pub(crate) fn check_workspace(input: &str) -> std::result::Result<String, &'static str> {
    let slug = input.trim().trim_matches('/').to_ascii_lowercase();
    if slug.is_empty() {
        return Err("type your workspace, the part after app.plane.so/, such as acme");
    }
    if !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("a workspace is lowercase letters, numbers and dashes, such as acme");
    }
    Ok(slug)
}

pub(crate) fn check_url(input: &str) -> std::result::Result<String, &'static str> {
    super::check_url(input, "a URL starts with https://, such as https://plane.acme.dev")
}

pub(crate) fn extra_params(filter: &str) -> Vec<(String, String)> {
    filter
        .trim()
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            let key = key.trim();
            (!key.is_empty()).then(|| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn params(closed: bool, assignee: Option<&str>, filter: &str) -> Option<Vec<(String, String)>> {
    let fixed = [("per_page", PAGE), ("expand", EXPAND), ("order_by", ORDER), ("paginate", "cursor")];
    let mut params: Vec<(String, String)> =
        fixed.into_iter().map(|(key, value)| (key.to_string(), value.to_string())).collect();
    let (groups, extra): (Vec<_>, Vec<_>) =
        extra_params(filter).into_iter().partition(|(key, _)| key == "state_group__in");
    if closed {
        params.extend(groups);
    } else {
        let wanted: Vec<&str> = groups.iter().flat_map(|(_, value)| value.split(',')).map(str::trim).collect();
        let open: Vec<&str> = OPEN.split(',').filter(|group| groups.is_empty() || wanted.contains(group)).collect();
        if open.is_empty() {
            return None;
        }
        params.push(("state_group__in".into(), open.join(",")));
    }
    if let Some(id) = assignee {
        params.push(("assignee_id".into(), id.to_string()));
    }
    params.extend(extra);
    Some(params)
}

#[cfg(test)]
mod tests;
