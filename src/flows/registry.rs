use std::path::{Path, PathBuf};
use std::time::Duration;

use super::share;
use crate::http::{self, Service};

pub(crate) const DEFAULT_URL: &str = "https://raw.githubusercontent.com";
pub(crate) const URL_ENV: &str = "CORNERCASE_FLOWS_URL";
pub(crate) const FILES: [&str; 2] = ["flow.toml", "cornercase-flow.toml"];
pub(crate) const PAGE: &str = "https://usecornercase.dev/flows/";
const TIMEOUT: Duration = Duration::from_secs(20);
const SERVICE: Service = Service { name: "the flow's host", rejected: "the flow's host refused the request" };

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    Stdin,
    File(PathBuf),
    Url(String),
    Repo(String),
}

impl Source {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Stdin => "-".to_string(),
            Self::File(path) => path.display().to_string(),
            Self::Url(url) => url.clone(),
            Self::Repo(repo) => repo.clone(),
        }
    }

    pub(crate) fn page(&self) -> String {
        match self {
            Self::Repo(repo) => format!("https://github.com/{repo}"),
            other => other.label(),
        }
    }

    pub(crate) fn address(&self) -> String {
        self.urls().into_iter().next().unwrap_or_else(|| self.label())
    }

    fn urls(&self) -> Vec<String> {
        match self {
            Self::Url(url) => vec![url.clone()],
            Self::Repo(repo) => FILES.iter().map(|file| format!("{}/{repo}/HEAD/{file}", base_url())).collect(),
            Self::Stdin | Self::File(_) => Vec::new(),
        }
    }
}

pub(crate) fn base_url() -> String {
    std::env::var(URL_ENV).unwrap_or_else(|_| DEFAULT_URL.to_string()).trim_end_matches('/').to_string()
}

pub(crate) fn is_repo(text: &str) -> bool {
    let part = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    text.split_once('/').is_some_and(|(owner, repo)| part(owner) && part(repo))
}

pub(crate) fn source(arg: &str, exists: impl Fn(&Path) -> bool) -> Result<Source, String> {
    if arg == "-" {
        return Ok(Source::Stdin);
    }
    if arg.starts_with("https://") {
        return Ok(Source::Url(arg.to_string()));
    }
    if arg.starts_with("http://") {
        return Err("use an https:// address".to_string());
    }
    let path = Path::new(arg);
    if exists(path) {
        return Ok(Source::File(path.to_path_buf()));
    }
    if is_repo(arg) {
        return Ok(Source::Repo(arg.to_string()));
    }
    Ok(Source::File(path.to_path_buf()))
}

pub(crate) fn typed(text: &str) -> Option<Source> {
    let text = text.trim();
    if text.contains('\n') {
        return None;
    }
    match source(text, |_| false) {
        Ok(found @ (Source::Url(_) | Source::Repo(_))) => Some(found),
        _ => None,
    }
}

fn too_long(url: &str) -> String {
    format!("{url} is too long for a flow (at most {} KiB)", share::MAX_BYTES / 1024)
}

pub(crate) fn fetch(source: &Source) -> Result<String, String> {
    let limit = share::MAX_BYTES as u64 + 1;
    let mut last = String::new();
    for url in source.urls() {
        match http::download_strictly(&SERVICE, &url, limit, TIMEOUT) {
            Ok(bytes) if bytes.len() > share::MAX_BYTES => {
                return Err(too_long(&url));
            }
            Ok(bytes) => return String::from_utf8(bytes).map_err(|_| format!("{url} is not text")),
            Err(e) if e.to_string() == http::TOO_LARGE => return Err(too_long(&url)),
            Err(e) if e.to_string().contains("answered 404") => last = url,
            Err(e) => {
                return Err(format!("cannot fetch {url}: {e}; download the file and import it from its path instead"));
            }
        }
    }
    Err(match source {
        Source::Repo(repo) => format!("no {} in {repo}: see {PAGE}", FILES.join(" or ")),
        _ => format!("nothing at {last}"),
    })
}

#[cfg(test)]
mod tests;
