pub mod browser;
pub mod cache;
pub mod github;
pub mod http;
pub mod jira;
pub mod linear;
pub mod md;
pub mod plane;
pub mod shortcut;

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const LIMIT: usize = 100;
const SLUG_MAX: usize = 40;
const NAME_MAX: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Github,
    Shortcut,
    Linear,
    Jira,
    Plane,
}

impl Source {
    pub const ALL: [Self; 5] = [Self::Github, Self::Shortcut, Self::Linear, Self::Jira, Self::Plane];
    pub const REMOTE: [Self; 4] = [Self::Shortcut, Self::Linear, Self::Jira, Self::Plane];

    pub fn id(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Shortcut => "shortcut",
            Self::Linear => "linear",
            Self::Jira => "jira",
            Self::Plane => "plane",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::Shortcut => "Shortcut",
            Self::Linear => "Linear",
            Self::Jira => "Jira",
            Self::Plane => "Plane",
        }
    }

    pub fn token_name(self) -> &'static str {
        match self {
            Self::Github | Self::Shortcut | Self::Jira => "API token",
            Self::Linear | Self::Plane => "API key",
        }
    }

    pub fn token_env(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Shortcut => Some(shortcut::TOKEN_ENV),
            Self::Linear => Some(linear::TOKEN_ENV),
            Self::Jira => Some(jira::TOKEN_ENV),
            Self::Plane => Some(plane::TOKEN_ENV),
        }
    }

    pub fn secret_key(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Shortcut => Some("shortcut_token"),
            Self::Linear => Some("linear_api_key"),
            Self::Jira => Some("jira_api_token"),
            Self::Plane => Some("plane_api_key"),
        }
    }

    pub fn people_fields(self) -> [&'static str; 2] {
        match self {
            Self::Github => ["assignee", "author"],
            Self::Shortcut => ["owner", "requester"],
            Self::Linear | Self::Plane => ["assignee", "creator"],
            Self::Jira => ["assignee", "reporter"],
        }
    }

    pub fn token_help(self) -> &'static str {
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
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Who {
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
    pub fn describe(&self) -> String {
        match self {
            Self::Anyone => "anyone".into(),
            Self::Me => "me".into(),
            Self::Person(handle) | Self::User { name: handle, .. } => format!("@{handle}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Query {
    pub closed: bool,
    #[serde(default)]
    pub people: [Who; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct People {
    pub github: [Who; 2],
    pub shortcut: [Who; 2],
    pub linear: [Who; 2],
    pub jira: [Who; 2],
    pub plane: [Who; 2],
}

impl People {
    pub fn of(&self, source: Source) -> &[Who; 2] {
        match source {
            Source::Github => &self.github,
            Source::Shortcut => &self.shortcut,
            Source::Linear => &self.linear,
            Source::Jira => &self.jira,
            Source::Plane => &self.plane,
        }
    }

    pub fn of_mut(&mut self, source: Source) -> &mut [Who; 2] {
        match source {
            Source::Github => &mut self.github,
            Source::Shortcut => &mut self.shortcut,
            Source::Linear => &mut self.linear,
            Source::Jira => &mut self.jira,
            Source::Plane => &mut self.plane,
        }
    }

    pub fn describe(&self, source: Source) -> String {
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
pub struct Person {
    pub handle: String,
    pub name: String,
    pub id: Option<String>,
}

impl Person {
    pub fn who(&self) -> Who {
        match &self.id {
            Some(id) => Who::User { id: id.clone(), name: self.handle.clone() },
            None => Who::Person(self.handle.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub source: Source,
    pub number: u64,
    pub key: String,
    pub title: String,
    pub url: String,
    pub state: String,
    pub closed: bool,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub author: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub author: String,
    pub created_at: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Detail {
    pub info: Vec<String>,
    pub body: String,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub handle: String,
    pub workspace: String,
}

impl Account {
    pub fn describe(&self) -> String {
        match (self.handle.is_empty(), self.workspace.is_empty()) {
            (false, false) => format!("@{} in {}", self.handle, self.workspace),
            (false, true) => format!("@{}", self.handle),
            _ => self.workspace.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub account: Option<Account>,
    pub issues: Vec<Issue>,
}

pub enum Client {
    Github { gh: PathBuf, dir: PathBuf },
    Shortcut { base: String, token: String },
    Linear { url: String, token: String },
    Jira(jira::Api),
    Plane(plane::Api),
}

impl Client {
    pub fn people(&self) -> Result<Vec<Person>> {
        match self {
            Self::Github { gh, dir } => github::people(gh, dir),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.people(),
            Self::Linear { url, token } => linear::Api { url, token }.people(),
            Self::Jira(api) => api.people(),
            Self::Plane(api) => api.people(),
        }
    }

    pub fn list(&self, query: &Query) -> Result<Listed> {
        match self {
            Self::Github { gh, dir } => Ok(Listed { account: None, issues: github::list(gh, dir, query)? }),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.list(query),
            Self::Linear { url, token } => linear::Api { url, token }.list(query),
            Self::Jira(api) => api.list(query),
            Self::Plane(api) => api.list(query),
        }
    }

    pub fn read(&self, issue: &Issue) -> Result<Detail> {
        match self {
            Self::Github { gh, dir } => github::view(gh, dir, issue.number),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.view(issue.number),
            Self::Linear { url, token } => linear::Api { url, token }.view(&issue.key),
            Self::Jira(api) => api.view(&issue.key),
            Self::Plane(api) => api.view(&issue.key),
        }
    }

    pub fn whoami(&self) -> Result<Account> {
        match self {
            Self::Github { .. } => Err(Error::Api("GitHub goes through gh and its own login".into())),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.whoami(),
            Self::Linear { url, token } => linear::Api { url, token }.whoami(),
            Self::Jira(api) => api.whoami(),
            Self::Plane(api) => api.whoami(),
        }
    }
}

pub fn filter<'a>(issues: impl IntoIterator<Item = &'a Issue>, query: &str) -> Vec<&'a Issue> {
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

pub fn sort_by_updated(issues: &mut [&Issue]) {
    issues.sort_by_key(|issue| std::cmp::Reverse(parse_time(&issue.updated_at).unwrap_or(0)));
}

pub fn branch(issue: &Issue) -> String {
    let prefix = match issue.source {
        Source::Github => format!("issue-{}", issue.number),
        Source::Shortcut => format!("sc-{}", issue.number),
        Source::Linear | Source::Jira | Source::Plane => issue.key.clone(),
    };
    let slug = slug(&issue.title, SLUG_MAX);
    if slug.is_empty() { prefix } else { format!("{prefix}-{slug}") }
}

pub fn workspace_name(issue: &Issue) -> String {
    crate::ui::truncate_right(&format!("{} {}", issue.key, issue.title), NAME_MAX)
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

pub fn prompt(template: &str, issue: &Issue, branch: &str) -> String {
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

pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn meta(issue: &Issue, now: i64) -> String {
    let state = if issue.source == Source::Github && !issue.closed { "" } else { issue.state.as_str() };
    let labels = issue.labels.join(", ");
    [state, labels.as_str(), issue.author.as_str(), age_of(&issue.updated_at, now).as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn age_of(timestamp: &str, now: i64) -> String {
    parse_time(timestamp).map(|t| age(now - t)).unwrap_or_default()
}

pub fn age(secs: i64) -> String {
    let minutes = secs.max(0) / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 60 {
        format!("{minutes}m")
    } else if hours < 24 {
        format!("{hours}h")
    } else if days < 30 {
        format!("{days}d")
    } else if days < 365 {
        format!("{}mo", days / 30)
    } else {
        format!("{}y", days / 365)
    }
}

pub fn parse_time(text: &str) -> Option<i64> {
    let field = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    let local = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(local - offset(text.get(19..).unwrap_or_default()))
}

fn offset(rest: &str) -> i64 {
    let zone = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let sign = match zone.chars().next() {
        Some('+') => 1,
        Some('-') => -1,
        _ => return 0,
    };
    let digits: String = zone.chars().filter(char::is_ascii_digit).collect();
    let part = |range: std::ops::Range<usize>| digits.get(range).and_then(|d| d.parse::<i64>().ok()).unwrap_or(0);
    sign * (part(0..2) * 3_600 + part(2..4) * 60)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn checklist(title: &str, items: &[(bool, String)]) -> String {
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

pub fn join_body(parts: &[&str]) -> String {
    parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
pub fn issue(source: Source, number: u64, title: &str) -> Issue {
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
