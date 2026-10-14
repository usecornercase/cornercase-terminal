use std::path::Path;

const MAX_REF: usize = 200;
const MAX_URL: usize = 2000;
const GITHUB: &str = "https://github.com/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) repo: String,
    pub(crate) url: String,
    pub(crate) reference: Option<String>,
}

impl Origin {
    pub(crate) fn parse(spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        if spec.chars().any(|c| c.is_whitespace() || c.is_control()) || spec.len() > MAX_URL {
            return Err(format!("`{spec}` is not a repository: no spaces or control characters"));
        }
        let (place, reference) = match (spec.rfind('@'), spec.rfind('/')) {
            (Some(at), Some(slash)) if at > slash => (&spec[..at], Some(&spec[at + 1..])),
            _ => (spec, None),
        };
        if let Some(reference) = reference {
            valid_ref(reference)?;
        }
        let reference = reference.map(str::to_string);
        if place.starts_with('/') {
            return Ok(Self { repo: place.to_string(), url: place.to_string(), reference });
        }
        if let Some(rest) = place.strip_prefix("https://") {
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            if host.is_empty() || host.contains('@') || path.is_empty() || host.starts_with('-') {
                return Err(format!(
                    "`{place}` is not a repository address; credentials go in git's credential helper, not the address"
                ));
            }
            let repo = match place.strip_prefix(GITHUB) {
                Some(path) => path.trim_end_matches(".git").trim_end_matches('/').to_string(),
                None => place.to_string(),
            };
            return Ok(Self { repo, url: place.to_string(), reference });
        }
        if place.contains("://") || place.contains("::") || place.contains(':') {
            return Err(format!(
                "`{place}` uses a transport cornercase does not install from; use owner/repo, an https:// address \
                 or a folder"
            ));
        }
        let Some((owner, name)) = place.split_once('/') else {
            return Err(format!("`{place}` is not owner/repo, an https:// address or a folder"));
        };
        let name = name.strip_suffix(".git").unwrap_or(name);
        if !valid_owner(owner) || !valid_name(name) {
            return Err(format!("`{place}` is not a GitHub owner/repo"));
        }
        Ok(Self { repo: format!("{owner}/{name}"), url: format!("{GITHUB}{owner}/{name}.git"), reference })
    }

    pub(crate) fn local(&self) -> bool {
        Path::new(&self.url).is_absolute()
    }

    pub(crate) fn protocol(&self) -> &'static str {
        if self.local() { "file" } else { "https" }
    }

    pub(crate) fn spec(&self) -> String {
        match &self.reference {
            Some(reference) => format!("{}@{reference}", self.url),
            None => self.url.clone(),
        }
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
