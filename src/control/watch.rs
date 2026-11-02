use serde::{Deserialize, Serialize};

use super::Report;
use crate::project::split::{Dir, Node};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Watch {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Snapshot<T = Report> {
    pub(crate) revision: u64,
    #[serde(flatten)]
    pub(crate) report: T,
}

impl<T: Serialize> Snapshot<T> {
    pub(crate) fn streamed(&self) -> Option<String> {
        super::streamed_line(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layout {
    Leaf(u64),
    Split { dir: Dir, ratio: f64, first: Box<Layout>, second: Box<Layout> },
}

impl From<&Layout> for Node<u64> {
    fn from(layout: &Layout) -> Self {
        match layout {
            Layout::Leaf(id) => Node::Leaf(*id),
            Layout::Split { dir, ratio, first, second } => Node::Split {
                dir: *dir,
                #[expect(clippy::cast_possible_truncation, reason = "a ratio between 0 and 1")]
                ratio: *ratio as f32,
                first: Box::new(Node::from(first.as_ref())),
                second: Box::new(Node::from(second.as_ref())),
            },
        }
    }
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
