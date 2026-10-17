use serde::{Deserialize, Serialize};

use super::Report;
use crate::project::split::{Dir, Node};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Watch {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Snapshot {
    pub(crate) revision: u64,
    #[serde(flatten)]
    pub(crate) report: Report,
}

impl Snapshot {
    pub(crate) fn streamed(&self) -> Option<String> {
        let value = serde_json::to_value(self).ok()?;
        serde_json::to_string(&super::Response::Ok(value)).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layout {
    Leaf(u64),
    Split { dir: Dir, ratio: f64, first: Box<Layout>, second: Box<Layout> },
}

impl From<&Node<u64>> for Layout {
    fn from(node: &Node<u64>) -> Self {
        match node {
            Node::Leaf(id) => Layout::Leaf(*id),
            Node::Split { dir, ratio, first, second } => Layout::Split {
                dir: *dir,
                ratio: ratio.to_string().parse().unwrap_or_else(|_| f64::from(*ratio)),
                first: Box::new(Layout::from(first.as_ref())),
                second: Box::new(Layout::from(second.as_ref())),
            },
        }
    }
}
