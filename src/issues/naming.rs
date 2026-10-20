use super::{Issue, SLUG_MAX, Source, slug};

pub(crate) const PLACEHOLDERS: [&str; 5] = ["key", "number", "slug", "source", "user"];

const USER_MAX: usize = 24;

enum Part<'a> {
    Text(&'a str),
    Field(&'a str),
}

fn parts(template: &str) -> Result<Vec<Part<'_>>, String> {
    let mut parts = Vec::new();
    let mut rest = template;
    while !rest.is_empty() {
        match (rest.find('{'), rest.find('}')) {
            (open, Some(close)) if open.is_none_or(|open| close < open) => return Err("a } has no {".into()),
            (Some(open), _) => {
                if open > 0 {
                    parts.push(Part::Text(&rest[..open]));
                }
                let after = &rest[open + 1..];
                let Some(end) = after.find('}') else { return Err("a { has no }".into()) };
                let name = &after[..end];
                if !PLACEHOLDERS.contains(&name) {
                    return Err(format!("unknown placeholder {{{name}}}: use {}", list()));
                }
                parts.push(Part::Field(name));
                rest = &after[end + 1..];
            }
            (None, _) => {
                parts.push(Part::Text(rest));
                rest = "";
            }
        }
    }
    Ok(parts)
}

fn list() -> String {
    let names: Vec<String> = PLACEHOLDERS.iter().map(|name| format!("{{{name}}}")).collect();
    let (last, rest) = names.split_last().map_or(("", &[][..]), |(last, rest)| (last.as_str(), rest));
    format!("{} or {last}", rest.join(", "))
}

pub(crate) fn uses_user(template: &str) -> bool {
    template.contains("{user}")
}

fn key(issue: &Issue) -> &str {
    issue.key.trim_start_matches('#')
}

fn value(name: &str, issue: &Issue, user: &str) -> String {
    match name {
        "key" => key(issue).to_string(),
        "number" => number(issue),
        "slug" => slug(&issue.title, SLUG_MAX),
        "source" => issue.source.id().to_string(),
        _ => slug(user, USER_MAX),
    }
}

fn number(issue: &Issue) -> String {
    let key = key(issue);
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    match key.rsplit_once('-') {
        Some((_, n)) if digits(n) => n.to_string(),
        _ if digits(key) => key.to_string(),
        _ if issue.number > 0 => issue.number.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn branch(issue: &Issue, template: &str, user: &str) -> String {
    let template = template.trim();
    if template.is_empty() {
        return super::branch(issue);
    }
    let Ok(parts) = parts(template) else { return super::branch(issue) };
    let text: String = parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => (*text).to_string(),
            Part::Field(name) => value(name, issue, user),
        })
        .collect();
    let branch = clean(&text);
    if branch.is_empty() { super::branch(issue) } else { branch }
}

pub(crate) fn clean(text: &str) -> String {
    text.split('/').map(component).filter(|part| !part.is_empty()).collect::<Vec<_>>().join("/")
}

fn component(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' };
        let last = out.chars().last();
        if (c == '-' && last == Some('-')) || (c == '.' && last == Some('.')) {
            continue;
        }
        out.push(c);
    }
    let mut out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    while let Some(stem) = out.strip_suffix(".lock") {
        out = stem.trim_end_matches(['-', '.']).to_string();
    }
    out
}

#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "git refuses a part ending in .lock, in lower case only"
)]
pub(crate) fn check_branch(text: &str) -> Result<String, String> {
    let branch = text.trim();
    let bad = |why: &str| Err(format!("{branch} cannot be a branch: {why}"));
    if branch.is_empty() {
        return Err("the worktree needs a branch".into());
    }
    if let Some(c) = branch.chars().find(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(*c)) {
        return bad(&format!("it holds {c:?}"));
    }
    if branch.starts_with('-') {
        return bad("it starts with -");
    }
    if branch == "@" || branch.contains("@{") || branch.contains("..") || branch.contains("//") {
        return bad("git refuses @, @{, .. and //");
    }
    if branch.starts_with('/') || branch.ends_with('/') || branch.ends_with('.') {
        return bad("it starts or ends with / or ends with .");
    }
    if branch.split('/').any(|part| part.starts_with('.') || part.ends_with(".lock")) {
        return bad("a part starts with . or ends with .lock");
    }
    Ok(branch.to_string())
}

pub(crate) fn sample(source: Source) -> Issue {
    let (number, key) = match source {
        Source::Github | Source::Gitlab => (482, "#482".to_string()),
        Source::Shortcut => (482, "sc-482".to_string()),
        Source::Asana => (0, "ENG-482".to_string()),
        Source::Linear | Source::Jira | Source::Plane => (482, "ENG-482".to_string()),
    };
    Issue {
        source,
        number,
        key,
        title: "Empty address crashes the returns page".into(),
        url: String::new(),
        state: String::new(),
        closed: false,
        labels: Vec::new(),
        assignees: Vec::new(),
        author: String::new(),
        updated_at: String::new(),
    }
}

pub(crate) fn check(template: &str) -> Result<String, String> {
    let template = template.trim();
    if template.is_empty() {
        return Ok(String::new());
    }
    let parts = parts(template)?;
    if !parts.iter().any(|part| matches!(part, Part::Field("key" | "number"))) {
        return Err("the template needs {key} or {number}, so each issue gets a branch of its own".into());
    }
    Ok(template.to_string())
}

pub(crate) fn note(template: &str) -> (String, bool) {
    let template = template.trim();
    if template.is_empty() {
        return ("empty: issue-482-…, 482-… on GitLab, sc-482-…, ENG-482-…".into(), false);
    }
    let example = branch(&sample(Source::Linear), template, "ada");
    let gitlab = branch(&sample(Source::Gitlab), template, "ada");
    if gitlab.starts_with("482-") {
        (format!("such as {example}"), false)
    } else {
        (format!("such as {example} · GitLab closes issues only from {{number}}-… branches"), true)
    }
}

#[cfg(test)]
mod tests;
