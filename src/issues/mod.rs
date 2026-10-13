pub(crate) mod browser;
pub(crate) mod cache;
pub(crate) mod github;
pub(crate) mod jira;
pub(crate) mod linear;
pub(crate) mod md;
pub(crate) mod plane;
pub(crate) mod secrets;
pub(crate) mod shortcut;

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::time::{age, parse_time};

pub(crate) const LIMIT: usize = 100;
const SLUG_MAX: usize = 40;
const NAME_MAX: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Source {
    Github,
    Shortcut,
    Linear,
    Jira,
    Plane,
}

impl Source {
    pub(crate) const ALL: [Self; 5] = [Self::Github, Self::Shortcut, Self::Linear, Self::Jira, Self::Plane];
    pub(crate) const REMOTE: [Self; 4] = [Self::Shortcut, Self::Linear, Self::Jira, Self::Plane];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Shortcut => "shortcut",
            Self::Linear => "linear",
            Self::Jira => "jira",
            Self::Plane => "plane",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::Shortcut => "Shortcut",
            Self::Linear => "Linear",
            Self::Jira => "Jira",
            Self::Plane => "Plane",
        }
    }

    pub(crate) fn token_name(self) -> &'static str {
        match self {
            Self::Github | Self::Shortcut | Self::Jira => "API token",
            Self::Linear | Self::Plane => "API key",
        }
    }

    pub(crate) fn token_env(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Shortcut => Some(shortcut::TOKEN_ENV),
            Self::Linear => Some(linear::TOKEN_ENV),
            Self::Jira => Some(jira::TOKEN_ENV),
            Self::Plane => Some(plane::TOKEN_ENV),
        }
    }

    pub(crate) fn secret_key(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Shortcut => Some("shortcut_token"),
            Self::Linear => Some("linear_api_key"),
            Self::Jira => Some("jira_api_token"),
            Self::Plane => Some("plane_api_key"),
        }
    }

    pub(crate) fn people_fields(self) -> [&'static str; 2] {
        match self {
            Self::Github => ["assignee", "author"],
            Self::Shortcut => ["owner", "requester"],
            Self::Linear | Self::Plane => ["assignee", "creator"],
            Self::Jira => ["assignee", "reporter"],
        }
    }

    pub(crate) fn token_help(self) -> &'static str {
        match self {
            Self::Github => "",
            Self::Shortcut => {
                "Create a token in Shortcut under Settings → Your account → API Tokens, paste it here and press Enter."
            }
            Self::Linear => {
                "Create a personal API key in Linear under Settings → Security & access → Personal API keys, paste it here and press Enter."
            }
            Self::Jira => {
                "Create an API token at id.atlassian.com under Security → Create and manage API tokens, paste it here and press Enter."
            }
            Self::Plane => {
                "Create an API key in Plane under Profile settings → Personal access tokens, paste it here and press Enter."
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Who {
    #[default]
    Anyone,
    Me,
    Person(String),
    User {
        id: String,
        name: String,
    },
}

impl Who {
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Anyone => "anyone".into(),
            Self::Me => "me".into(),
            Self::Person(handle) | Self::User { name: handle, .. } => format!("@{handle}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub(crate) struct Query {
    pub(crate) closed: bool,
    #[serde(default)]
    pub(crate) people: [Who; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct People {
    pub(crate) github: [Who; 2],
    pub(crate) shortcut: [Who; 2],
    pub(crate) linear: [Who; 2],
    pub(crate) jira: [Who; 2],
    pub(crate) plane: [Who; 2],
}

impl People {
    pub(crate) fn of(&self, source: Source) -> &[Who; 2] {
        match source {
            Source::Github => &self.github,
            Source::Shortcut => &self.shortcut,
            Source::Linear => &self.linear,
            Source::Jira => &self.jira,
            Source::Plane => &self.plane,
        }
    }

    pub(crate) fn of_mut(&mut self, source: Source) -> &mut [Who; 2] {
        match source {
            Source::Github => &mut self.github,
            Source::Shortcut => &mut self.shortcut,
            Source::Linear => &mut self.linear,
            Source::Jira => &mut self.jira,
            Source::Plane => &mut self.plane,
        }
    }

    pub(crate) fn describe(&self, source: Source) -> String {
        source
            .people_fields()
            .iter()
            .zip(self.of(source))
            .filter(|(_, who)| **who != Who::Anyone)
            .map(|(field, who)| format!("{field} {}", who.describe()))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Person {
    pub(crate) handle: String,
    pub(crate) name: String,
    pub(crate) id: Option<String>,
}

impl Person {
    pub(crate) fn who(&self) -> Who {
        match &self.id {
            Some(id) => Who::User { id: id.clone(), name: self.handle.clone() },
            None => Who::Person(self.handle.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Issue {
    pub(crate) source: Source,
    pub(crate) number: u64,
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) closed: bool,
    pub(crate) labels: Vec<String>,
    pub(crate) assignees: Vec<String>,
    pub(crate) author: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comment {
    pub(crate) author: String,
    pub(crate) created_at: String,
    pub(crate) body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Detail {
    pub(crate) info: Vec<String>,
    pub(crate) body: String,
    pub(crate) comments: Vec<Comment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Account {
    pub(crate) handle: String,
    pub(crate) workspace: String,
}

impl Account {
    pub(crate) fn describe(&self) -> String {
        match (self.handle.is_empty(), self.workspace.is_empty()) {
            (false, false) => format!("@{} in {}", self.handle, self.workspace),
            (false, true) => format!("@{}", self.handle),
            _ => self.workspace.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) account: Option<Account>,
    pub(crate) issues: Vec<Issue>,
}

pub(crate) enum Client {
    Github { gh: PathBuf, dir: PathBuf },
    Shortcut { base: String, token: String },
    Linear { url: String, token: String },
    Jira(jira::Api),
    Plane(plane::Api),
}

impl Client {
    pub(crate) fn people(&self) -> Result<Vec<Person>> {
        match self {
            Self::Github { gh, dir } => github::people(gh, dir),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.people(),
            Self::Linear { url, token } => linear::Api { url, token }.people(),
            Self::Jira(api) => api.people(),
            Self::Plane(api) => api.people(),
        }
    }

    pub(crate) fn list(&self, query: &Query) -> Result<Listed> {
        match self {
            Self::Github { gh, dir } => Ok(Listed { account: None, issues: github::list(gh, dir, query)? }),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.list(query),
            Self::Linear { url, token } => linear::Api { url, token }.list(query),
            Self::Jira(api) => api.list(query),
            Self::Plane(api) => api.list(query),
        }
    }

    pub(crate) fn read(&self, issue: &Issue) -> Result<Detail> {
        match self {
            Self::Github { gh, dir } => github::view(gh, dir, issue.number),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.view(issue.number),
            Self::Linear { url, token } => linear::Api { url, token }.view(&issue.key),
            Self::Jira(api) => api.view(&issue.key),
            Self::Plane(api) => api.view(&issue.key),
        }
    }

    pub(crate) fn whoami(&self) -> Result<Account> {
        match self {
            Self::Github { .. } => Err(Error::Api("GitHub goes through gh and its own login".into())),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.whoami(),
            Self::Linear { url, token } => linear::Api { url, token }.whoami(),
            Self::Jira(api) => api.whoami(),
            Self::Plane(api) => api.whoami(),
        }
    }
}

pub(crate) fn filter<'a>(issues: impl IntoIterator<Item = &'a Issue>, query: &str) -> Vec<&'a Issue> {
    let needle = query.trim().to_lowercase();
    issues
        .into_iter()
        .filter(|issue| {
            [issue.key.as_str(), &issue.title, &issue.author, &issue.state]
                .into_iter()
                .chain(issue.labels.iter().map(String::as_str))
                .chain(issue.assignees.iter().map(String::as_str))
                .any(|key| key.to_lowercase().contains(&needle))
        })
        .collect()
}

pub(crate) fn sort_by_updated(issues: &mut [&Issue]) {
    issues.sort_by_key(|issue| std::cmp::Reverse(parse_time(&issue.updated_at).unwrap_or(0)));
}

pub(crate) fn branch(issue: &Issue) -> String {
    let prefix = match issue.source {
        Source::Github => format!("issue-{}", issue.number),
        Source::Shortcut => format!("sc-{}", issue.number),
        Source::Linear | Source::Jira | Source::Plane => issue.key.clone(),
    };
    let slug = slug(&issue.title, SLUG_MAX);
    if slug.is_empty() { prefix } else { format!("{prefix}-{slug}") }
}

pub(crate) fn workspace_name(issue: &Issue) -> String {
    crate::text::truncate_right(&format!("{} {}", issue.key, issue.title), NAME_MAX)
}

fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        _ => c,
    }
}

fn slug(text: &str, max: usize) -> String {
    let mut slug = String::new();
    for c in text.chars().flat_map(char::to_lowercase).map(fold) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.len() <= max {
        return slug.to_string();
    }
    let cut = &slug[..=max];
    let end = cut.rfind('-').filter(|&i| i > 0).unwrap_or(max);
    slug[..end].trim_end_matches('-').to_string()
}

pub(crate) fn prompt(template: &str, issue: &Issue, branch: &str) -> String {
    let number = issue.number.to_string();
    let labels = issue.labels.join(", ");
    let vars = [
        ("url", issue.url.as_str()),
        ("number", &number),
        ("key", &issue.key),
        ("ref", &issue.key),
        ("title", &issue.title),
        ("branch", branch),
        ("source", issue.source.id()),
        ("author", &issue.author),
        ("labels", &labels),
    ];
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let var = after.find('}').and_then(|end| vars.iter().find(|(k, _)| *k == &after[..end]).map(|(_, v)| (end, v)));
        if let Some((end, value)) = var {
            out.push_str(value);
            rest = &after[end + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn meta(issue: &Issue, now: i64) -> String {
    let state = if issue.source == Source::Github && !issue.closed { "" } else { issue.state.as_str() };
    let labels = issue.labels.join(", ");
    [state, labels.as_str(), issue.author.as_str(), age_of(&issue.updated_at, now).as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(crate) fn age_of(timestamp: &str, now: i64) -> String {
    parse_time(timestamp).map(|t| age(now - t)).unwrap_or_default()
}

pub(crate) fn checklist(title: &str, items: &[(bool, String)]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = items
        .iter()
        .map(|(done, text)| {
            format!("- [{}] {}", if *done { 'x' } else { ' ' }, text.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .collect();
    format!("**{title}**\n\n{}", lines.join("\n"))
}

pub(crate) fn join_body(parts: &[&str]) -> String {
    parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
pub(crate) fn issue(source: Source, number: u64, title: &str) -> Issue {
    let key = match source {
        Source::Github => format!("#{number}"),
        Source::Shortcut => format!("sc-{number}"),
        Source::Linear => format!("ENG-{number}"),
        Source::Jira => format!("PROJ-{number}"),
        Source::Plane => format!("WEB-{number}"),
    };
    Issue {
        source,
        number,
        key,
        title: title.into(),
        url: format!("https://github.com/acme/shop/issues/{number}"),
        state: "open".into(),
        closed: false,
        labels: Vec::new(),
        assignees: Vec::new(),
        author: "ana".into(),
        updated_at: String::new(),
    }
}

#[cfg(test)]
mod tests;
