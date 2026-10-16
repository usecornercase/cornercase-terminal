use crate::error::{Error, Result};
use crate::issues::{Client, Issue, Source};

const MARKERS: [&str; 5] = ["issues", "work_items", "story", "issue", "browse"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Locator {
    pub(crate) source: Source,
    pub(crate) key: String,
    pub(crate) url: Option<String>,
}

pub(crate) fn locator(text: &str, source: Option<&str>, available: &[Source]) -> Result<Locator> {
    let text = text.trim();
    let selected = source
        .map(|source| {
            Source::ALL
                .into_iter()
                .find(|kind| kind.id() == source)
                .ok_or_else(|| Error::Api(format!("unknown issue source {source}")))
        })
        .transpose()?;
    if let Some(url) = text.strip_prefix("https://").or_else(|| text.strip_prefix("http://")) {
        let (host, path) = url.split_once('/').ok_or_else(|| Error::Api("the issue URL has no path".into()))?;
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        let inferred = if host == "github.com" {
            Some(Source::Github)
        } else if parts.windows(2).any(|parts| parts == ["-", "issues"] || parts == ["-", "work_items"]) {
            Some(Source::Gitlab)
        } else if host == "app.shortcut.com" {
            Some(Source::Shortcut)
        } else if host == "linear.app" {
            Some(Source::Linear)
        } else if host.ends_with(".atlassian.net") {
            Some(Source::Jira)
        } else if host == "app.plane.so" {
            Some(Source::Plane)
        } else {
            None
        };
        let source = selected.or(inferred).ok_or_else(|| Error::Api("use --source for this issue URL".into()))?;
        if inferred.is_some_and(|inferred| inferred != source) {
            return Err(Error::Api("the issue URL and --source disagree".into()));
        }
        let marker = match source {
            Source::Github | Source::Gitlab => "issues",
            Source::Shortcut => "story",
            Source::Linear => "issue",
            Source::Jira | Source::Plane => "browse",
            Source::Asana => "task",
        };
        let key = parts
            .windows(2)
            .find(|parts| parts[0] == marker || (marker == "issues" && parts[0] == "work_items"))
            .map(|parts| parts[1])
            .filter(|key| !key.is_empty())
            .ok_or_else(|| Error::Api("the issue URL has no readable issue key".into()))?;
        return Ok(Locator { source, key: key.into(), url: Some(text.into()) });
    }
    let source = match selected {
        Some(source) => source,
        None if text.starts_with("sc-") => Source::Shortcut,
        None if text.starts_with('#') => {
            only(available, &[Source::Github, Source::Gitlab], "#N needs a GitHub or GitLab repository")?
        }
        None if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()) => only(
            available,
            &[Source::Github, Source::Gitlab, Source::Shortcut],
            "a number needs a repository or Shortcut",
        )?,
        None => {
            only(available, &[Source::Linear, Source::Jira, Source::Plane], "this key needs Linear, Jira or Plane")?
        }
    };
    let key = match source {
        Source::Shortcut => text.trim_start_matches("sc-"),
        Source::Github | Source::Gitlab => text.trim_start_matches('#'),
        _ => text,
    };
    if key.is_empty() || key.contains(char::is_whitespace) {
        return Err(Error::Api("the issue key is empty or contains spaces".into()));
    }
    Ok(Locator { source, key: key.into(), url: None })
}

fn only(available: &[Source], kinds: &[Source], missing: &str) -> Result<Source> {
    let found: Vec<_> = kinds.iter().copied().filter(|kind| available.contains(kind)).collect();
    match found.as_slice() {
        [source] => Ok(*source),
        [] => Err(Error::Api(format!("{missing}; use --source github|gitlab|shortcut|linear|jira|plane"))),
        _ => {
            let names: Vec<_> = found.iter().map(|source| source.id()).collect();
            Err(Error::Api(format!("this key could be {}; use --source to choose", names.join(" or "))))
        }
    }
}

pub(crate) fn find(client: &Client, locator: &Locator) -> Result<Issue> {
    let key = locator.key.as_str();
    let number = || key.parse().map_err(|_| Error::Api("this tracker needs a numeric issue key".into()));
    let issue = match client {
        Client::Github { gh, dir } => crate::issues::github::find(gh, dir, key),
        Client::Gitlab(api) => api.find(number()?),
        Client::Shortcut { base, token } => crate::issues::shortcut::Api { base, token }.find(number()?),
        Client::Linear { url, token } => crate::issues::linear::Api { url, token }.find(key),
        Client::Jira(api) => api.find(key),
        Client::Plane(api) => api.find(key),
        Client::Asana(api) => api.find(key),
    }?;
    match &locator.url {
        Some(url) if place(url) != place(&issue.url) => Err(Error::Api(format!(
            "that URL is not an issue of this project's {}: its {} is {}",
            locator.source.name(),
            issue.key,
            issue.url
        ))),
        _ => Ok(issue),
    }
}

fn place(url: &str) -> Option<Vec<String>> {
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let url = url.split_once("://").map_or(url, |(_, rest)| rest);
    let parts: Vec<String> = url.split('/').filter(|part| !part.is_empty()).map(str::to_lowercase).collect();
    let marker = parts.iter().position(|part| MARKERS.contains(&part.as_str()))?;
    let mut place = parts.get(..marker + 2)?.to_vec();
    if place[marker] == "work_items" {
        place[marker] = "issues".into();
    }
    Some(place)
}

#[cfg(test)]
mod tests;
