use serde::{Deserialize, Serialize};

use super::Fields;
use crate::issues::{self, Issue, Person};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Who {
    #[default]
    Anyone,
    Me,
    Person {
        handle: String,
        #[serde(default)]
        id: Option<String>,
    },
}

impl Who {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Anyone => "anyone",
            Self::Me => "me",
            Self::Person { .. } => "person",
        }
    }

    pub(crate) fn to_issues(&self) -> Result<issues::Who, String> {
        match self {
            Self::Anyone => Ok(issues::Who::Anyone),
            Self::Me => Ok(issues::Who::Me),
            Self::Person { handle, id } => {
                let handle = handle.trim().trim_start_matches('@');
                if handle.is_empty() {
                    return Err("a person needs a handle".into());
                }
                Ok(match id {
                    Some(id) => issues::Who::User { id: id.clone(), name: handle.into() },
                    None => issues::Who::Person(handle.into()),
                })
            }
        }
    }
}

impl From<&issues::Who> for Who {
    fn from(who: &issues::Who) -> Self {
        match who {
            issues::Who::Anyone => Self::Anyone,
            issues::Who::Me => Self::Me,
            issues::Who::Person(handle) => Self::Person { handle: handle.clone(), id: None },
            issues::Who::User { id, name } => Self::Person { handle: name.clone(), id: Some(id.clone()) },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct IssueSources {
    pub(crate) project: Option<u64>,
}

impl IssueSources {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct IssueList {
    pub(crate) project: Option<u64>,
    pub(crate) source: Option<String>,
    pub(crate) closed: bool,
    pub(crate) people: [Who; 2],
    pub(crate) cached: bool,
}

impl IssueList {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project).maybe("source", self.source.as_ref()).add("closed", self.closed);
        let kinds: Vec<&str> = self.people.iter().map(Who::kind).collect();
        fields.add("people", kinds.join(",")).add("cached", self.cached);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct IssueRead {
    pub(crate) project: Option<u64>,
    pub(crate) issue: String,
    pub(crate) source: Option<String>,
}

impl IssueRead {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project).maybe("source", self.source.as_ref()).add("bytes", self.issue.len());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct IssuePeople {
    pub(crate) project: Option<u64>,
    pub(crate) source: String,
    pub(crate) cached: bool,
}

impl IssuePeople {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project).add("source", &self.source).add("cached", self.cached);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceState {
    Connected,
    Missing,
    Failing,
    #[serde(other)]
    #[default]
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SourceInfo {
    pub(crate) source: String,
    pub(crate) name: String,
    pub(crate) tab: bool,
    pub(crate) repository: bool,
    pub(crate) state: SourceState,
    pub(crate) missing: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) env: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) shadowed: bool,
    pub(crate) fields: [String; 2],
    pub(crate) people: [Who; 2],
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PlaceInfo {
    pub(crate) project: u64,
    pub(crate) workspace: Option<u64>,
    pub(crate) label: String,
    pub(crate) worktree: bool,
    pub(crate) here: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_field_names, reason = "the answer's JSON key is sources")]
pub(crate) struct Sources {
    pub(crate) project: u64,
    pub(crate) closed: bool,
    pub(crate) sources: Vec<SourceInfo>,
    pub(crate) places: Vec<PlaceInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct IssueInfo {
    pub(crate) source: String,
    pub(crate) key: String,
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) closed: bool,
    pub(crate) labels: Vec<String>,
    pub(crate) assignees: Vec<String>,
    pub(crate) author: String,
    pub(crate) updated_at: String,
    pub(crate) branch: String,
}

impl From<&Issue> for IssueInfo {
    fn from(issue: &Issue) -> Self {
        Self {
            source: issue.source.id().into(),
            key: issue.key.clone(),
            number: issue.number,
            title: issue.title.clone(),
            url: issue.url.clone(),
            state: issue.state.clone(),
            closed: issue.closed,
            labels: issue.labels.clone(),
            assignees: issue.assignees.clone(),
            author: issue.author.clone(),
            updated_at: issue.updated_at.clone(),
            branch: issues::branch(issue),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ListedInfo {
    pub(crate) source: String,
    pub(crate) count: usize,
    pub(crate) cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Lists {
    pub(crate) project: u64,
    pub(crate) issues: Vec<IssueInfo>,
    pub(crate) sources: Vec<ListedInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CommentInfo {
    pub(crate) author: String,
    pub(crate) created_at: String,
    pub(crate) body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ReadIssue {
    pub(crate) issue: IssueInfo,
    pub(crate) info: Vec<String>,
    pub(crate) body: String,
    pub(crate) comments: Vec<CommentInfo>,
    pub(crate) prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersonInfo {
    pub(crate) handle: String,
    pub(crate) name: String,
    pub(crate) id: Option<String>,
}

impl From<&Person> for PersonInfo {
    fn from(person: &Person) -> Self {
        Self { handle: person.handle.clone(), name: person.name.clone(), id: person.id.clone() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PeopleInfo {
    pub(crate) source: String,
    pub(crate) fields: [String; 2],
    pub(crate) cached: bool,
    pub(crate) people: Vec<PersonInfo>,
}

#[cfg(test)]
mod tests;
