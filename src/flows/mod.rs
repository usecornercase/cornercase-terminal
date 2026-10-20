use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agents::{self, Known};
use crate::config::Config;

pub(crate) mod brief;
pub(crate) mod builtin;
pub(crate) mod dialog;
pub(crate) mod folder;
pub(crate) mod progress;
pub(crate) mod registry;
pub(crate) mod run;
pub(crate) mod share;
pub(crate) mod start;
pub(crate) mod store;

pub(crate) const ORCHESTRATOR: &str = "orchestrator";
pub(crate) const VERSION: u32 = 2;
const OWN_WORKTREE_VERSION: u32 = 2;
pub(crate) const MAX_ROLES: usize = 8;
pub(crate) const FALLBACK_BRANCH: &str = "flow";
const BRANCH_WORDS: usize = 8;
const BRANCH_MAX: usize = 40;
const ROLE_SLUG_MAX: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Flow {
    #[serde(default = "first_version")]
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
    #[serde(skip_serializing_if = "Worktree::is_shared")]
    pub(crate) worktree: Worktree,
    pub(crate) prompt: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Worktree {
    #[default]
    Shared,
    Own,
}

impl Worktree {
    pub(crate) const ALL: [Self; 2] = [Self::Shared, Self::Own];

    #[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes a reference")]
    pub(crate) fn is_shared(&self) -> bool {
        *self == Self::Shared
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Own => "own",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Shared => "with the team, in the run's worktree",
            Self::Own => "a worktree and branch of its own, for competing coders and prototypes",
        }
    }
}

fn first_version() -> u32 {
    1
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

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::PullRequest => "pull_request",
            Self::Commits => "commits",
        }
    }

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

    pub(crate) fn file_version(&self) -> u32 {
        if self.roles.iter().any(Role::own_worktree) { OWN_WORKTREE_VERSION } else { 1 }
    }

    pub(crate) fn agents(&self) -> usize {
        self.roles.len() + 1
    }

    pub(crate) fn stage_of(&self, role: &str) -> Option<usize> {
        self.stages.iter().position(|stage| stage.roles.iter().any(|r| r == role))
    }

    pub(crate) fn problems(&self) -> Vec<String> {
        let mut out = self.leading_problems();
        out.extend(self.role_list_problems());
        if self.stages.is_empty() {
            out.push("the flow has no stages".to_string());
        }
        out.extend(self.folder_problems());
        out.extend(self.branch_problems());
        for (i, stage) in self.stages.iter().enumerate() {
            out.extend(self.stage_problems(i, stage));
        }
        out
    }
}

impl Flow {
    fn leading_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.name.trim().is_empty() {
            out.push("the flow has no name".to_string());
        }
        if self.version > VERSION {
            out.push("the flow was made by a newer cornercase".to_string());
        }
        out.extend(self.control_problems());
        out.extend(role_problems(&self.orchestrator, ORCHESTRATOR));
        if self.orchestrator.own_worktree() {
            out.push(format!(
                "the {ORCHESTRATOR} works in the run's worktree: only a role can have a worktree of its own"
            ));
        }
        out
    }

    fn role_list_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.roles.is_empty() {
            out.push("the flow has no roles besides the orchestrator".to_string());
        }
        if self.roles.len() > MAX_ROLES {
            out.push(format!("a flow takes at most {MAX_ROLES} roles besides the orchestrator"));
        }
        for (i, role) in self.roles.iter().enumerate() {
            let name = role.name.trim();
            out.extend(self.name_problem(i, name));
            out.extend(role_problems(role, name));
            if !name.is_empty() && self.stage_of(&role.name).is_none() {
                out.push(format!("`{name}` is in no stage"));
            }
        }
        out
    }

    fn name_problem(&self, i: usize, name: &str) -> Option<String> {
        if name.is_empty() {
            Some(format!("role {} has no name", i + 1))
        } else if name == ORCHESTRATOR {
            Some(format!("`{ORCHESTRATOR}` is the name of the orchestrator; give the role another one"))
        } else if self.roles[..i].iter().any(|other| other.name.trim() == name) {
            Some(format!("two roles are named `{name}`"))
        } else {
            None
        }
    }

    fn stage_problems(&self, i: usize, stage: &Stage) -> Vec<String> {
        let number = i + 1;
        let mut out = Vec::new();
        if stage.roles.is_empty() {
            out.push(format!("stage {number} has no roles"));
        }
        for (j, name) in stage.roles.iter().enumerate() {
            out.extend(self.member_problem(i, j, name));
        }
        out.extend(self.sharing_problem(number, stage));
        if let Some(back) = &stage.back {
            if back.to == 0 || back.to >= number {
                out.push(format!("stage {number} can only go back to an earlier stage"));
            }
            if back.at_most == 0 {
                out.push(format!("stage {number} goes back at most 0 times"));
            }
        }
        out
    }

    fn member_problem(&self, i: usize, j: usize, name: &str) -> Option<String> {
        let number = i + 1;
        if self.roles.iter().all(|role| role.name != name) {
            Some(format!("stage {number} names `{name}`, which is not a role"))
        } else if self.stages[i].roles[..j].iter().any(|r| r == name) {
            Some(format!("stage {number} names `{name}` twice"))
        } else {
            let k = self.stages[..i].iter().position(|earlier| earlier.roles.iter().any(|r| r == name))?;
            Some(format!("`{name}` is in stage {} and stage {number}; a role works in one stage", k + 1))
        }
    }
}

impl Flow {
    fn control_problems(&self) -> Vec<String> {
        let bad = |text: &str| text.chars().any(char::is_control);
        let mut out = Vec::new();
        let mut field = |found: bool, what: String| {
            if found {
                out.push(format!("{what} contains a control character"));
            }
        };
        field(bad(&self.name), "the flow's name".into());
        field(bad(&self.description), "the flow's description".into());
        for (i, role) in std::iter::once(&self.orchestrator).chain(&self.roles).enumerate() {
            let which = if i == 0 { "the orchestrator".to_string() } else { format!("role {i}") };
            let fields = [&role.name, &role.agent];
            let model = role.model.iter().chain(&role.effort);
            field(fields.into_iter().chain(model).any(|t| bad(t)), format!("{which}'s name, agent, model or effort"));
            field(role.env.keys().any(|key| bad(key)), format!("{which}'s variable names"));
        }
        for (i, stage) in self.stages.iter().enumerate() {
            field(stage.roles.iter().any(|r| bad(r)), format!("stage {}'s roles", i + 1));
            field(stage.back.as_ref().is_some_and(|back| bad(&back.when)), format!("stage {}'s loop label", i + 1));
        }
        out
    }

    fn sharing_problem(&self, number: usize, stage: &Stage) -> Option<String> {
        let sharing: Vec<String> = stage
            .roles
            .iter()
            .filter_map(|name| self.roles.iter().find(|role| &role.name == name))
            .filter(|role| role.edits && !role.own_worktree())
            .map(|role| format!("`{}`", role.name.trim()))
            .collect();
        let (last, rest) = sharing.split_last().filter(|(_, rest)| !rest.is_empty())?;
        Some(format!(
            "stage {number} runs {} and {last} side by side, and they both edit the shared worktree: give all but \
             one of them a worktree of its own, or put them in stages of their own",
            rest.join(", ")
        ))
    }

    fn branch_problems(&self) -> Vec<String> {
        let mut seen: Vec<(String, &str)> = Vec::new();
        let mut out = Vec::new();
        for role in self.roles.iter().filter(|role| role.own_worktree()) {
            let branch = role_branch("", &role.name);
            if let Some((_, first)) = seen.iter().find(|(other, _)| *other == branch) {
                out.push(format!("`{first}` and `{}` would work on the same branch: rename one", role.name.trim()));
            } else {
                seen.push((branch, role.name.trim()));
            }
        }
        out
    }

    fn folder_problems(&self) -> Vec<String> {
        let members: Vec<&Role> =
            std::iter::once(&self.orchestrator).chain(self.roles.iter().filter(|role| !role.own_worktree())).collect();
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
                     tell two apart in one worktree: give all but one of them another agent, or a worktree of its own",
                    rest.join(", ")
                ))
            })
            .collect()
    }
}

impl Role {
    pub(crate) fn own_worktree(&self) -> bool {
        self.worktree == Worktree::Own
    }

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

fn parts(config: &Config, role: &Role, dir: Option<&Path>) -> Option<(Known, Vec<String>, Vec<String>)> {
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
    if let (Some(flag), Some(dir)) = (kind.add_dir(), dir.and_then(Path::to_str)) {
        args.push(flag.to_string());
        args.push(dir.to_string());
    }
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

pub(crate) fn command(config: &Config, role: &Role, dir: Option<&Path>) -> Option<String> {
    let (_, words, args) = parts(config, role, dir)?;
    Some(quoted(words.into_iter().chain(args)))
}

pub(crate) fn resume(config: &Config, role: &Role, conversation: String, dir: Option<&Path>) -> Option<String> {
    let (kind, words, args) = parts(config, role, dir)?;
    let args = kind.resume(args, conversation)?;
    Some(quoted(words.into_iter().chain(args)))
}

pub(crate) fn role_branch(branch: &str, role: &str) -> String {
    let slug = crate::issues::slug(role, ROLE_SLUG_MAX);
    format!("{branch}--{}", if slug.is_empty() { "role" } else { &slug })
}

pub(crate) fn branch(task: &str) -> String {
    let words: Vec<&str> = task.split_whitespace().take(BRANCH_WORDS).collect();
    let slug = crate::issues::slug(&words.join(" "), BRANCH_MAX);
    if slug.is_empty() { FALLBACK_BRANCH.to_string() } else { slug }
}

#[cfg(test)]
mod tests;
