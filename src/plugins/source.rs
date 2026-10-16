use std::path::Path;

const MAX_REF: usize = 200;
const MAX_URL: usize = 2000;
const GITHUB: &str = "https://github.com/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) repo: String,
    pub(crate) url: String,
    pub(crate) reference: Option<String>,
    pub(crate) subdir: Option<String>,
}

impl Origin {
    pub(crate) fn parse(spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        if spec.chars().any(|c| c.is_whitespace() || c.is_control()) || spec.len() > MAX_URL {
            return Err(format!("`{spec}` is not a repository: no spaces or control characters"));
        }
        let (place, reference) = match spec.split_once('@') {
            Some((place, reference)) => (place, Some(reference)),
            None => (spec, None),
        };
        if let Some(reference) = reference {
            valid_ref(reference)?;
        }
        let reference = reference.map(str::to_string);
        if place.starts_with('/') {
            let (path, subdir) = split_subdir(place, place.find("//"))?;
            return Ok(Self { repo: path.to_string(), url: path.to_string(), reference, subdir });
        }
        if let Some(rest) = place.strip_prefix("https://") {
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            if host.is_empty() || host.contains('@') || path.is_empty() || host.starts_with('-') {
                return Err(format!(
                    "`{place}` is not a repository address; credentials go in git's credential helper, not the address"
                ));
            }
            let marker = rest.find("//").map(|at| at + "https://".len());
            if marker.is_none()
                && let Some(path) = place.strip_prefix(GITHUB)
            {
                return github(path, reference);
            }
            let (url, subdir) = split_subdir(place, marker)?;
            let repo = match url.strip_prefix(GITHUB) {
                Some(path) => path.trim_end_matches('/').trim_end_matches(".git").to_string(),
                None => url.to_string(),
            };
            return Ok(Self { repo, url: url.to_string(), reference, subdir });
        }
        if place.contains("://") || place.contains("::") || place.contains(':') {
            return Err(format!(
                "`{place}` uses a transport cornercase does not install from; use owner/repo, an https:// address \
                 or a folder"
            ));
        }
        if !place.contains('/') {
            return Err(format!("`{place}` is not owner/repo, an https:// address or a folder"));
        }
        github(place, reference)
    }

    pub(crate) fn local(&self) -> bool {
        Path::new(&self.url).is_absolute()
    }

    pub(crate) fn protocol(&self) -> &'static str {
        if self.local() { "file" } else { "https" }
    }

    pub(crate) fn spec(&self) -> String {
        let subdir = self.subdir.as_ref().map_or_else(String::new, |subdir| format!("//{subdir}"));
        let at = self.reference.as_ref().map_or_else(String::new, |reference| format!("@{reference}"));
        format!("{}{subdir}{at}", self.url)
    }

    pub(crate) fn place(&self) -> String {
        match &self.subdir {
            Some(subdir) => format!("{} (folder {subdir})", self.repo),
            None => self.repo.clone(),
        }
    }
}

fn github(path: &str, reference: Option<String>) -> Result<Origin, String> {
    let mut parts = path.trim_end_matches('/').splitn(3, '/');
    let (owner, name) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
    let name = name.strip_suffix(".git").unwrap_or(name);
    if !valid_owner(owner) || !valid_name(name) {
        return Err(format!("`{path}` is not a GitHub owner/repo"));
    }
    let subdir = parts.next().map(valid_subdir).transpose()?;
    Ok(Origin { repo: format!("{owner}/{name}"), url: format!("{GITHUB}{owner}/{name}.git"), reference, subdir })
}

fn split_subdir(place: &str, marker: Option<usize>) -> Result<(&str, Option<String>), String> {
    match marker {
        Some(at) => Ok((&place[..at], Some(valid_subdir(&place[at + 2..])?))),
        None => Ok((place, None)),
    }
}

fn valid_subdir(subdir: &str) -> Result<String, String> {
    let subdir = subdir.trim_end_matches('/');
    let good = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && !part.starts_with('-')
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    if subdir.split('/').all(good) {
        Ok(subdir.to_string())
    } else {
        Err(format!(
            "`{subdir}` is not a folder in the repository: no empty, `.` or `..` parts, none starting with `-`"
        ))
    }
}

fn valid_owner(owner: &str) -> bool {
    (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && !name.starts_with(['-', '.'])
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn valid_ref(reference: &str) -> Result<(), String> {
    let chars = reference.bytes().all(|b| b.is_ascii_alphanumeric() || b"._/+-".contains(&b));
    if reference.is_empty()
        || reference.len() > MAX_REF
        || !chars
        || reference.starts_with(['-', '/', '.'])
        || reference.contains("..")
    {
        return Err(format!("`{reference}` is not a branch, tag or commit"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
