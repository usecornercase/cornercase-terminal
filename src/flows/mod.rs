use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agents::{self, Known};
use crate::config::Config;

pub(crate) mod brief;
pub(crate) mod builtin;
pub(crate) mod dialog;
pub(crate) mod progress;
pub(crate) mod run;
pub(crate) mod start;
pub(crate) mod store;

pub(crate) const ORCHESTRATOR: &str = "orchestrator";
pub(crate) const VERSION: u32 = 1;
pub(crate) const MAX_ROLES: usize = 8;
pub(crate) const FALLBACK_BRANCH: &str = "flow";
const BRANCH_WORDS: usize = 8;
const BRANCH_MAX: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Flow {
    pub(crate) version: u32,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) notes: String,
    pub(crate) end: End,
    pub(crate) orchestrator: Role,
    #[serde(rename = "role", skip_serializing_if = "Vec::is_empty")]
    pub(crate) roles: Vec<Role>,
    #[serde(rename = "stage", skip_serializing_if = "Vec::is_empty")]
    pub(crate) stages: Vec<Stage>,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            version: VERSION,
            name: String::new(),
            description: String::new(),
            notes: String::new(),
            end: End::default(),
            orchestrator: Role { name: ORCHESTRATOR.into(), ..Role::default() },
            roles: Vec::new(),
            stages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Role {
    pub(crate) name: String,
    pub(crate) agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) effort: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) args: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) edits: bool,
    pub(crate) prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Stage {
    pub(crate) roles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) back: Option<Back>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Back {
    pub(crate) to: usize,
    pub(crate) when: String,
    pub(crate) at_most: u32,
}

impl Default for Back {
    fn default() -> Self {
        Self { to: 1, when: "problems".into(), at_most: 3 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum End {
    #[default]
    PullRequest,
    Commits,
}

impl End {
    pub(crate) const ALL: [Self; 2] = [Self::PullRequest, Self::Commits];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PullRequest => "open a pull request",
            Self::Commits => "leave the commits",
        }
    }
}

impl Flow {
    pub(crate) fn role(&self, name: &str) -> Option<&Role> {
        if name == ORCHESTRATOR {
            return Some(&self.orchestrator);
        }
        self.roles.iter().find(|role| role.name == name)
    }

    pub(crate) fn agents(&self) -> usize {
        self.roles.len() + 1
    }

    pub(crate) fn stage_of(&self, role: &str) -> Option<usize> {
        self.stages.iter().position(|stage| stage.roles.iter().any(|r| r == role))
    }

    pub(crate) fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.name.trim().is_empty() {
            out.push("the flow has no name".to_string());
        }
        if self.version > VERSION {
            out.push("the flow was made by a newer cornercase".to_string());
        }
        out.extend(role_problems(&self.orchestrator, ORCHESTRATOR));
        if self.roles.is_empty() {
            out.push("the flow has no roles besides the orchestrator".to_string());
        }
        if self.roles.len() > MAX_ROLES {
            out.push(format!("a flow takes at most {MAX_ROLES} roles besides the orchestrator"));
        }
        for (i, role) in self.roles.iter().enumerate() {
            let name = role.name.trim();
            if name.is_empty() {
                out.push(format!("role {} has no name", i + 1));
            } else if name == ORCHESTRATOR {
                out.push(format!("`{ORCHESTRATOR}` is the name of the orchestrator; give the role another one"));
            } else if self.roles[..i].iter().any(|other| other.name.trim() == name) {
                out.push(format!("two roles are named `{name}`"));
            }
            out.extend(role_problems(role, name));
            if !name.is_empty() && self.stage_of(&role.name).is_none() {
                out.push(format!("`{name}` is in no stage"));
            }
        }
        if self.stages.is_empty() {
            out.push("the flow has no stages".to_string());
        }
        out.extend(self.folder_problems());
        for (i, stage) in self.stages.iter().enumerate() {
            let number = i + 1;
            if stage.roles.is_empty() {
                out.push(format!("stage {number} has no roles"));
            }
            for (j, name) in stage.roles.iter().enumerate() {
                if self.roles.iter().all(|role| &role.name != name) {
                    out.push(format!("stage {number} names `{name}`, which is not a role"));
                } else if stage.roles[..j].contains(name) {
                    out.push(format!("stage {number} names `{name}` twice"));
                } else if let Some(k) = self.stages[..i].iter().position(|earlier| earlier.roles.contains(name)) {
                    out.push(format!("`{name}` is in stage {} and stage {number}; a role works in one stage", k + 1));
                }
            }
            if let Some(back) = &stage.back {
                if back.to == 0 || back.to >= number {
                    out.push(format!("stage {number} can only go back to an earlier stage"));
                }
                if back.at_most == 0 {
                    out.push(format!("stage {number} goes back at most 0 times"));
                }
            }
        }
        out
    }
}

impl Flow {
    fn folder_problems(&self) -> Vec<String> {
        let members: Vec<&Role> = std::iter::once(&self.orchestrator).chain(&self.roles).collect();
        Known::ALL
            .into_iter()
            .filter(|kind| kind.status_by_folder())
            .filter_map(|kind| {
                let names: Vec<String> = members
                    .iter()
                    .filter(|role| role.agent == kind.name())
                    .map(|role| format!("`{}`", role.name.trim()))
                    .collect();
                let (last, rest) = names.split_last().filter(|(_, rest)| !rest.is_empty())?;
                let title = kind.title();
                Some(format!(
                    "{title} runs {} and {last}, but cornercase reads {title}'s status from its folder and cannot \
                     tell two apart in one worktree: give all but one of them another agent",
                    rest.join(", ")
                ))
            })
            .collect()
    }
}

impl Role {
    pub(crate) fn summary(&self) -> String {
        let agent = if self.agent.trim().is_empty() { "no agent" } else { self.agent.trim() };
        std::iter::once(agent)
            .chain(chosen(self.model.as_ref()))
            .chain(chosen(self.effort.as_ref()))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

fn role_problems(role: &Role, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if Known::of(&role.agent).is_none() {
        let known = Known::ALL.map(Known::name).join(", ");
        out.push(format!("`{name}` runs `{}`, but a flow takes one of {known}", role.agent));
    } else if let Some(refusal) =
        chosen(role.effort.as_ref()).and_then(|level| agents::effort_refusal(&role.agent, level))
    {
        out.push(format!("`{name}`: {refusal}"));
    }
    for key in role.env.keys() {
        let valid = key.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            out.push(format!("`{name}` sets `{key}`, which is not a variable name"));
        }
    }
    out
}

fn parts(config: &Config, role: &Role) -> Option<(Known, Vec<String>, Vec<String>)> {
    let kind = Known::of(&role.agent)?;
    let name = kind.name();
    let modes = agents::modes(config, name);
    let mut args: Vec<String> = kind.unattended().iter().map(|a| (*a).to_string()).collect();
    args.extend(agents::extra_args(name, &agents::args(config, name), &modes));
    let (picked, effort) = (chosen(role.model.as_ref()), chosen(role.effort.as_ref()));
    let mut own = role.args.clone();
    if picked.is_some() || agents::names_model(name, &own) {
        args = agents::without_model(name, &args);
    }
    if picked.is_some() {
        own = agents::without_model(name, &own);
    }
    if effort.is_some() || agents::names_effort(name, &own) {
        args = agents::without_effort(name, &args);
    }
    if effort.is_some() {
        own = agents::without_effort(name, &own);
    }
    let mut args = agents::with_effort(name, agents::with_model(name, args, picked), effort);
    args.extend(own);
    let mut words: Vec<String> = Vec::new();
    if !role.env.is_empty() {
        words.push("env".to_string());
        words.extend(role.env.iter().map(|(key, value)| format!("{key}={value}")));
    }
    words.push(agents::command(config, name));
    Some((kind, words, args))
}

fn chosen(value: Option<&String>) -> Option<&str> {
    value.map(|value| value.trim()).filter(|value| !value.is_empty())
}

fn quoted(words: impl IntoIterator<Item = String>) -> String {
    words.into_iter().map(|word| agents::quote(&word)).collect::<Vec<_>>().join(" ")
}

pub(crate) fn command(config: &Config, role: &Role) -> Option<String> {
    let (_, words, args) = parts(config, role)?;
    Some(quoted(words.into_iter().chain(args)))
}

pub(crate) fn resume(config: &Config, role: &Role, conversation: String) -> Option<String> {
    let (kind, words, args) = parts(config, role)?;
    let args = kind.resume(args, conversation)?;
    Some(quoted(words.into_iter().chain(args)))
}

pub(crate) fn branch(task: &str) -> String {
    let words: Vec<&str> = task.split_whitespace().take(BRANCH_WORDS).collect();
    let slug = crate::issues::slug(&words.join(" "), BRANCH_MAX);
    if slug.is_empty() { FALLBACK_BRANCH.to_string() } else { slug }
}

#[cfg(test)]
mod tests;
