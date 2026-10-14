use super::edit::{DEFAULT_MODEL, Editing, Row, Tab};
use super::{Agents, RowText};
use crate::agents::{self, Known};
use crate::text::one_line;

const TEXT_MARK: &str = "✎";

impl Editing {
    pub(crate) fn row_texts(&self, agents: &Agents) -> Vec<RowText> {
        self.rows().into_iter().map(|row| self.row_text(row, agents)).collect()
    }

    fn row_text(&self, row: Row, agents: &Agents) -> RowText {
        let text = |label: &str, value: String, note: &str| RowText {
            section: if self.tab == Tab::Flow && !matches!(row, Row::Name | Row::Description | Row::Place) {
                "Plan"
            } else {
                ""
            },
            label: label.to_string(),
            value,
            note: note.to_string(),
            ..RowText::default()
        };
        let flow = &self.flow;
        let back = |i: usize| flow.stages.get(i).and_then(|stage| stage.back.as_ref());
        match row {
            Row::Name => text("name", flow.name.clone(), ""),
            Row::Description => text("description", flow.description.clone(), ""),
            Row::Place => {
                let file = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
                text("saved in", self.place.label().into(), &file.unwrap_or_else(|| "not saved yet".into()))
            }
            Row::Stage(i) => {
                let stage = &flow.stages[i];
                let roles = if stage.roles.is_empty() { "no roles yet".to_string() } else { stage.roles.join(" + ") };
                let note =
                    back(i).map(|b| format!("{} → {} · at most {}", b.when, b.to, b.at_most)).unwrap_or_default();
                let arrow = if self.open == Some(i) { "▾" } else { "▸" };
                RowText { removable: true, movable: true, ..text(&format!("{arrow} {}", i + 1), roles, &note) }
            }
            Row::Roles(i) => text("    roles", flow.stages[i].roles.join(", "), "several work side by side"),
            Row::Back(i) => match back(i) {
                Some(b) => text("    goes back", format!("to stage {}", b.to), "when there are problems"),
                None => text("    goes back", "never".into(), "the plan moves on"),
            },
            Row::When(i) => text("    when", back(i).map(|b| b.when.clone()).unwrap_or_default(), "what sends it back"),
            Row::AtMost(i) => text(
                "    at most",
                back(i).map(|b| format!("{} times", b.at_most)).unwrap_or_default(),
                "then it moves on",
            ),
            Row::AddStage => text("+ stage", String::new(), ""),
            Row::End => text("◆ at the end", flow.end.label().into(), ""),
            Row::Notes => text("notes", one_line(&flow.notes), TEXT_MARK),
            Row::RoleName | Row::Agent | Row::Model | Row::Args | Row::Env | Row::Edits | Row::Prompt => {
                self.role_row(row, agents)
            }
        }
    }

    fn role_row(&self, row: Row, agents: &Agents) -> RowText {
        let Some(role) = self.role() else { return RowText::default() };
        let text = |label: &str, value: String, note: String| RowText {
            label: label.to_string(),
            value,
            note,
            ..RowText::default()
        };
        match row {
            Row::RoleName => text("name", role.name.clone(), String::new()),
            Row::Agent => {
                let note = match Known::of(&role.agent) {
                    Some(kind) if agents.installed.contains(&kind) => kind.title().to_string(),
                    Some(_) => "not installed".to_string(),
                    None => "not an agent a flow can start".to_string(),
                };
                RowText { red: Known::of(&role.agent).is_none(), ..text("agent", role.agent.clone(), note) }
            }
            Row::Model => text("model", role.model.clone().unwrap_or_else(|| DEFAULT_MODEL.into()), String::new()),
            Row::Args => text("args", or_none(agents::join_args(&role.args)), "added to the agent's command".into()),
            Row::Env => {
                let env = role.env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ");
                text("env", or_none(env), "KEY=value pairs".into())
            }
            Row::Edits => {
                let (value, note) = if role.edits {
                    ("[x] yes", "it may change and commit files")
                } else {
                    ("[ ] no", "it is told not to change files")
                };
                text("edits files", value.into(), note.into())
            }
            _ => text("prompt", one_line(&role.prompt), TEXT_MARK.into()),
        }
    }

    pub(crate) fn field_label(&self) -> String {
        let Some(field) = &self.field else { return String::new() };
        match field.row {
            Row::Name | Row::RoleName => "name",
            Row::Description => "description",
            Row::When(_) => "goes back when",
            Row::AtMost(_) => "at most",
            Row::Model => "model",
            Row::Args => "args",
            Row::Env => "env",
            _ => "",
        }
        .into()
    }

    pub(crate) fn pick_title(&self) -> String {
        let Some(pick) = &self.pick else { return String::new() };
        let role = self.role().map(|role| role.name.trim().to_string()).unwrap_or_default();
        match pick.row {
            Row::Place => "Where should the flow be saved?".into(),
            Row::Roles(i) => format!("Which roles work in stage {}?", i + 1),
            Row::Back(i) => format!("Where does stage {} send the work back when there are problems?", i + 1),
            Row::End => "What happens at the end?".into(),
            Row::Agent => format!("Which agent is the {role}?"),
            Row::Model => format!("Which model does the {role} use?"),
            _ => String::new(),
        }
    }
}

fn or_none(value: String) -> String {
    if value.is_empty() { "none".into() } else { value }
}
