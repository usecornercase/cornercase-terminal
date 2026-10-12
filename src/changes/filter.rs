use super::diff::{Diff, File};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    query: String,
    pub focused: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self { query: String::new(), focused: true }
    }
}

impl Filter {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn push(&mut self, c: char) {
        self.query.push(c);
    }

    pub fn pop(&mut self) {
        self.query.pop();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    Part(String),
    Glob(String),
}

impl Pattern {
    fn new(word: &str) -> Self {
        if word.contains(['*', '?']) { Self::Glob(word.to_string()) } else { Self::Part(word.to_string()) }
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::Part(part) => {
                let fold = smart_fold(part);
                let (part, path) = (chars(part, fold), chars(path, fold));
                path.windows(part.len()).any(|w| w == part.as_slice())
            }
            Self::Glob(pattern) => glob_matches(pattern, path),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

impl Query {
    pub fn parse(raw: &str) -> Self {
        let mut query = Self::default();
        for word in raw.split_whitespace() {
            match word.strip_prefix('!') {
                Some(excluded) if !excluded.is_empty() => query.exclude.push(Pattern::new(excluded)),
                _ => query.include.push(Pattern::new(word)),
            }
        }
        query
    }

    pub fn keeps(&self, file: &File) -> bool {
        let paths = [Some(file.path.as_str()), file.old_path.as_deref()];
        let matches = |pattern: &Pattern| paths.iter().flatten().any(|path| pattern.matches(path));
        (self.include.is_empty() || self.include.iter().any(matches)) && !self.exclude.iter().any(matches)
    }
}

pub fn kept(diff: &Diff, raw: &str) -> Vec<bool> {
    let query = Query::parse(raw);
    diff.files.iter().map(|f| query.keeps(f)).collect()
}

fn chars(text: &str, fold: bool) -> Vec<char> {
    text.chars().map(|c| if fold { c.to_lowercase().next().unwrap_or(c) } else { c }).collect()
}

fn smart_fold(pattern: &str) -> bool {
    !pattern.chars().any(char::is_uppercase)
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let fold = smart_fold(pattern);
    let (pattern, dir) = pattern.strip_suffix('/').map_or((pattern, false), |p| (p, true));
    let (pattern, anchored) = pattern.strip_prefix('/').map_or((pattern, false), |p| (p, true));
    if anchored || pattern.contains('/') {
        let path = chars(path, fold);
        let whole = chars(pattern, fold);
        let below = chars(&format!("{pattern}/**"), fold);
        return (!dir && glob(&whole, &path)) || glob(&below, &path);
    }
    let pattern = chars(pattern, fold);
    let parts: Vec<&str> = path.split('/').collect();
    let last = parts.len().saturating_sub(1);
    parts.iter().enumerate().any(|(i, part)| (!dir || i < last) && glob(&pattern, &chars(part, fold)))
}

fn glob(pattern: &[char], text: &[char]) -> bool {
    match pattern {
        [] => text.is_empty(),
        ['*', '*', rest @ ..] => {
            let starts = (0..=text.len()).filter(|&i| i == 0 || text[i - 1] == '/');
            let skip_slash =
                rest.strip_prefix(&['/']).is_some_and(|after| starts.into_iter().any(|i| glob(after, &text[i..])));
            skip_slash || (0..=text.len()).any(|i| glob(rest, &text[i..]))
        }
        ['*', rest @ ..] => {
            let end = text.iter().position(|c| *c == '/').unwrap_or(text.len());
            (0..=end).any(|i| glob(rest, &text[i..]))
        }
        ['?', rest @ ..] => text.first().is_some_and(|c| *c != '/') && glob(rest, &text[1..]),
        [c, rest @ ..] => text.first() == Some(c) && glob(rest, &text[1..]),
    }
}

#[cfg(test)]
mod tests;
