use std::path::Path;

use crate::config::Config;
use crate::git;
use crate::project::group::Group;
use crate::project::{Project, group_indices};

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

pub fn candidates(groups: &[Group], projects: &[Project], config: &Config, home: Option<&Path>) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = groups
        .iter()
        .map(|g| Candidate {
            kind: Kind::Group,
            goto: Goto::Group(g.id),
            name: g.entry.label(),
            context: String::new(),
            keys: vec![g.entry.name.clone()],
        })
        .collect();
    for (p, group) in projects.iter().zip(group_indices(projects, groups)) {
        let project = p.label(home);
        let place = |workspace, tab| Goto::Place { project: p.id, workspace, tab };
        candidates.push(Candidate {
            kind: Kind::Project,
            goto: place(None, None),
            name: project.clone(),
            context: group.map(|g| groups[g].entry.name.clone()).unwrap_or_default(),
            keys: vec![project.clone()],
        });
        for w in p.workspaces.iter().filter(|w| w.open()) {
            let label = w.label();
            let mut keys = vec![label.clone()];
            keys.extend(w.name.as_ref().and_then(|_| git::branch(&w.path)));
            let goto = place(Some(w.id), None);
            candidates.push(Candidate {
                kind: Kind::Workspace,
                goto,
                name: label.clone(),
                context: project.clone(),
                keys: keys.clone(),
            });
            for t in &w.tabs {
                let name = t.label(config);
                candidates.push(Candidate {
                    kind: Kind::Tab,
                    goto: place(Some(w.id), Some(t.id)),
                    context: format!("{project} › {label}"),
                    keys: std::iter::once(name.clone()).chain(keys.iter().cloned()).collect(),
                    name,
                });
            }
        }
    }
    candidates
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
