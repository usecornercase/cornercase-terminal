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

pub(crate) fn primary(dir: &std::path::Path) -> Option<Remote> {
    let mut remotes = of(dir);
    let at = remotes.iter().position(|r| r.name == "origin").unwrap_or(0);
    (at < remotes.len()).then(|| remotes.swap_remove(at))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Repo {
    pub(crate) remotes: Vec<(String, String)>,
    pub(crate) identity: Option<String>,
}

pub(crate) fn identity(remotes: &[(String, String)], resolve: impl Fn(&str) -> Option<String>) -> Option<String> {
    let parsed: Vec<(&str, Remote)> = remotes
        .iter()
        .filter_map(|(name, url)| Some((url.as_str(), remote(name, url)?)))
        .filter(|(_, r)| !r.path.is_empty())
        .collect();
    let named = |wanted: &str| parsed.iter().find(|(_, r)| r.name == wanted);
    let (url, chosen) = named("upstream").or_else(|| named("origin")).or_else(|| parsed.first())?;
    let host =
        if is_ssh(url) { resolve(&chosen.host).unwrap_or_else(|| chosen.host.clone()) } else { chosen.host.clone() };
    let path = chosen.path.to_lowercase();
    Some(format!("{}/{}", host.to_lowercase(), path.strip_suffix(".git").unwrap_or(&path)))
}

pub(crate) fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let Some((user_info, host)) = authority.rsplit_once('@') else { return url.to_string() };
    let user = user_info.split(':').next().unwrap_or_default();
    if !is_ssh_scheme(scheme) || user.is_empty() {
        return format!("{scheme}://{host}{tail}");
    }
    format!("{scheme}://{user}@{host}{tail}")
}

fn is_ssh_scheme(scheme: &str) -> bool {
    ["ssh", "git+ssh", "ssh+git"].iter().any(|ssh| scheme.eq_ignore_ascii_case(ssh))
}

fn is_ssh(url: &str) -> bool {
    ["ssh://", "git+ssh://", "ssh+git://"].iter().any(|scheme| url.starts_with(scheme)) || !url.contains("://")
}
