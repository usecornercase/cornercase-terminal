use std::path::Path;

use serde::Deserialize;

use super::{Ci, State, conflicting_of, state_of};
use crate::error::{Error, Result};
use crate::gh;

const QUERY: &str = "query($url: URI!) { resource(url: $url) { ... on PullRequest { stack { number size baseRefName \
    entries(first: 100) { nodes { position pullRequest { number title url state isDraft mergeable reviewDecision \
    statusCheckRollup { state } } } } } } } }";

const UNDEFINED_FIELD: &str = "undefinedField";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stack {
    pub(crate) number: u64,
    pub(crate) base: String,
    pub(crate) size: usize,
    pub(crate) layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layer {
    pub(crate) position: usize,
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: State,
    pub(crate) checks: Option<Ci>,
    pub(crate) conflicting: Option<bool>,
    pub(crate) review: Option<Review>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Review {
    Approved,
    ChangesRequested,
    Required,
}

impl Review {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
            Self::Required => "review_required",
        }
    }

    fn of(decision: Option<&str>) -> Option<Self> {
        match decision {
            Some("APPROVED") => Some(Self::Approved),
            Some("CHANGES_REQUESTED") => Some(Self::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(Self::Required),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Block {
    Conflicts,
    Checks,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StackMark {
    pub(crate) position: usize,
    pub(crate) size: usize,
    pub(crate) blocked: Option<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer {
    Alone,
    In(Stack),
    Unsupported,
}

impl Layer {
    pub(crate) fn conflicts(&self) -> bool {
        !self.state.settled() && self.conflicting == Some(true)
    }

    fn block(&self) -> Option<Block> {
        if self.state.settled() {
            None
        } else if self.conflicts() {
            Some(Block::Conflicts)
        } else if self.checks == Some(Ci::Failing) {
            Some(Block::Checks)
        } else if self.review == Some(Review::ChangesRequested) {
            Some(Block::Review)
        } else {
            None
        }
    }
}

impl Stack {
    pub(crate) fn position_of(&self, number: u64) -> Option<usize> {
        self.layers.iter().find(|l| l.number == number).map(|l| l.position)
    }

    pub(crate) fn blocker(&self, number: u64) -> Option<(&Layer, Block)> {
        let position = self.position_of(number)?;
        let below = || self.layers.iter().filter(move |l| l.position < position);
        let lowest = |wanted: fn(Block) -> bool| {
            below().find_map(|layer| layer.block().filter(|b| wanted(*b)).map(|b| (layer, b)))
        };
        lowest(|b| b != Block::Review).or_else(|| lowest(|b| b == Block::Review))
    }

    pub(crate) fn mark(&self, number: u64, open: bool) -> Option<StackMark> {
        let position = self.position_of(number)?;
        let blocked = if open { self.blocker(number).map(|(_, block)| block) } else { None };
        Some(StackMark { position, size: self.size, blocked })
    }

    pub(crate) fn keep_known(&mut self, before: &Self) {
        if self.number != before.number {
            return;
        }
        for layer in self.layers.iter_mut().filter(|l| l.conflicting.is_none()) {
            if let Some(known) = before.layers.iter().find(|b| b.number == layer.number) {
                layer.conflicting = known.conflicting;
            }
        }
    }
}

pub(crate) fn host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split('/').next().filter(|h| !h.is_empty())?;
    Some(host.to_lowercase())
}

pub(crate) fn read(gh: &Path, dir: &Path, url: &str) -> Result<Answer> {
    let Some(host) = host(url) else { return Ok(Answer::Alone) };
    let query = format!("query={QUERY}");
    let url = format!("url={url}");
    let args = ["api", "graphql", "--hostname", &host, "-f", &query, "-f", &url];
    let out = gh::run_output(gh, dir, &args)?;
    let body = serde_json::from_slice::<Body>(&out.stdout);
    if body.as_ref().is_ok_and(|b| b.errors.iter().any(|e| e.extensions.code == UNDEFINED_FIELD)) {
        return Ok(Answer::Unsupported);
    }
    if !out.status.success() {
        return Err(Error::Gh(gh::failure(&out, "gh")));
    }
    Ok(body.map_err(Error::GhOutput)?.answer())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Body {
    data: Option<Data>,
    errors: Vec<Failure>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Failure {
    extensions: Extensions,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Extensions {
    code: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Data {
    resource: Option<Resource>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Resource {
    stack: Option<RawStack>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RawStack {
    number: u64,
    size: usize,
    base_ref_name: String,
    entries: Entries,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Entries {
    nodes: Vec<Entry>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Entry {
    position: usize,
    pull_request: Option<RawPull>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RawPull {
    number: u64,
    title: String,
    url: String,
    state: String,
    is_draft: bool,
    mergeable: Option<String>,
    review_decision: Option<String>,
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Rollup {
    state: String,
}

impl Body {
    fn answer(self) -> Answer {
        match self.data.and_then(|d| d.resource).and_then(|r| r.stack) {
            Some(stack) => Answer::In(stack.into()),
            None => Answer::Alone,
        }
    }
}

impl From<RawStack> for Stack {
    fn from(raw: RawStack) -> Self {
        let mut layers: Vec<Layer> = raw.entries.nodes.into_iter().filter_map(Entry::layer).collect();
        layers.sort_by_key(|l| l.position);
        Self { number: raw.number, base: raw.base_ref_name, size: raw.size, layers }
    }
}

impl Entry {
    fn layer(self) -> Option<Layer> {
        let pull = self.pull_request?;
        let checks = match pull.status_check_rollup.as_ref().map(|r| r.state.as_str()) {
            Some("FAILURE" | "ERROR") => Some(Ci::Failing),
            Some("PENDING" | "EXPECTED") => Some(Ci::Pending),
            Some("SUCCESS") => Some(Ci::Passing),
            _ => None,
        };
        Some(Layer {
            position: self.position,
            number: pull.number,
            title: pull.title,
            url: pull.url,
            state: state_of(&pull.state, pull.is_draft),
            checks,
            conflicting: conflicting_of(pull.mergeable.as_deref()),
            review: Review::of(pull.review_decision.as_deref()),
        })
    }
}

#[cfg(test)]
mod tests;
