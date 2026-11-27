use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Source {
    Question,
    Permission,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Awaited {
    pub(crate) source: Source,
    pub(crate) tool: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) questions: Vec<Question>,
    pub(crate) open: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Question {
    pub(crate) header: Option<String>,
    #[serde(rename = "question")]
    pub(crate) text: String,
    pub(crate) multi_select: bool,
    pub(crate) free_text: bool,
    pub(crate) options: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Choice {
    pub(crate) number: usize,
    pub(crate) label: String,
    pub(crate) description: Option<String>,
    pub(crate) selected: Option<bool>,
}

impl Awaited {
    pub(crate) fn questions(questions: Vec<Question>) -> Self {
        Self { source: Source::Question, tool: None, command: None, questions, open: None }
    }

    pub(crate) fn permission(tool: String, command: Option<String>) -> Self {
        Self { source: Source::Permission, tool: Some(tool), command, questions: Vec::new(), open: None }
    }
}
