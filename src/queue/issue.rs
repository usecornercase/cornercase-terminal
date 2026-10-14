use crate::error::{Error, Result};
use crate::issues::{Client, Issue, Source};

pub(crate) fn locator(text: &str, source: Option<&str>, connected: &[Source]) -> Result<(Source, String)> {
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
        } else if parts.windows(2).any(|parts| parts == ["-", "issues"]) {
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
        if matches!(source, Source::Github | Source::Gitlab) {
            return Ok((source, text.into()));
        }
        let marker = match source {
            Source::Shortcut => "story",
            Source::Linear => "issue",
            Source::Jira | Source::Plane => "browse",
            Source::Gitlab => "issues",
            Source::Github => unreachable!(),
        };
        let key = parts
            .windows(2)
            .find(|parts| parts[0] == marker)
            .map(|parts| parts[1])
            .ok_or_else(|| Error::Api("the issue URL has no readable issue key".into()))?;
        return Ok((source, key.into()));
    }
    let source = selected
        .or_else(|| text.starts_with("sc-").then_some(Source::Shortcut))
        .or_else(|| text.starts_with('#').then_some(Source::Github))
        .or_else(|| (connected.len() == 1).then(|| connected[0]))
        .ok_or_else(|| {
            Error::Api("use --source github|gitlab|shortcut|linear|jira|plane with this issue key".into())
        })?;
    let key = match source {
        Source::Shortcut => text.trim_start_matches("sc-"),
        Source::Github | Source::Gitlab => text.trim_start_matches('#'),
        _ => text,
    };
    if key.is_empty() || key.contains(char::is_whitespace) {
        return Err(Error::Api("the issue key is empty or contains spaces".into()));
    }
    Ok((source, key.into()))
}

pub(crate) fn find(client: &Client, key: &str) -> Result<Issue> {
    let number = || key.parse().map_err(|_| Error::Api("this tracker needs a numeric issue key".into()));
    match client {
        Client::Github { gh, dir } => crate::issues::github::find(gh, dir, key),
        Client::Gitlab(api) => api.find(key),
        Client::Shortcut { base, token } => crate::issues::shortcut::Api { base, token }.find(number()?),
        Client::Linear { url, token } => crate::issues::linear::Api { url, token }.find(key),
        Client::Jira(api) => api.find(key),
        Client::Plane(api) => api.find(key),
    }
}

#[cfg(test)]
mod tests;
