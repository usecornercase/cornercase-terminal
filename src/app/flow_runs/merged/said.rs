use std::path::PathBuf;

use crate::app::control::worktrees::Risky;
use crate::control::BuildsFreed;
use crate::text;

#[derive(Debug)]
pub(crate) enum Outcome {
    Freed(BuildsFreed),
    Kept(Risky),
    Failed(String),
}

#[derive(Debug)]
pub(crate) struct TreeFreed {
    pub(crate) role: Option<String>,
    pub(crate) path: PathBuf,
    pub(crate) outcome: Outcome,
}

impl TreeFreed {
    fn place(&self) -> String {
        self.role.as_ref().map_or_else(|| "the worktree".to_string(), |role| format!("{role}'s worktree"))
    }

    fn bytes(&self) -> u64 {
        match &self.outcome {
            Outcome::Freed(freed) => freed.bytes,
            _ => 0,
        }
    }

    fn failed(&self) -> bool {
        match &self.outcome {
            Outcome::Freed(freed) => !freed.failed.is_empty(),
            Outcome::Kept(_) => false,
            Outcome::Failed(_) => true,
        }
    }

    fn parts(&self) -> Vec<String> {
        let place = self.place();
        match &self.outcome {
            Outcome::Freed(freed) => freed_parts(&place, freed),
            Outcome::Kept(risky) => self.kept_part(&place, risky).into_iter().collect(),
            Outcome::Failed(message) => vec![format!("could not free {place}: {message}")],
        }
    }

    fn kept_part(&self, place: &str, risky: &Risky) -> Option<String> {
        let bytes = risky.size.builds_bytes();
        if bytes == 0 {
            return None;
        }
        Some(format!("{place} kept {} of builds: {}", text::size(bytes), risky.why(&self.path)))
    }
}

fn freed_parts(place: &str, freed: &BuildsFreed) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(first) = freed.failed.first() {
        let more = freed.failed.len() - 1;
        let more = if more == 0 { String::new() } else { format!(" and {more} more") };
        parts.push(format!("could not free {place}: {}: {}{more}", first.path, first.error));
    }
    for folder in &freed.kept {
        parts.push(format!("{place} kept {folder}: no longer an ignored folder there, or on another filesystem"));
    }
    parts
}

pub(super) fn builds_said(trees: &[TreeFreed]) -> (String, bool) {
    let total: u64 = trees.iter().map(TreeFreed::bytes).sum();
    let mut parts = Vec::new();
    if total > 0 {
        parts.push(format!("{} of builds freed", text::size(total)));
    }
    parts.extend(trees.iter().flat_map(TreeFreed::parts));
    let failed = trees.iter().any(TreeFreed::failed);
    if parts.is_empty() {
        return ("no builds to free".to_string(), failed);
    }
    (parts.join("; "), failed)
}

#[cfg(test)]
mod tests;
