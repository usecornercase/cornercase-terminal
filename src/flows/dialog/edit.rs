use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Agents, CANCEL, DELETE_ROLE, DIAGRAM, DONE, FORM, Place, Room, SAVE};
use crate::agents::{self, Known};
use crate::flows::{Back, End, Flow, MAX_ROLES, ORCHESTRATOR, Role, Stage, Worktree, builtin};
use crate::list::Search;
use crate::todo::editor::Editor;

pub(crate) const DEFAULT_MODEL: &str = "default";
pub(crate) const OTHER_MODEL: &str = "other…";
const NEVER: &str = "never";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Flow,
    Orchestrator,
    Role(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Name,
    Description,
    Place,
    Stage(usize),
    Roles(usize),
    Back(usize),
    When(usize),
    AtMost(usize),
    AddStage,
    End,
    Notes,
    RoleName,
    Agent,
    Model,
    Effort,
    Args,
    Env,
    Edits,
    Worktree,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) row: Row,
    pub(crate) input: String,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) note: String,
}

#[derive(Debug)]
pub(crate) struct Pick {
    pub(crate) row: Row,
    pub(crate) items: Vec<Choice>,
    pub(crate) search: Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Text {
    pub(crate) row: Row,
    pub(crate) editor: Editor,
    original: String,
}

#[derive(Debug)]
pub(crate) struct Editing {
    pub(crate) flow: Flow,
    original: Flow,
    pub(crate) place: Place,
    original_place: Place,
    pub(crate) path: Option<PathBuf>,
    pub(crate) tab: Tab,
    pub(crate) cursor: usize,
    pub(crate) open: Option<usize>,
    pub(crate) field: Option<Field>,
    pub(crate) pick: Option<Pick>,
    pub(crate) text: Option<Text>,
    pub(crate) diagram: bool,
    pub(crate) notice: Option<String>,
    pub(crate) preview: u16,
    pub(crate) imported: bool,
    followed: Option<usize>,
}

fn open_stage_rows(i: usize, stage: &Stage) -> Vec<Row> {
    let mut rows = vec![Row::Roles(i), Row::Back(i)];
    if stage.back.is_some() {
        rows.extend([Row::When(i), Row::AtMost(i)]);
    }
    rows
}

impl Editing {
    pub(crate) fn new(flow: Flow, place: Place, path: Option<PathBuf>) -> Self {
        Self {
            original: flow.clone(),
            flow,
            place,
            original_place: place,
            path,
            tab: Tab::Flow,
            cursor: 0,
            open: None,
            field: None,
            pick: None,
            text: None,
            diagram: false,
            notice: None,
            preview: 0,
            imported: false,
            followed: None,
        }
    }

    pub(crate) fn imported(flow: Flow) -> Self {
        Self { original: Flow::default(), imported: true, ..Self::new(flow, Place::Yours, None) }
    }

    pub(crate) fn preview_stage(&self) -> Option<usize> {
        if self.tab != Tab::Flow {
            return None;
        }
        let row = self.pick.as_ref().map(|pick| pick.row).or_else(|| self.field.as_ref().map(|field| field.row));
        match row.or_else(|| self.rows().get(self.cursor).copied())? {
            Row::Stage(i) | Row::Roles(i) | Row::Back(i) | Row::When(i) | Row::AtMost(i) => Some(i),
            _ => None,
        }
    }

    pub(crate) fn newly_focused(&mut self) -> Option<usize> {
        let stage = self.preview_stage();
        if stage == self.followed {
            return None;
        }
        self.followed = stage;
        stage
    }

    pub(crate) fn scroll_preview(&mut self, delta: isize, most: u16) {
        self.preview = super::scrolled(self.preview, delta, most);
    }

    pub(crate) fn is_project_flow(&self) -> bool {
        self.place == Place::Project || self.original_place == Place::Project
    }

    pub(crate) fn dirty(&self) -> bool {
        self.flow != self.original
            || self.place != self.original_place
            || self.text.as_ref().is_some_and(|text| text.editor.text() != text.original)
    }

    pub(crate) fn saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.original = self.flow.clone();
        self.original_place = self.place;
    }

    pub(crate) fn role(&self) -> Option<&Role> {
        match self.tab {
            Tab::Flow => None,
            Tab::Orchestrator => Some(&self.flow.orchestrator),
            Tab::Role(i) => self.flow.roles.get(i),
        }
    }

    fn role_mut(&mut self) -> Option<&mut Role> {
        match self.tab {
            Tab::Flow => None,
            Tab::Orchestrator => Some(&mut self.flow.orchestrator),
            Tab::Role(i) => self.flow.roles.get_mut(i),
        }
    }

    fn flow_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Name, Row::Description, Row::Place];
        for (i, stage) in self.flow.stages.iter().enumerate() {
            rows.push(Row::Stage(i));
            if self.open == Some(i) {
                rows.extend(open_stage_rows(i, stage));
            }
        }
        rows.extend([Row::AddStage, Row::End, Row::Notes]);
        rows
    }

    pub(crate) fn rows(&self) -> Vec<Row> {
        match self.tab {
            Tab::Flow => self.flow_rows(),
            Tab::Orchestrator => vec![Row::Agent, Row::Model, Row::Effort, Row::Args, Row::Env, Row::Prompt],
            Tab::Role(_) => {
                vec![
                    Row::RoleName,
                    Row::Agent,
                    Row::Model,
                    Row::Effort,
                    Row::Args,
                    Row::Env,
                    Row::Edits,
                    Row::Worktree,
                    Row::Prompt,
                ]
            }
        }
    }

    pub(crate) fn tabs(&self) -> Vec<String> {
        let roles = self.flow.roles.iter().enumerate().map(|(i, role)| {
            let name = role.name.trim();
            if name.is_empty() { format!("role {}", i + 1) } else { name.to_string() }
        });
        ["Flow".to_string(), "Orchestrator".to_string()]
            .into_iter()
            .chain(roles)
            .chain([super::ADD_ROLE.into()])
            .collect()
    }

    pub(crate) fn tab_index(&self) -> usize {
        match self.tab {
            Tab::Flow => 0,
            Tab::Orchestrator => 1,
            Tab::Role(i) => i + 2,
        }
    }

    pub(crate) fn show_tab(&mut self, i: usize, agents: &Agents) {
        let tab = match i {
            0 => Tab::Flow,
            1 => Tab::Orchestrator,
            i if i - 2 < self.flow.roles.len() => Tab::Role(i - 2),
            _ => return self.add_role(agents),
        };
        self.go_to(tab);
    }

    pub(crate) fn step_tab(&mut self, delta: isize) {
        let count = self.flow.roles.len() + 2;
        let at = self.tab_index().saturating_add_signed(delta.rem_euclid(isize::try_from(count).unwrap_or(1)));
        self.go_to(match at % count {
            0 => Tab::Flow,
            1 => Tab::Orchestrator,
            i => Tab::Role(i - 2),
        });
    }

    fn go_to(&mut self, tab: Tab) {
        self.tab = tab;
        self.cursor = 0;
        self.field = None;
        self.pick = None;
        self.notice = None;
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < self.rows().len() {
            self.cursor = i;
        }
    }

    pub(crate) fn step(&mut self, delta: isize) {
        let last = self.rows().len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(delta).min(last);
    }

    fn select_row(&mut self, row: Row) {
        if let Some(i) = self.rows().iter().position(|r| *r == row) {
            self.cursor = i;
        }
    }

    pub(crate) fn activate(&mut self, agents: &Agents) {
        let Some(row) = self.rows().get(self.cursor).copied() else { return };
        self.notice = None;
        match row {
            Row::Name => self.start_field(row, self.flow.name.clone()),
            Row::Description => self.start_field(row, self.flow.description.clone()),
            Row::Stage(i) => {
                self.open = if self.open == Some(i) { None } else { Some(i) };
                self.select_row(row);
            }
            Row::When(i) => {
                let when = self.flow.stages[i].back.as_ref().map(|b| b.when.clone()).unwrap_or_default();
                self.start_field(row, when);
            }
            Row::AtMost(i) => {
                let at_most = self.flow.stages[i].back.as_ref().map_or(0, |b| b.at_most);
                self.start_field(row, at_most.to_string());
            }
            Row::AddStage => self.add_stage(),
            Row::Notes => self.open_text(row, &self.flow.notes.clone()),
            Row::Edits => {
                if let Some(role) = self.role_mut() {
                    role.edits = !role.edits;
                }
            }
            Row::RoleName | Row::Args | Row::Env | Row::Prompt => self.activate_role(row),
            Row::Effort if self.effort_levels().is_none() => {}
            Row::Place
            | Row::Roles(_)
            | Row::Back(_)
            | Row::End
            | Row::Agent
            | Row::Model
            | Row::Effort
            | Row::Worktree => {
                let items = self.choices(row, agents);
                self.pick = Some(Pick { row, items, search: Search::default() });
            }
        }
    }

    fn activate_role(&mut self, row: Row) {
        let Some(role) = self.role() else { return };
        match row {
            Row::RoleName => self.start_field(row, role.name.clone()),
            Row::Args => self.start_field(row, agents::join_args(&role.args)),
            Row::Env => {
                let env = role.env.iter().map(|(k, v)| agents::quote(&format!("{k}={v}"))).collect::<Vec<_>>();
                self.start_field(row, env.join(" "));
            }
            _ => self.open_text(row, &role.prompt.clone()),
        }
    }

    fn start_field(&mut self, row: Row, input: String) {
        self.field = Some(Field { row, input, error: None });
    }

    fn open_text(&mut self, row: Row, text: &str) {
        let mut editor = Editor::new(text);
        editor.home();
        self.text = Some(Text { row, editor, original: text.to_string() });
    }

    pub(crate) fn text_heading(&self) -> String {
        match (self.text.as_ref().map(|t| t.row), self.role()) {
            (Some(Row::Prompt), Some(role)) => format!("{} › prompt", role.name.trim()),
            (Some(Row::Prompt), None) => "prompt".into(),
            _ => "notes for the orchestrator".into(),
        }
    }

    pub(crate) fn keep(&mut self) {
        if let Some(text) = self.text.take() {
            let value = text.editor.text().trim_end().to_string();
            match text.row {
                Row::Prompt => self.set_prompt(value),
                _ => self.flow.notes = value,
            }
        } else if self.field.is_some() {
            self.submit_field();
        }
    }

    fn set_prompt(&mut self, value: String) {
        if let Some(role) = self.role_mut() {
            role.prompt = value;
        }
    }

    pub(crate) fn drop_open(&mut self) {
        if self.text.take().is_none() && self.field.take().is_none() {
            self.pick = None;
        }
    }

    pub(crate) fn submit_field(&mut self) {
        let Some(field) = self.field.take() else { return };
        let value = field.input.trim().to_string();
        let refused = match field.row {
            Row::Name if value.is_empty() => Some("a flow needs a name".to_string()),
            Row::Name => {
                self.flow.name = value;
                None
            }
            Row::Description => {
                self.flow.description = value;
                None
            }
            Row::When(i) => self.set_when(i, value),
            Row::AtMost(i) => self.set_at_most(i, &value),
            Row::RoleName => self.rename_role(&value),
            Row::Model => {
                if let Some(role) = self.role_mut() {
                    role.model = (!value.is_empty()).then_some(value);
                }
                None
            }
            Row::Args => {
                if let Some(role) = self.role_mut() {
                    role.args = agents::split_args(&value);
                }
                None
            }
            Row::Env => self.set_env(&value),
            _ => None,
        };
        if let Some(error) = refused {
            self.field = Some(Field { error: Some(error), ..field });
        }
    }

    fn set_when(&mut self, i: usize, value: String) -> Option<String> {
        if value.is_empty() {
            return Some("say when it goes back, such as findings".into());
        }
        if let Some(back) = self.flow.stages.get_mut(i).and_then(|s| s.back.as_mut()) {
            back.when = value;
        }
        None
    }

    fn set_at_most(&mut self, i: usize, value: &str) -> Option<String> {
        let Some(times) = value.parse::<u32>().ok().filter(|n| (1..=99).contains(n)) else {
            return Some("write a number from 1 to 99".into());
        };
        if let Some(back) = self.flow.stages.get_mut(i).and_then(|s| s.back.as_mut()) {
            back.at_most = times;
        }
        None
    }

    fn set_env(&mut self, value: &str) -> Option<String> {
        let mut env = BTreeMap::new();
        for pair in agents::split_args(value) {
            let Some((key, value)) = pair.split_once('=').filter(|(key, _)| !key.is_empty()) else {
                return Some(format!("write KEY=value pairs; `{pair}` has no ="));
            };
            env.insert(key.to_string(), value.to_string());
        }
        if let Some(role) = self.role_mut() {
            role.env = env;
        }
        None
    }

    fn rename_role(&mut self, name: &str) -> Option<String> {
        let Tab::Role(i) = self.tab else { return None };
        if name.is_empty() {
            return Some("a role needs a name".into());
        }
        if name == ORCHESTRATOR {
            return Some(format!("`{ORCHESTRATOR}` is taken by the orchestrator"));
        }
        if self.flow.roles.iter().enumerate().any(|(j, role)| j != i && role.name.trim() == name) {
            return Some(format!("another role is named `{name}`"));
        }
        let old = std::mem::replace(&mut self.flow.roles[i].name, name.to_string());
        for role in self.flow.stages.iter_mut().flat_map(|stage| stage.roles.iter_mut()).filter(|role| **role == old) {
            name.clone_into(role);
        }
        None
    }

    pub(crate) fn add_stage(&mut self) {
        self.flow.stages.push(Stage::default());
        let i = self.flow.stages.len() - 1;
        self.open = Some(i);
        self.select_row(Row::Roles(i));
        let items = self.role_choices(i);
        self.pick = Some(Pick { row: Row::Roles(i), items, search: Search::default() });
    }

    pub(crate) fn remove_stage(&mut self, i: usize) {
        if i >= self.flow.stages.len() {
            return;
        }
        self.flow.stages.remove(i);
        let removed = i + 1;
        for stage in &mut self.flow.stages {
            match &mut stage.back {
                Some(back) if back.to == removed => stage.back = None,
                Some(back) if back.to > removed => back.to -= 1,
                _ => {}
            }
        }
        self.fix_loops();
        self.follow(|k| match k.cmp(&i) {
            std::cmp::Ordering::Less => Some(k),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(k - 1),
        });
        self.step(0);
    }

    pub(crate) fn move_stage(&mut self, i: usize, up: bool) {
        let Some(j) = (if up { i.checked_sub(1) } else { Some(i + 1) }) else { return };
        if j >= self.flow.stages.len() || i >= self.flow.stages.len() {
            return;
        }
        self.flow.stages.swap(i, j);
        for back in self.flow.stages.iter_mut().filter_map(|stage| stage.back.as_mut()) {
            if back.to == i + 1 {
                back.to = j + 1;
            } else if back.to == j + 1 {
                back.to = i + 1;
            }
        }
        self.fix_loops();
        self.follow(|k| {
            Some(if k == i {
                j
            } else if k == j {
                i
            } else {
                k
            })
        });
        self.select_row(Row::Stage(j));
    }

    fn follow(&mut self, moved: impl Fn(usize) -> Option<usize>) {
        self.open = self.open.and_then(&moved);
        let Some(field) = &self.field else { return };
        let row = match field.row {
            Row::When(k) => moved(k).map(Row::When),
            Row::AtMost(k) => moved(k).map(Row::AtMost),
            _ => return,
        };
        let looping = |row: Row| match row {
            Row::When(k) | Row::AtMost(k) => self.flow.stages.get(k).is_some_and(|stage| stage.back.is_some()),
            _ => false,
        };
        match row.filter(|row| looping(*row)) {
            Some(row) => {
                if let Some(field) = &mut self.field {
                    field.row = row;
                }
            }
            None => self.field = None,
        }
    }

    fn fix_loops(&mut self) {
        for (n, stage) in self.flow.stages.iter_mut().enumerate() {
            if stage.back.as_ref().is_some_and(|back| back.to == 0 || back.to > n) {
                stage.back = None;
            }
        }
    }

    pub(crate) fn add_role(&mut self, agents: &Agents) {
        if self.flow.roles.len() >= MAX_ROLES {
            self.notice = Some(format!("a flow takes at most {MAX_ROLES} roles besides the orchestrator"));
            return;
        }
        let taken = |name: &str| self.flow.roles.iter().any(|role| role.name.trim() == name);
        let name = (self.flow.roles.len() + 1..=MAX_ROLES + 1)
            .map(|n| format!("role {n}"))
            .find(|n| !taken(n))
            .unwrap_or_default();
        let taken: Vec<Known> = std::iter::once(&self.flow.orchestrator)
            .chain(self.flow.roles.iter().filter(|role| !role.own_worktree()))
            .filter_map(|role| Known::of(&role.agent))
            .collect();
        self.flow.roles.push(builtin::member(&name, &agents.installed, agents.default, &taken));
        self.flow.stages.push(Stage { roles: vec![name], back: None });
        self.go_to(Tab::Role(self.flow.roles.len() - 1));
        self.start_field(Row::RoleName, String::new());
    }

    pub(crate) fn delete_role(&mut self) {
        let Tab::Role(i) = self.tab else { return };
        let name = self.flow.roles.remove(i).name;
        for stage in &mut self.flow.stages {
            stage.roles.retain(|role| *role != name);
        }
        while let Some(empty) = self.flow.stages.iter().position(|stage| stage.roles.is_empty()) {
            self.remove_stage(empty);
        }
        self.go_to(Tab::Flow);
        self.notice = Some(format!("deleted the role {}", name.trim()));
    }

    pub(crate) fn remove_row(&mut self, i: usize) {
        if let Some(Row::Stage(stage)) = self.rows().get(i).copied() {
            self.remove_stage(stage);
        }
    }

    pub(crate) fn move_row(&mut self, i: usize, up: bool) {
        if let Some(Row::Stage(stage)) = self.rows().get(i).copied() {
            self.move_stage(stage, up);
        }
    }

    fn role_choices(&self, stage: usize) -> Vec<Choice> {
        let choice = |role: &Role| {
            let note = match self.stage_elsewhere(stage, &role.name) {
                Some(other) => format!("in stage {}: a role works in one stage", other + 1),
                None => role.agent.clone(),
            };
            Choice { value: role.name.clone(), label: role.name.clone(), note }
        };
        self.flow.roles.iter().map(choice).collect()
    }

    fn stage_elsewhere(&self, stage: usize, name: &str) -> Option<usize> {
        self.flow.stages.iter().enumerate().position(|(k, other)| k != stage && other.roles.iter().any(|r| r == name))
    }

    fn choices(&self, row: Row, agents: &Agents) -> Vec<Choice> {
        let choice = |value: String, label: String, note: &str| Choice { value, label, note: note.to_string() };
        match row {
            Row::Place => {
                let mut items = vec![choice("yours".into(), Place::Yours.label().into(), "your flows folder")];
                if agents.project || self.place == Place::Project {
                    let note = ".cornercase/flows in the project, to share it";
                    items.push(choice("project".into(), Place::Project.label().into(), note));
                }
                items
            }
            Row::Roles(i) => self.role_choices(i),
            Row::Back(i) => {
                let never = choice(NEVER.into(), NEVER.into(), "the plan moves on");
                let earlier = self.flow.stages.iter().take(i).enumerate().map(|(k, stage)| {
                    choice((k + 1).to_string(), format!("stage {}", k + 1), &stage.roles.join(" + "))
                });
                std::iter::once(never).chain(earlier).collect()
            }
            Row::End => End::ALL.iter().map(|end| choice(end.label().into(), end.label().into(), "")).collect(),
            Row::Agent => Known::ALL
                .iter()
                .map(|kind| {
                    let note = if agents.installed.contains(kind) {
                        kind.title().to_string()
                    } else {
                        format!("{} · not installed", kind.title())
                    };
                    choice(kind.name().into(), kind.name().into(), &note)
                })
                .collect(),
            Row::Model => self.model_choices(agents),
            Row::Effort => self.effort_choices(),
            Row::Worktree => {
                Worktree::ALL.iter().map(|kind| choice(kind.name().into(), kind.name().into(), kind.label())).collect()
            }
            _ => Vec::new(),
        }
    }

    fn model_choices(&self, agents: &Agents) -> Vec<Choice> {
        let kind = self.role().and_then(|role| Known::of(&role.agent));
        let default =
            Choice { value: DEFAULT_MODEL.into(), label: DEFAULT_MODEL.into(), note: "the agent's own".into() };
        let known = kind.map(|kind| agents.models(kind)).unwrap_or_default();
        let known = known.into_iter().map(|model| Choice { value: model.clone(), label: model, note: String::new() });
        let reading = kind.is_some_and(|kind| agents.reading(kind));
        let note = if reading { "reading the models…" } else { "type a model" };
        let other = Choice { value: OTHER_MODEL.into(), label: OTHER_MODEL.into(), note: note.into() };
        std::iter::once(default).chain(known).chain([other]).collect()
    }

    pub(crate) fn model_pick(&self) -> Option<Known> {
        self.pick.as_ref().filter(|pick| pick.row == Row::Model)?;
        Known::of(&self.role()?.agent)
    }

    pub(crate) fn models_read(&mut self, kind: Known, agents: &Agents) {
        if self.model_pick() == Some(kind) {
            let items = self.model_choices(agents);
            if let Some(pick) = &mut self.pick {
                pick.items = items;
            }
        }
    }

    pub(crate) fn pick_choices(&self) -> Vec<&Choice> {
        let Some(pick) = &self.pick else { return Vec::new() };
        let needle = pick.search.query().trim().to_lowercase();
        pick.items.iter().filter(|item| item.label.to_lowercase().contains(&needle)).collect()
    }

    pub(crate) fn pick_marks(&self) -> Option<Vec<Option<bool>>> {
        let Some(Row::Roles(i)) = self.pick.as_ref().map(|pick| pick.row) else { return None };
        let stage = self.flow.stages.get(i)?;
        let mark =
            |item: &&Choice| self.stage_elsewhere(i, &item.value).is_none().then(|| stage.roles.contains(&item.value));
        Some(self.pick_choices().iter().map(mark).collect())
    }

    pub(crate) fn choose(&mut self, index: Option<usize>) {
        let Some(pick) = &self.pick else { return };
        let choices = self.pick_choices();
        let i = index.unwrap_or_else(|| pick.search.selected()).min(choices.len().saturating_sub(1));
        let Some(value) = choices.get(i).map(|item| item.value.clone()) else { return };
        let row = pick.row;
        if let Row::Roles(stage) = row {
            self.toggle_role(stage, &value);
            if let Some(pick) = &mut self.pick {
                pick.search.select(i);
            }
            return;
        }
        self.pick = None;
        self.apply_choice(row, value);
    }

    fn apply_choice(&mut self, row: Row, value: String) {
        match row {
            Row::Place => self.place = if value == "project" { Place::Project } else { Place::Yours },
            Row::Back(stage) => self.set_back(stage, &value),
            Row::End => self.flow.end = End::ALL.into_iter().find(|end| end.label() == value).unwrap_or_default(),
            Row::Agent => {
                if let Some(role) = self.role_mut()
                    && role.agent != value
                {
                    role.agent = value;
                    role.model = None;
                    role.effort = None;
                }
            }
            Row::Model if value == OTHER_MODEL => {
                let current = self.role().and_then(|role| role.model.clone()).unwrap_or_default();
                self.start_field(Row::Model, current);
            }
            Row::Model => {
                if let Some(role) = self.role_mut() {
                    role.model = (value != DEFAULT_MODEL).then_some(value);
                }
            }
            Row::Effort => {
                if let Some(role) = self.role_mut() {
                    role.effort = (value != DEFAULT_MODEL).then_some(value);
                }
            }
            Row::Worktree => {
                if let Some(role) = self.role_mut() {
                    role.worktree = Worktree::ALL.into_iter().find(|kind| kind.name() == value).unwrap_or_default();
                }
            }
            _ => {}
        }
    }

    fn toggle_role(&mut self, stage: usize, name: &str) {
        if let Some(other) = self.stage_elsewhere(stage, name) {
            self.notice = Some(format!("`{name}` works in stage {}, and a role works in one stage", other + 1));
            return;
        }
        let order: Vec<String> = self.flow.roles.iter().map(|role| role.name.clone()).collect();
        let Some(stage) = self.flow.stages.get_mut(stage) else { return };
        if stage.roles.iter().any(|role| role == name) {
            stage.roles.retain(|role| role != name);
        } else {
            stage.roles.push(name.to_string());
            stage.roles.sort_by_key(|role| order.iter().position(|o| o == role));
        }
    }

    fn set_back(&mut self, stage: usize, value: &str) {
        let Some(stage) = self.flow.stages.get_mut(stage) else { return };
        stage.back = value.parse::<usize>().ok().map(|to| Back { to, ..stage.back.clone().unwrap_or_default() });
    }

    pub(crate) fn buttons(&self, room: Room) -> Vec<&'static str> {
        if self.text.is_some() || self.field.is_some() {
            return vec![SAVE, CANCEL];
        }
        if let Some(pick) = &self.pick {
            return vec![if matches!(pick.row, Row::Roles(_)) { DONE } else { CANCEL }];
        }
        let mut buttons = Vec::new();
        if matches!(self.tab, Tab::Role(_)) {
            buttons.push(DELETE_ROLE);
        }
        if !room.wide {
            buttons.push(if self.diagram { FORM } else { DIAGRAM });
        }
        buttons.extend([SAVE, DONE]);
        buttons
    }

    pub(crate) fn submit(&self) -> &'static str {
        match &self.pick {
            Some(pick) if matches!(pick.row, Row::Roles(_)) => DONE,
            Some(_) => "",
            None => SAVE,
        }
    }

    pub(crate) fn hint(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        let hint = if self.text.is_some() {
            "enter starts a new line · save keeps the text · esc cancels"
        } else if self.field.is_some() {
            "enter keeps it · esc cancels"
        } else if self.pick.as_ref().is_some_and(|pick| matches!(pick.row, Row::Roles(_))) {
            "enter or a click checks a role · several work in parallel · esc goes back"
        } else if self.pick.is_some() {
            "enter picks · type to filter · esc goes back"
        } else if self.tab == Tab::Flow {
            "enter or a click changes a row or opens a stage · tab or ←→ switches tabs"
        } else {
            "enter or a click changes a row · tab or ←→ switches tabs"
        };
        hint.into()
    }
}
