use std::path::PathBuf;

use serde_json::Value;

use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who};
use crate::error::{Error, Result};
use crate::git::worktree;
use crate::http::{self, Answer, Service, into_json, path_segment, text};

pub(crate) const TOKEN_ENV: &str = "GITLAB_TOKEN";
pub(crate) const CLOUD: &str = "https://gitlab.com";
const REJECTED: &str = "GitLab rejected the token: it may have expired or lack the read_api scope";
const SERVICE: Service = Service { name: "GitLab", rejected: REJECTED };
const PAGE: usize = 100;
const PEOPLE_PAGES: usize = 5;

pub(crate) struct Api {
    pub(crate) base: String,
    pub(crate) token: String,
    pub(crate) dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Remote {
    name: String,
    host: String,
    path: String,
    web: bool,
}

pub(crate) fn server(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() { CLOUD.to_string() } else { url.to_string() }
}

pub(crate) fn check_url(input: &str) -> std::result::Result<String, &'static str> {
    super::check_url(input, "a URL starts with https://, such as https://gitlab.acme.dev")
}

pub(crate) fn unencrypted(url: &str) -> bool {
    let Some(rest) = url.trim().strip_prefix("http://") else { return false };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or(authority).to_ascii_lowercase();
    let name = host.split(':').next().unwrap_or_default();
    !(name == "localhost" || name.starts_with("127.") || host.starts_with("[::1]"))
}

pub(crate) const UNENCRYPTED: &str = "the address starts with http://, so the token travels unencrypted";

impl Api {
    fn get(&self, path: &str, query: &[(&str, &str)], missing: &str) -> Result<Value> {
        let url = format!("{}/api/v4{path}", self.base.trim_end_matches('/'));
        let answer = http::get_without_redirects(&SERVICE, &url, query, &[("PRIVATE-TOKEN", &self.token)])?;
        into_json(answer, missing, failure)
    }

    fn host(&self) -> String {
        split_url(&self.base).map_or_else(|| self.base.clone(), |(host, _)| host)
    }

    fn user(&self) -> Result<Value> {
        let user = self.get("/user", &[], "no GitLab user was found for this token")?;
        if text(&user, "/username").is_empty() {
            return Err(Error::Api(format!("{} does not look like a GitLab server", self.base)));
        }
        Ok(user)
    }

    pub(crate) fn whoami(&self) -> Result<Account> {
        let user = self.user()?;
        Ok(Account { handle: text(&user, "/username"), workspace: self.host() })
    }

    fn project(&self) -> Result<String> {
        let out = worktree::check(worktree::git(&self.dir, ["remote", "-v"])?)?;
        let remotes = remotes(&String::from_utf8_lossy(&out));
        project_of(&self.base, &remotes).ok_or_else(|| Error::Api(no_project(&self.host(), &remotes)))
    }

    fn project_path(project: &str, rest: &str) -> String {
        format!("/projects/{}{rest}", path_segment(project))
    }

    fn missing(&self, project: &str) -> String {
        format!("no GitLab project {project} on {}, or the token cannot see it", self.host())
    }

    pub(crate) fn list(&self, query: &Query) -> Result<Listed> {
        let user = self.user()?;
        let project = self.project()?;
        let me = text(&user, "/username");
        let account = Account { handle: me.clone(), workspace: self.host() };
        let per_page = PAGE.to_string();
        let mut params = vec![("per_page", per_page.as_str()), ("order_by", "updated_at"), ("sort", "desc")];
        if !query.closed {
            params.push(("state", "opened"));
        }
        let people: Vec<(&str, String)> = ["assignee_username", "author_username"]
            .into_iter()
            .zip(&query.people)
            .filter_map(|(field, who)| person(who, &me).map(|name| (field, name)))
            .collect();
        params.extend(people.iter().map(|(field, name)| (*field, name.as_str())));
        let rows = self.get(&Self::project_path(&project, "/issues"), &params, &self.missing(&project))?;
        let mut issues: Vec<Issue> = rows.as_array().into_iter().flatten().filter_map(issue).collect();
        issues.truncate(LIMIT);
        Ok(Listed { account: Some(account), issues })
    }

    pub(crate) fn people(&self) -> Result<Vec<Person>> {
        let project = self.project()?;
        let path = Self::project_path(&project, "/members/all");
        let per_page = PAGE.to_string();
        let mut people: Vec<Person> = Vec::new();
        for page in 1..=PEOPLE_PAGES {
            let page = page.to_string();
            let params = [("per_page", per_page.as_str()), ("page", page.as_str())];
            let rows = self.get(&path, &params, &self.missing(&project))?;
            let rows = rows.as_array().cloned().unwrap_or_default();
            people.extend(
                rows.iter()
                    .filter(|m| text(m, "/state").is_empty() || text(m, "/state") == "active")
                    .filter(|m| m["bot"].as_bool() != Some(true))
                    .map(|m| Person { handle: text(m, "/username"), name: text(m, "/name"), id: None })
                    .filter(|p| !p.handle.is_empty()),
            );
            if rows.len() < PAGE {
                break;
            }
        }
        people.sort_by_key(|p| p.handle.to_lowercase());
        people.dedup_by(|a, b| a.handle == b.handle);
        Ok(people)
    }

    pub(crate) fn view(&self, number: u64) -> Result<Detail> {
        let project = self.project()?;
        let path = Self::project_path(&project, &format!("/issues/{number}"));
        let node = self.get(&path, &[], &format!("issue #{number} not found in {project}"))?;
        let per_page = PAGE.to_string();
        let params = [("sort", "asc"), ("order_by", "created_at"), ("per_page", per_page.as_str())];
        let notes = self.get(&format!("{path}/notes"), &params, "the comments were not found")?;
        let comments: Vec<Comment> = notes
            .as_array()
            .into_iter()
            .flatten()
            .filter(|n| n["system"].as_bool() != Some(true))
            .map(|n| Comment {
                author: text(n, "/author/username"),
                created_at: text(n, "/created_at"),
                body: text(n, "/body"),
            })
            .filter(|c| !c.body.trim().is_empty())
            .collect();
        Ok(detail(&node, comments))
    }
}

fn person(who: &Who, me: &str) -> Option<String> {
    match who {
        Who::Anyone => None,
        Who::Me => Some(me.to_string()),
        Who::Person(name) | Who::User { name, .. } => Some(name.clone()),
    }
}

fn issue(node: &Value) -> Option<Issue> {
    let number = node["iid"].as_u64()?;
    let closed = text(node, "/state") == "closed";
    Some(Issue {
        source: Source::Gitlab,
        number,
        key: format!("#{number}"),
        title: text(node, "/title"),
        url: text(node, "/web_url"),
        state: if closed { "closed" } else { "open" }.into(),
        closed,
        labels: labels(node),
        assignees: assignees(node),
        author: text(node, "/author/username"),
        updated_at: text(node, "/updated_at"),
    })
}

fn labels(node: &Value) -> Vec<String> {
    node["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| l.as_str().map_or_else(|| text(l, "/name"), String::from))
        .filter(|l| !l.is_empty())
        .collect()
}

fn assignees(node: &Value) -> Vec<String> {
    let mut names: Vec<String> =
        node["assignees"].as_array().into_iter().flatten().map(|a| text(a, "/username")).collect();
    if names.is_empty() {
        names.push(text(node, "/assignee/username"));
    }
    names.retain(|n| !n.is_empty());
    names
}

fn detail(node: &Value, comments: Vec<Comment>) -> Detail {
    let closed = text(node, "/state") == "closed";
    let labels = labels(node).join(", ");
    let milestone = text(node, "/milestone/title");
    let due = text(node, "/due_date");
    let people: Vec<String> = assignees(node).iter().map(|a| format!("@{a}")).collect();
    let author = text(node, "/author/username");
    let mut info = vec![if closed { "closed" } else { "open" }.to_string()];
    info.extend(node["confidential"].as_bool().is_some_and(|c| c).then(|| "confidential".to_string()));
    info.extend((!labels.is_empty()).then_some(labels));
    info.extend((!milestone.is_empty()).then(|| format!("milestone {milestone}")));
    info.extend((!due.is_empty()).then(|| format!("due {due}")));
    info.extend((!people.is_empty()).then(|| format!("assigned to {}", people.join(" "))));
    info.extend((!author.is_empty()).then(|| format!("opened by @{author}")));
    Detail { info, body: text(node, "/description").trim().to_string(), comments }
}

fn failure(answer: &Answer) -> Error {
    if (300..400).contains(&answer.status) {
        return Error::Api(format!(
            "GitLab answered {} with a redirect, which is not followed so the token stays on this server: set the \
             GitLab URL to the address it leads to",
            answer.status
        ));
    }
    let message = ["/message", "/error_description", "/error"].iter().find_map(|p| {
        let value = answer.json.pointer(p)?;
        let text = value.as_str().map_or_else(|| value.to_string(), String::from);
        (!text.trim().is_empty()).then(|| text.trim().to_string())
    });
    match message {
        Some(message) => Error::Api(format!("GitLab answered: {message}")),
        None => Error::Api(format!("GitLab answered {}", answer.status)),
    }
}

fn split_url(url: &str) -> Option<(String, String)> {
    let (_, rest) = url.split_once("://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    (!host.is_empty()).then(|| (host, path.trim_matches('/').to_string()))
}

fn remote(name: &str, url: &str) -> Option<Remote> {
    let (host, path, web) = if url.contains("://") {
        let (host, path) = split_url(url)?;
        (host, path, url.starts_with("http://") || url.starts_with("https://"))
    } else {
        let (authority, path) = url.split_once(':')?;
        if authority.contains('/') || authority.is_empty() {
            return None;
        }
        let host = authority.rsplit('@').next().unwrap_or(authority).to_ascii_lowercase();
        (host, path.trim_matches('/').to_string(), false)
    };
    let path = path.strip_suffix(".git").unwrap_or(&path).trim_end_matches('/').to_string();
    Some(Remote { name: name.to_string(), host, path, web })
}

fn remotes(output: &str) -> Vec<Remote> {
    let mut remotes: Vec<Remote> = Vec::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url)) = (parts.next(), parts.next()) else { continue };
        if parts.next() == Some("(push)") || remotes.iter().any(|r| r.name == name) {
            continue;
        }
        remotes.extend(remote(name, url));
    }
    remotes.sort_by_key(|r| r.name != "origin");
    remotes
}

fn project_of(base: &str, remotes: &[Remote]) -> Option<String> {
    let (host, prefix) = split_url(base)?;
    remotes
        .iter()
        .filter(|r| r.host == host || r.host.ends_with(&format!(".{host}")))
        .map(|r| {
            let inside = (r.web && !prefix.is_empty()).then(|| r.path.strip_prefix(&format!("{prefix}/"))).flatten();
            inside.unwrap_or(&r.path).to_string()
        })
        .find(|path| path.contains('/'))
}

fn no_project(host: &str, remotes: &[Remote]) -> String {
    if remotes.is_empty() {
        return format!("this repository has no remote, so it has no project on {host}");
    }
    let places: Vec<String> = remotes.iter().map(|r| format!("{} is on {}", r.name, r.host)).collect();
    let hint = remotes
        .iter()
        .find(|r| r.host != host && r.host.contains("gitlab"))
        .map(|r| format!(": set the GitLab URL in settings to https://{}", r.host))
        .unwrap_or_default();
    format!("no remote of this repository is on {host} ({}){hint}", places.join(", "))
}

#[cfg(test)]
mod tests;
