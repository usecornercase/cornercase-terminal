use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{Comment, Detail, Issue, LIMIT, Person, Query, Source, Who};
use crate::error::{Error, Result};

const LIST_FIELDS: &str = "number,title,state,labels,assignees,author,updatedAt,url";
const VIEW_FIELDS: &str = "number,title,state,labels,assignees,author,updatedAt,url,body,comments";
const GH_ENV: [(&str, &str); 4] =
    [("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1"), ("NO_COLOR", "1"), ("GH_PAGER", "cat")];

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    title: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    labels: Vec<Named>,
    #[serde(default)]
    assignees: Vec<Login>,
    author: Option<Login>,
    #[serde(rename = "updatedAt", default)]
    updated_at: String,
    url: String,
}

#[derive(Deserialize)]
struct RawView {
    #[serde(flatten)]
    issue: RawIssue,
    #[serde(default)]
    body: String,
    #[serde(default)]
    comments: Vec<RawComment>,
}

#[derive(Deserialize)]
struct RawComment {
    author: Option<Login>,
    #[serde(default)]
    body: String,
    #[serde(rename = "createdAt", default)]
    created_at: String,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

impl From<RawIssue> for Issue {
    fn from(raw: RawIssue) -> Self {
        let closed = raw.state.eq_ignore_ascii_case("closed");
        Self {
            source: Source::Github,
            number: raw.number,
            key: format!("#{}", raw.number),
            title: raw.title,
            url: raw.url,
            state: if closed { "closed" } else { "open" }.into(),
            closed,
            labels: raw.labels.into_iter().map(|l| l.name).collect(),
            assignees: raw.assignees.into_iter().map(|a| a.login).collect(),
            author: raw.author.map(|a| a.login).unwrap_or_default(),
            updated_at: raw.updated_at,
        }
    }
}

fn person(who: &Who) -> Option<String> {
    match who {
        Who::Anyone => None,
        Who::Me => Some("@me".into()),
        Who::Person(login) | Who::User { name: login, .. } => Some(login.clone()),
    }
}

pub(crate) fn list(gh: &Path, dir: &Path, query: &Query) -> Result<Vec<Issue>> {
    let state = if query.closed { "all" } else { "open" };
    let limit = LIMIT.to_string();
    let mut args: Vec<String> =
        ["issue", "list", "--state", state, "--limit", &limit, "--json", LIST_FIELDS].map(String::from).to_vec();
    for (flag, who) in ["--assignee", "--author"].iter().zip(&query.people) {
        if let Some(person) = person(who) {
            args.extend([(*flag).to_string(), person]);
        }
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw: Vec<RawIssue> = run(gh, dir, &args)?;
    Ok(raw.into_iter().map(Issue::from).collect())
}

pub(crate) fn people(gh: &Path, dir: &Path) -> Result<Vec<Person>> {
    let out = run_raw(gh, dir, &["api", "repos/{owner}/{repo}/assignees", "--paginate", "--jq", ".[].login"])?;
    let mut people: Vec<Person> = String::from_utf8_lossy(&out)
        .lines()
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .map(|login| Person { handle: login.to_string(), name: String::new(), id: None })
        .collect();
    people.sort_by_key(|p| p.handle.to_lowercase());
    Ok(people)
}

pub(crate) fn view(gh: &Path, dir: &Path, number: u64) -> Result<Detail> {
    let number = number.to_string();
    let raw: RawView = run(gh, dir, &["issue", "view", &number, "--json", VIEW_FIELDS])?;
    let issue = Issue::from(raw.issue);
    let mut info = vec![issue.state.clone()];
    info.extend((!issue.labels.is_empty()).then(|| issue.labels.join(", ")));
    info.extend((!issue.assignees.is_empty()).then(|| {
        let people: Vec<String> = issue.assignees.iter().map(|a| format!("@{a}")).collect();
        format!("assigned to {}", people.join(" "))
    }));
    info.extend((!issue.author.is_empty()).then(|| format!("opened by @{}", issue.author)));
    let comments = raw
        .comments
        .into_iter()
        .filter(|c| !c.body.trim().is_empty())
        .map(|c| Comment {
            author: c.author.map(|a| a.login).unwrap_or_default(),
            created_at: c.created_at,
            body: c.body,
        })
        .collect();
    Ok(Detail { info, body: raw.body, comments })
}

fn run<T: DeserializeOwned>(gh: &Path, dir: &Path, args: &[&str]) -> Result<T> {
    serde_json::from_slice(&run_raw(gh, dir, args)?).map_err(Error::GhOutput)
}

fn run_raw(gh: &Path, dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new(gh)
        .current_dir(dir)
        .args(args)
        .envs(GH_ENV)
        .stdin(Stdio::null())
        .output()
        .map_err(Error::RunGh)?;
    if !out.status.success() {
        let text = [&out.stderr, &out.stdout].map(|bytes| String::from_utf8_lossy(bytes).into_owned());
        let message = text
            .iter()
            .flat_map(|t| t.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| format!("gh failed with {}", out.status), str::to_string);
        return Err(Error::Gh(message));
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests;
