use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use serde_json::Value;

use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, html, join_body};
use crate::error::{Error, Result};
use crate::http::{self, Answer, Service, into_json, path_segment, text};
use crate::time::{Stamp, parse_time};

pub(crate) const TOKEN_ENV: &str = "ASANA_ACCESS_TOKEN";
pub(crate) const API_ENV: &str = "CORNERCASE_ASANA_API";
pub(crate) const DEFAULT_API: &str = "https://app.asana.com/api/1.0";
const REJECTED: &str = "Asana rejected the access token";
const SERVICE: Service = Service { name: "Asana", rejected: REJECTED };
const PAGE: &str = "100";
const LIST_PAGES: usize = 10;
const PEOPLE_PAGES: usize = 20;
const COMMENT_PAGES: usize = 5;
const PROJECTS: usize = 10;
const LIST_FIELDS: &str = "name,completed,modified_at,created_at,permalink_url,resource_subtype,parent,\
    is_rendered_as_separator,assignee.name,created_by,tags.name,memberships.section.name,\
    custom_fields.display_value,custom_fields.representation_type,custom_fields.id_prefix";
const DETAIL_FIELDS: &str = "name,completed,modified_at,created_at,permalink_url,resource_subtype,\
    is_rendered_as_separator,assignee.name,created_by,tags.name,memberships.section.name,\
    custom_fields.display_value,custom_fields.representation_type,custom_fields.id_prefix,\
    html_notes,notes,due_on,memberships.project.name";
const IMAGE: &str = "\u{E000}";
const COMMENT_FIELDS: &str = "resource_subtype,html_text,text,created_by.name,created_at";

pub(crate) struct Api {
    pub(crate) base: String,
    pub(crate) token: String,
    pub(crate) workspace: String,
    pub(crate) projects: Vec<String>,
}

struct Paged<'a> {
    path: &'a str,
    query: &'a [(&'a str, &'a str)],
    missing: &'a str,
    pages: usize,
    cap: usize,
}

struct Me {
    gid: String,
    name: String,
    workspace: String,
    workspace_name: String,
}

impl Api {
    fn get(&self, path: &str, query: &[(&str, &str)], missing: &str) -> Result<Value> {
        let url = format!("{}{path}", self.base.trim_end_matches('/'));
        let bearer = format!("Bearer {}", self.token);
        let answer = http::get_without_redirects(&SERVICE, &url, query, &[("Authorization", &bearer)])?;
        into_json(answer, missing, failure)
    }

    fn rows(&self, paged: &Paged, wanted: impl Fn(&Value) -> bool) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        let mut offset = String::new();
        for _ in 0..paged.pages {
            let mut pairs = paged.query.to_vec();
            if !offset.is_empty() {
                pairs.push(("offset", offset.as_str()));
            }
            let page = self.get(paged.path, &pairs, paged.missing)?;
            rows.extend(page["data"].as_array().into_iter().flatten().filter(|row| wanted(row)).cloned());
            offset = text(&page, "/next_page/offset");
            if rows.len() >= paged.cap || offset.is_empty() {
                break;
            }
        }
        Ok(rows)
    }

    fn profile(&self) -> Result<(Value, Vec<(String, String)>)> {
        let answer = self.get(
            "/users/me",
            &[("opt_fields", "name,email,workspaces.name")],
            "no Asana user was found for this access token",
        )?;
        let user = answer["data"].clone();
        if text(&user, "/gid").is_empty() {
            return Err(Error::Api(format!("{} does not look like the Asana API", self.base)));
        }
        let workspaces = user["workspaces"].as_array().into_iter().flatten();
        let workspaces = workspaces.map(|w| (text(w, "/gid"), text(w, "/name"))).collect();
        Ok((user, workspaces))
    }

    fn me(&self) -> Result<Me> {
        let (user, workspaces) = self.profile()?;
        let (workspace, workspace_name) = choose(&self.workspace, &workspaces)?;
        Ok(Me { gid: text(&user, "/gid"), name: display(&user), workspace, workspace_name })
    }

    fn users(&self, me: &Me) -> Result<Vec<Value>> {
        let path = format!("/workspaces/{}/users", path_segment(&me.workspace));
        let query = [("opt_fields", "name,email"), ("limit", PAGE)];
        let missing = "the workspace users were not found";
        self.rows(&Paged { path: &path, query: &query, missing, pages: PEOPLE_PAGES, cap: usize::MAX }, |_| true)
    }

    pub(crate) fn whoami(&self) -> Result<Account> {
        let (user, workspaces) = self.profile()?;
        let handle = display(&user);
        match choose(&self.workspace, &workspaces) {
            Ok((_, workspace)) => Ok(Account { handle, workspace }),
            Err(e) if workspaces.len() > 1 => Err(Error::Workspaces(Box::new(crate::error::Workspaces {
                handle,
                choices: workspaces,
                message: e.to_string(),
            }))),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn people(&self) -> Result<Vec<Person>> {
        let mut people: Vec<Person> = self
            .users(&self.me()?)?
            .iter()
            .map(|u| Person { handle: text(u, "/name"), name: text(u, "/email"), id: Some(text(u, "/gid")) })
            .filter(|p| !p.handle.is_empty() && p.id.as_ref().is_some_and(|id| !id.is_empty()))
            .collect();
        people.sort_by_key(|p| p.handle.to_lowercase());
        Ok(people)
    }

    pub(crate) fn list(&self, query: &Query) -> Result<Listed> {
        let me = self.me()?;
        let names = names(&self.users(&me)?);
        let account = Account { handle: me.name.clone(), workspace: me.workspace_name.clone() };
        let assignee = self.assignee_filter(query, &me)?;
        let creator = person(&query.people[1], &me.gid);
        let wanted = |task: &Value| {
            text(task, "/resource_subtype") == "default_task"
                && task["is_rendered_as_separator"].as_bool() != Some(true)
                && task["parent"].is_null()
                && assignee.as_deref().is_none_or(|id| text(task, "/assignee/gid") == id)
                && creator.as_deref().is_none_or(|id| text(task, "/created_by/gid") == id)
        };
        let since = (!query.closed).then(|| {
            let now = Stamp(SystemTime::now()).to_string();
            format!("{}Z", now.get(..19).unwrap_or_default())
        });
        let sources = self.list_sources(since.as_deref(), &me.workspace);
        let mut issues = self.collect_tasks(&sources, wanted, &names)?;
        issues.sort_by_key(|i| std::cmp::Reverse(parse_time(&i.updated_at).unwrap_or(0)));
        issues.truncate(LIMIT);
        Ok(Listed { account: Some(account), issues })
    }

    fn assignee_filter(&self, query: &Query, me: &Me) -> Result<Option<String>> {
        if !self.projects.is_empty() {
            return Ok(person(&query.people[0], &me.gid));
        }
        if !matches!(query.people[0], Who::Anyone | Who::Me) {
            return Err(Error::Api(
                "this list is your tasks: set Asana projects in Settings → Issues → Asana to filter by someone else"
                    .into(),
            ));
        }
        Ok(None)
    }

    fn list_sources<'a>(&self, since: Option<&'a str>, workspace: &'a str) -> Vec<(String, Vec<(&'a str, &'a str)>)> {
        let mut base = vec![("opt_fields", LIST_FIELDS), ("limit", PAGE)];
        base.extend(since.map(|since| ("completed_since", since)));
        if self.projects.is_empty() {
            let mut query = base;
            query.extend([("assignee", "me"), ("workspace", workspace)]);
            return vec![("/tasks".to_string(), query)];
        }
        let project = |gid: &String| (format!("/projects/{}/tasks", path_segment(gid)), base.clone());
        self.projects.iter().take(PROJECTS).map(project).collect()
    }

    fn collect_tasks(
        &self,
        sources: &[(String, Vec<(&str, &str)>)],
        wanted: impl Fn(&Value) -> bool,
        names: &HashMap<String, String>,
    ) -> Result<Vec<Issue>> {
        let mut seen = HashSet::new();
        let mut issues = Vec::new();
        for (path, query) in sources {
            let missing = "the tasks were not found";
            let paged = Paged { path, query, missing, pages: LIST_PAGES, cap: LIMIT };
            let tasks = self.rows(&paged, &wanted)?;
            issues.extend(
                tasks.iter().filter(|task| seen.insert(text(task, "/gid"))).filter_map(|task| issue(task, names)),
            );
        }
        Ok(issues)
    }

    fn node(&self, me: &Me, key: &str) -> Result<Value> {
        let missing = format!("task {key} not found");
        let gid = if !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()) {
            key.to_string()
        } else {
            let path = format!("/workspaces/{}/tasks/custom_id/{}", path_segment(&me.workspace), path_segment(key));
            text(&self.get(&path, &[], &missing)?, "/data/gid")
        };
        if gid.is_empty() {
            return Err(Error::Api(missing));
        }
        let node = self.get(&format!("/tasks/{}", path_segment(&gid)), &[("opt_fields", DETAIL_FIELDS)], &missing)?;
        Ok(node["data"].clone())
    }

    pub(crate) fn find(&self, key: &str) -> Result<Issue> {
        let me = self.me()?;
        let node = self.node(&me, key)?;
        issue(&node, &names(&self.users(&me)?)).ok_or_else(|| Error::Api(format!("task {key} has no readable key")))
    }

    pub(crate) fn view(&self, key: &str) -> Result<Detail> {
        let me = self.me()?;
        let node = self.node(&me, key)?;
        let names = names(&self.users(&me).unwrap_or_default());
        let comments = self.comments(&text(&node, "/gid"))?;
        Ok(detail(&node, &names, comments))
    }

    fn comments(&self, gid: &str) -> Result<Vec<Comment>> {
        let path = format!("/tasks/{}/stories", path_segment(gid));
        let query = [("resource_subtype", "comment_added"), ("opt_fields", COMMENT_FIELDS), ("limit", PAGE)];
        let wanted = |story: &Value| text(story, "/resource_subtype") == "comment_added";
        let missing = "the comments were not found";
        let stories =
            self.rows(&Paged { path: &path, query: &query, missing, pages: COMMENT_PAGES, cap: usize::MAX }, wanted)?;
        let mut comments: Vec<Comment> = stories
            .iter()
            .map(|c| Comment {
                author: text(c, "/created_by/name"),
                created_at: text(c, "/created_at"),
                body: body(c, "/html_text", "/text"),
            })
            .filter(|c| !c.body.trim().is_empty())
            .collect();
        comments.sort_by_key(|c| parse_time(&c.created_at).unwrap_or(0));
        Ok(comments)
    }
}

fn workspace_names(workspaces: &[(String, String)]) -> String {
    workspaces.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", ")
}

fn choose(setting: &str, workspaces: &[(String, String)]) -> Result<(String, String)> {
    let setting = setting.trim();
    if !setting.is_empty() {
        let named = |(_, name): &&(String, String)| name.eq_ignore_ascii_case(setting);
        let by_name: Vec<_> = workspaces.iter().filter(named).collect();
        let found = workspaces.iter().find(|(gid, _)| gid == setting).or(match by_name.as_slice() {
            [only] => Some(only),
            _ => None,
        });
        if found.is_none() && by_name.len() > 1 {
            let gids: Vec<&str> = by_name.iter().map(|(gid, _)| gid.as_str()).collect();
            return Err(Error::Api(format!(
                "several of your Asana workspaces are named {setting}: use the ID of one, {}",
                gids.join(" or ")
            )));
        }
        return found.cloned().ok_or_else(|| {
            Error::Api(format!("no Asana workspace named {setting}; yours are {}", workspace_names(workspaces)))
        });
    }
    match workspaces {
        [only] => Ok(only.clone()),
        [] => Err(Error::Api("this Asana account belongs to no workspace".into())),
        _ => Err(Error::Api(format!(
            "this Asana account has several workspaces ({}): set the one to use in Settings → Issues → Asana",
            workspace_names(workspaces)
        ))),
    }
}

pub(crate) fn pick_workspace(input: &str, workspaces: &[(String, String)]) -> std::result::Result<String, String> {
    if input.trim().is_empty() {
        return Err(format!("type one of your workspaces: {}", workspace_names(workspaces)));
    }
    choose(input, workspaces).map(|(gid, _)| gid).map_err(|e| e.to_string())
}

#[expect(clippy::unnecessary_wraps, reason = "the settings take every row's check as one function type")]
pub(crate) fn check_workspace(input: &str) -> std::result::Result<String, &'static str> {
    Ok(input.trim().to_string())
}

pub(crate) fn check_projects(input: &str) -> std::result::Result<String, &'static str> {
    let tokens: Vec<&str> = input.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let gids = project_gids(input);
    if gids.len() != tokens.len() {
        return Err(
            "type project IDs or links, such as 1200000000000001 or https://app.asana.com/0/1200000000000001/list",
        );
    }
    Ok(gids.join(", "))
}

fn display(user: &Value) -> String {
    ["/name", "/email"].iter().map(|p| text(user, p)).find(|n| !n.is_empty()).unwrap_or_default()
}

fn names(users: &[Value]) -> HashMap<String, String> {
    users
        .iter()
        .map(|u| (text(u, "/gid"), display(u)))
        .filter(|(gid, name)| !gid.is_empty() && !name.is_empty())
        .collect()
}

fn person(who: &Who, me: &str) -> Option<String> {
    match who {
        Who::Anyone => None,
        Who::Me => Some(me.to_string()),
        Who::User { id, .. } | Who::Person(id) => Some(id.clone()),
    }
}

fn custom_id(task: &Value) -> String {
    task["custom_fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| text(f, "/representation_type") == "custom_id" || !f["id_prefix"].is_null())
        .map(|f| text(f, "/display_value"))
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn issue(task: &Value, names: &HashMap<String, String>) -> Option<Issue> {
    let gid = text(task, "/gid");
    if gid.is_empty() {
        return None;
    }
    let completed = task["completed"].as_bool() == Some(true);
    let section =
        task["memberships"].as_array().into_iter().flatten().map(|m| text(m, "/section/name")).find(|s| !s.is_empty());
    let state = if completed { "completed".to_string() } else { section.unwrap_or_else(|| "open".into()) };
    let key = Some(custom_id(task)).filter(|id| !id.is_empty()).unwrap_or_else(|| gid.clone());
    let updated_at = ["/modified_at", "/created_at"].iter().map(|p| text(task, p)).find(|t| !t.is_empty());
    Some(Issue {
        source: Source::Asana,
        number: gid.parse().unwrap_or_default(),
        key,
        title: text(task, "/name"),
        url: text(task, "/permalink_url"),
        state,
        closed: completed,
        labels: task["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| text(t, "/name"))
            .filter(|n| !n.is_empty())
            .collect(),
        assignees: Some(text(task, "/assignee/name")).filter(|n| !n.is_empty()).into_iter().collect(),
        author: names.get(&text(task, "/created_by/gid")).cloned().unwrap_or_default(),
        updated_at: updated_at.unwrap_or_default(),
    })
}

fn rich(html: &str) -> String {
    let html = html.trim();
    let html = html.strip_prefix("<body>").unwrap_or(html);
    let mut html = html.strip_suffix("</body>").unwrap_or(html).to_string();
    while let Some(start) = html.find("<img") {
        let end = html[start..].find('>').map_or(html.len(), |i| start + i + 1);
        html.replace_range(start..end, IMAGE);
    }
    html
}

fn body(node: &Value, html_field: &str, plain: &str) -> String {
    let markdown = html::markdown(&rich(&text(node, html_field))).replace(IMAGE, "[image]");
    if markdown.is_empty() { text(node, plain).trim().to_string() } else { markdown }
}

fn detail(node: &Value, names: &HashMap<String, String>, comments: Vec<Comment>) -> Detail {
    let due = text(node, "/due_on");
    let author = names.get(&text(node, "/created_by/gid")).cloned().unwrap_or_default();
    let found = issue(node, names);
    let projects: Vec<String> = node["memberships"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| text(m, "/project/name"))
        .filter(|n| !n.is_empty())
        .collect();
    let mut info = vec![found.as_ref().map(|i| i.state.clone()).unwrap_or_default()];
    info.extend((!due.is_empty()).then(|| format!("due {due}")));
    info.extend((!author.is_empty()).then(|| format!("created by @{author}")));
    info.extend(found.and_then(|i| i.assignees.first().map(|a| format!("assigned to @{a}"))));
    info.push(projects.join(", "));
    info.retain(|part| !part.is_empty());
    Detail { info, body: join_body(&[&body(node, "/html_notes", "/notes")]), comments }
}

fn failure(answer: &Answer) -> Error {
    if answer.status == 402 {
        return Error::Api("this needs a paid Asana plan".into());
    }
    let message = text(&answer.json, "/errors/0/message");
    if message.trim().is_empty() {
        Error::Api(format!("Asana answered {}", answer.status))
    } else {
        Error::Api(format!("Asana answered: {}", message.trim()))
    }
}

pub(crate) fn project_gids(input: &str) -> Vec<String> {
    input.split(|c: char| c.is_whitespace() || c == ',').filter_map(project_gid).collect()
}

fn project_gid(token: &str) -> Option<String> {
    let link = token.contains("://");
    let path = token.split_once("://").map_or(token, |(_, rest)| rest);
    let parts: Vec<&str> = path.split(['/', '?', '#']).filter(|part| !part.is_empty()).collect();
    let after = ["project", "0"].iter().find_map(|marker| parts.iter().position(|part| part == marker)).map(|i| i + 1);
    let part = parts.get(after.or_else(|| (!link).then_some(0))?)?;
    part.bytes().all(|b| b.is_ascii_digit()).then(|| (*part).to_string())
}

#[cfg(test)]
mod tests;
