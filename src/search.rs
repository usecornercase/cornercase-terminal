#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Group,
    Project,
    Workspace,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goto {
    Group(u64),
    Place { project: u64, workspace: Option<u64>, tab: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub kind: Kind,
    pub goto: Goto,
    pub name: String,
    pub context: String,
    pub keys: Vec<String>,
}

pub fn rank(candidates: Vec<Candidate>, query: &str) -> Vec<Candidate> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, Candidate)> = candidates
        .into_iter()
        .filter_map(|c| c.keys.iter().filter_map(|key| key_rank(&key.to_lowercase(), &needle)).min().map(|r| (r, c)))
        .collect();
    hits.sort_by_key(|(rank, c)| (*rank, c.kind));
    hits.into_iter().map(|(_, c)| c).collect()
}

fn key_rank(key: &str, needle: &str) -> Option<u8> {
    if key == needle {
        Some(0)
    } else if key.starts_with(needle) {
        Some(1)
    } else if key.contains(needle) {
        Some(2)
    } else {
        None
    }
}

pub fn find(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let hay: Vec<char> = haystack.chars().map(fold).collect();
    let needle: Vec<char> = needle.trim().chars().map(fold).collect();
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == needle[..]).map(|i| (i, i + needle.len()))
}

#[cfg(test)]
mod tests;
