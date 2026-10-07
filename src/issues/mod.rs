pub mod browser;
pub mod cache;
pub mod github;
pub mod http;
pub mod jira;
pub mod linear;
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
}

impl Source {
    pub const ALL: [Self; 4] = [Self::Github, Self::Shortcut, Self::Linear, Self::Jira];
    pub const REMOTE: [Self; 3] = [Self::Shortcut, Self::Linear, Self::Jira];

    pub fn id(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Shortcut => "shortcut",
            Self::Linear => "linear",
            Self::Jira => "jira",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::Shortcut => "Shortcut",
            Self::Linear => "Linear",
            Self::Jira => "Jira",
        }
    }

    pub fn token_name(self) -> &'static str {
        match self {
            Self::Github | Self::Shortcut | Self::Jira => "API token",
            Self::Linear => "API key",
        }
    }

    pub fn token_env(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Shortcut => Some(shortcut::TOKEN_ENV),
            Self::Linear => Some(linear::TOKEN_ENV),
            Self::Jira => Some(jira::TOKEN_ENV),
        }
    }

    pub fn secret_key(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Shortcut => Some("shortcut_token"),
            Self::Linear => Some("linear_api_key"),
            Self::Jira => Some("jira_api_token"),
        }
    }

    pub fn people_fields(self) -> [&'static str; 2] {
        match self {
            Self::Github => ["assignee", "author"],
            Self::Shortcut => ["owner", "requester"],
            Self::Linear => ["assignee", "creator"],
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
}

impl People {
    pub fn of(&self, source: Source) -> &[Who; 2] {
        match source {
            Source::Github => &self.github,
            Source::Shortcut => &self.shortcut,
            Source::Linear => &self.linear,
            Source::Jira => &self.jira,
        }
    }

    pub fn of_mut(&mut self, source: Source) -> &mut [Who; 2] {
        match source {
            Source::Github => &mut self.github,
            Source::Shortcut => &mut self.shortcut,
            Source::Linear => &mut self.linear,
            Source::Jira => &mut self.jira,
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
}

impl Client {
    pub fn people(&self) -> Result<Vec<Person>> {
        match self {
            Self::Github { gh, dir } => github::people(gh, dir),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.people(),
            Self::Linear { url, token } => linear::Api { url, token }.people(),
            Self::Jira(api) => api.people(),
        }
    }

    pub fn list(&self, query: &Query) -> Result<Listed> {
        match self {
            Self::Github { gh, dir } => Ok(Listed { account: None, issues: github::list(gh, dir, query)? }),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.list(query),
            Self::Linear { url, token } => linear::Api { url, token }.list(query),
            Self::Jira(api) => api.list(query),
        }
    }

    pub fn read(&self, issue: &Issue) -> Result<Detail> {
        match self {
            Self::Github { gh, dir } => github::view(gh, dir, issue.number),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.view(issue.number),
            Self::Linear { url, token } => linear::Api { url, token }.view(&issue.key),
            Self::Jira(api) => api.view(&issue.key),
        }
    }

    pub fn whoami(&self) -> Result<Account> {
        match self {
            Self::Github { .. } => Err(Error::Api("GitHub goes through gh and its own login".into())),
            Self::Shortcut { base, token } => shortcut::Api { base, token }.whoami(),
            Self::Linear { url, token } => linear::Api { url, token }.whoami(),
            Self::Jira(api) => api.whoami(),
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
        Source::Linear | Source::Jira => issue.key.clone(),
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
mod tests {
    use rstest::rstest;

    use super::*;

    fn gh(number: u64, title: &str) -> Issue {
        issue(Source::Github, number, title)
    }

    mod filter {
        use super::*;

        fn found(query: &str) -> Vec<u64> {
            let issues = [
                Issue { labels: vec!["bug".into()], ..gh(482, "Returns page crashes") },
                Issue { author: "luis".into(), state: "In Review".into(), ..gh(48, "Dark mode") },
            ];
            filter(&issues, query).iter().map(|i| i.number).collect()
        }

        #[rstest]
        #[case::empty("", vec![482, 48])]
        #[case::title("CRASH", vec![482])]
        #[case::key("#48", vec![482, 48])]
        #[case::whole_key("#482", vec![482])]
        #[case::label("bug", vec![482])]
        #[case::author("luis", vec![48])]
        #[case::state("review", vec![48])]
        #[case::nothing("zzz", vec![])]
        fn keeps_the_matching_issues_in_order(#[case] query: &str, #[case] expected: Vec<u64>) {
            assert_eq!(found(query), expected);
        }
    }

    #[test]
    fn sorting_puts_the_last_updated_first() {
        let old = Issue { updated_at: "2026-01-01T00:00:00Z".into(), ..gh(1, "old") };
        let new = Issue { updated_at: "2026-09-01T00:00:00.123Z".into(), ..issue(Source::Linear, 2, "new") };
        let mut list = vec![&old, &new];
        sort_by_updated(&mut list);
        assert_eq!(list.iter().map(|i| i.number).collect::<Vec<_>>(), [2, 1]);
    }

    mod branch {
        use super::*;

        #[rstest]
        #[case::plain("Returns page crashes", "issue-482-returns-page-crashes")]
        #[case::symbols("Fix: `cd` breaks (again)!", "issue-482-fix-cd-breaks-again")]
        #[case::accents("Añadir búsqueda rápida", "issue-482-anadir-busqueda-rapida")]
        #[case::nothing_left("???", "issue-482")]
        #[case::cut_at_a_word(
            "Returns page crashes when the shipping address is empty on mobile",
            "issue-482-returns-page-crashes-when-the-shipping"
        )]
        fn is_named_after_a_github_issue(#[case] title: &str, #[case] expected: &str) {
            assert_eq!(branch(&gh(482, title)), expected);
        }

        #[rstest]
        #[case::story(Source::Shortcut, "sc-12-dark-mode")]
        #[case::linear(Source::Linear, "ENG-12-dark-mode")]
        #[case::jira(Source::Jira, "PROJ-12-dark-mode")]
        fn starts_with_the_key_of_the_tracker(#[case] source: Source, #[case] expected: &str) {
            assert_eq!(branch(&issue(source, 12, "Dark mode")), expected);
        }
    }

    #[test]
    fn the_workspace_is_named_after_the_issue() {
        let name = workspace_name(&gh(482, "Returns page crashes when the shipping address is empty"));
        assert_eq!(name, "#482 Returns page crashes when the ship…");
    }

    mod prompt {
        use super::*;

        #[rstest]
        #[case::url("{url}", "https://github.com/acme/shop/issues/7")]
        #[case::text_as_typed("Fix {key}: {title}", "Fix #7: it's broken")]
        #[case::number_and_branch("{number} on {branch}", "7 on issue-7")]
        #[case::ref_and_source("{ref} from {source}", "#7 from github")]
        #[case::unknown_placeholders_stay("x {nope} {url", "x {nope} {url")]
        #[case::newlines_are_kept("a\nb", "a\nb")]
        fn fills_in_the_issue(#[case] template: &str, #[case] expected: &str) {
            assert_eq!(prompt(template, &gh(7, "it's broken"), "issue-7"), expected);
        }

        #[test]
        fn one_line_joins_lines_and_spaces() {
            assert_eq!(one_line(" a\n  b\tc "), "a b c");
        }
    }

    mod meta {
        use super::*;

        const UPDATED: &str = "2026-09-19T12:00:00Z";

        fn updated() -> Issue {
            Issue { updated_at: UPDATED.into(), labels: vec!["bug".into(), "p1".into()], ..gh(1, "x") }
        }

        #[rstest]
        #[case::minutes(5 * 60, "5m")]
        #[case::hours(3 * 3_600, "3h")]
        #[case::days(4 * 86_400, "4d")]
        #[case::months(65 * 86_400, "2mo")]
        #[case::years(800 * 86_400, "2y")]
        fn shows_labels_author_and_age(#[case] ago: i64, #[case] age: &str) {
            let now = parse_time(UPDATED).expect("timestamp") + ago;
            assert_eq!(meta(&updated(), now), format!("bug, p1 · ana · {age}"));
        }

        #[test]
        fn shows_the_state_of_stories_and_closed_issues() {
            let story = Issue { state: "In Review".into(), ..issue(Source::Shortcut, 1, "x") };
            let closed = Issue { state: "closed".into(), closed: true, ..gh(2, "y") };
            assert_eq!((meta(&story, 0), meta(&closed, 0)), ("In Review · ana".into(), "closed · ana".into()));
        }

        #[test]
        fn leaves_out_what_is_missing() {
            assert_eq!(meta(&Issue { author: String::new(), ..gh(1, "x") }, 0), "");
        }

        #[rstest]
        #[case::utc("1970-01-02T00:00:10Z", 86_410)]
        #[case::ahead("1970-01-02T02:00:10.000+0200", 86_410)]
        #[case::behind_with_a_colon("1970-01-01T19:30:10-04:30", 86_410)]
        #[case::no_zone("1970-01-02T00:00:10", 86_410)]
        fn parses_the_time_zone_offset(#[case] text: &str, #[case] expected: i64) {
            assert_eq!(parse_time(text), Some(expected));
        }

        #[test]
        fn parses_timestamps_with_or_without_milliseconds() {
            assert_eq!(
                (parse_time("1970-01-02T00:00:10Z"), parse_time("1970-01-02T00:00:10.250Z")),
                (Some(86_410), Some(86_410))
            );
        }
    }

    #[test]
    fn a_checklist_is_a_bold_title_and_task_items() {
        let items = [(true, "Reproduce".to_string()), (false, "Fix\nit".to_string())];
        assert_eq!(checklist("Tasks", &items), "**Tasks**\n\n- [x] Reproduce\n- [ ] Fix it");
    }

    #[test]
    fn people_describe_the_fields_that_filter() {
        let people = People { shortcut: [Who::Me, Who::Person("bo".into())], ..People::default() };
        assert_eq!(
            (people.describe(Source::Shortcut), people.describe(Source::Linear)),
            ("owner me · requester @bo".into(), String::new())
        );
    }

    #[test]
    fn an_account_describes_who_and_where() {
        let account = Account { handle: "ana".into(), workspace: "acme".into() };
        assert_eq!(account.describe(), "@ana in acme");
    }
}
