use std::collections::HashMap;

use serde_json::Value;

use super::{Account, Comment, Detail, Issue, LIMIT, Listed, Person, Query, Source, Who, checklist, join_body};
use crate::error::{Error, Result};
use crate::http::{self, Service, text};

pub(crate) const TOKEN_ENV: &str = "SHORTCUT_API_TOKEN";
pub(crate) const API_ENV: &str = "CORNERCASE_SHORTCUT_API";
pub(crate) const DEFAULT_API: &str = "https://api.app.shortcut.com/api/v3";
const SERVICE: Service = Service { name: "Shortcut", rejected: "Shortcut rejected the token" };
const OPEN_QUERY: &str = "!is:done !is:archived";
const ANY_QUERY: &str = "!is:archived";
const PAGE_SIZE: &str = "25";

pub(crate) struct Api<'a> {
    pub(crate) base: &'a str,
    pub(crate) token: &'a str,
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
        let answer = http::get_without_redirects(&SERVICE, url, query, &[("Shortcut-Token", self.token)])?;
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

    pub(crate) fn whoami(&self) -> Result<Account> {
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

    pub(crate) fn people(&self) -> Result<Vec<Person>> {
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

    pub(crate) fn list(&self, query: &Query) -> Result<Listed> {
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

    pub(crate) fn find(&self, id: u64) -> Result<Issue> {
        let story = self.story(id)?;
        Ok(issue(&story, &self.lookups()?))
    }

    fn story(&self, id: u64) -> Result<Value> {
        self.request(&self.path(&format!("/stories/{id}")), &[], &format!("story sc-{id}"))
    }

    pub(crate) fn view(&self, id: u64) -> Result<Detail> {
        let lookups = self.lookups()?;
        let story = self.story(id)?;
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
mod tests;
