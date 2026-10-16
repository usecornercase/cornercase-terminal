#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Remote {
    pub(crate) name: String,
    pub(crate) host: String,
    pub(crate) path: String,
    pub(crate) web: bool,
}

pub(crate) fn split_url(url: &str) -> Option<(String, String)> {
    let (_, rest) = url.split_once("://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    (!host.is_empty()).then(|| (host, path.trim_matches('/').to_string()))
}

pub(crate) fn remote(name: &str, url: &str) -> Option<Remote> {
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

pub(crate) fn remotes(output: &str) -> Vec<Remote> {
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

pub(crate) fn of(dir: &std::path::Path) -> Vec<Remote> {
    super::remote_urls(dir).iter().filter_map(|(name, url)| remote(name, url)).collect()
}
