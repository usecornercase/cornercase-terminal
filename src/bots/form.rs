use crossterm::event::{KeyCode, KeyEvent};

use super::Bot;
use crate::accounts::AgentAccounts;
use crate::control::bots::{AgentInfo, EditBot, NewBot};
use crate::editor::Editor;
use crate::list::Combo;

pub(crate) const DEFAULT: &str = "default";

pub(crate) const ACCESSES: [(&str, &str); 2] = [("cornercase", "through cornercase"), ("full", "full access")];

const NOT_AVAILABLE: &str = "not available";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Name,
    Agent,
    Model,
    Effort,
    Account,
    Access,
    Follow,
    Secrets,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    None,
    Submit,
    Cancel,
    Chose(Field),
    Manage,
}

#[derive(Debug, Clone)]
pub(crate) struct Form {
    pub(crate) editing: Option<String>,
    loaded_about: String,
    pub(crate) agents: Vec<AgentInfo>,
    pub(crate) name: Editor,
    pub(crate) agent: Combo,
    pub(crate) model: Combo,
    pub(crate) effort: Combo,
    pub(crate) account: Option<AgentAccounts>,
    pub(crate) access: Combo,
    pub(crate) follow: bool,
    pub(crate) about: Editor,
    pub(crate) focus: Field,
    pub(crate) error: Option<String>,
    pub(crate) secrets: String,
}

fn agent_combo(agents: &[AgentInfo], chosen: usize) -> Combo {
    let titles = agents.iter().map(|info| info.title.clone()).collect();
    let notes = agents
        .iter()
        .map(|info| (!info.bot).then(|| info.why.clone().unwrap_or_else(|| NOT_AVAILABLE.to_string())))
        .collect();
    Combo::with_notes(titles, Some(chosen), notes)
}

fn with_default(levels: &[String], chosen: Option<&str>) -> Vec<String> {
    let mut options = vec![DEFAULT.to_string()];
    options.extend(levels.iter().cloned());
    if let Some(own) = chosen.filter(|own| !options.iter().any(|option| option == own)) {
        options.push(own.to_string());
    }
    options
}

fn position(options: &[String], chosen: Option<&str>) -> usize {
    chosen.and_then(|own| options.iter().position(|option| option == own)).unwrap_or(0)
}

impl Form {
    pub(crate) fn new(agents: Vec<AgentInfo>, preferred: &str) -> Self {
        let first = agents.iter().position(|info| info.bot && info.agent == preferred);
        let chosen = first.or_else(|| agents.iter().position(|info| info.bot)).unwrap_or(0);
        let mut form = Self {
            editing: None,
            loaded_about: String::new(),
            agent: agent_combo(&agents, chosen),
            agents,
            name: Editor::default(),
            model: Combo::default(),
            effort: Combo::default(),
            account: None,
            access: Combo::default(),
            follow: true,
            about: Editor::default(),
            focus: Field::Name,
            error: None,
            secrets: String::new(),
        };
        form.refit_agent();
        form
    }

    pub(crate) fn editing(agents: Vec<AgentInfo>, bot: &Bot, about: &str) -> Self {
        let chosen = agents.iter().position(|info| info.agent == bot.agent).unwrap_or(0);
        let mut form = Self {
            editing: Some(bot.id.clone()),
            loaded_about: about.to_string(),
            agent: agent_combo(&agents, chosen),
            agents,
            name: Editor::new(&bot.name),
            model: Combo::default(),
            effort: Combo::default(),
            account: None,
            access: Combo::default(),
            follow: bot.follow,
            about: Editor::new(about),
            focus: Field::Name,
            error: None,
            secrets: String::new(),
        };
        form.fit((bot.model.as_deref(), bot.effort.as_deref()), Some(bot.access.name()));
        form
    }

    pub(crate) fn info(&self) -> Option<&AgentInfo> {
        self.agent.chosen().and_then(|at| self.agents.get(at))
    }

    fn levels(&self) -> Vec<String> {
        self.info().map(|info| info.efforts.clone()).unwrap_or_default()
    }

    fn models(&self, own: Option<&str>) -> Vec<String> {
        with_default(&self.info().map(|info| info.models.clone()).unwrap_or_default(), own)
    }

    fn choose_access(&mut self, name: Option<&str>) {
        let offered: Vec<&str> =
            self.info().map(|info| info.accesses.iter().map(String::as_str).collect()).unwrap_or_default();
        let why = self.info().and_then(|info| info.why.clone()).unwrap_or_else(|| NOT_AVAILABLE.to_string());
        let labels = ACCESSES.iter().map(|(_, label)| (*label).to_string()).collect();
        let notes = ACCESSES.iter().map(|(id, _)| (!offered.contains(id)).then(|| why.clone())).collect();
        let chosen = name.and_then(|name| ACCESSES.iter().position(|(id, _)| *id == name));
        self.access = Combo::with_notes(labels, chosen, notes);
    }

    fn fit(&mut self, (model, effort): (Option<&str>, Option<&str>), access: Option<&str>) {
        let models = self.models(model);
        self.model = Combo::typed(models.clone(), Some(position(&models, model)));
        let levels = with_default(&self.levels(), effort);
        self.effort = Combo::new(levels.clone(), Some(position(&levels, effort)));
        self.choose_access(access);
    }

    pub(crate) fn refit_agent(&mut self) {
        let offered = self.info().is_some_and(|info| info.accesses.iter().any(|access| access == "cornercase"));
        self.fit((None, None), offered.then_some("cornercase"));
    }

    pub(crate) fn reads_only_folder(&self) -> bool {
        self.info().is_some_and(|info| info.reads.as_deref() == Some("folder"))
    }

    pub(crate) fn takes_effort(&self) -> bool {
        self.info().is_some_and(|info| !info.efforts.is_empty())
    }

    pub(crate) fn fields(&self) -> Vec<Field> {
        let mut fields = vec![Field::Name, Field::Agent, Field::Model];
        if self.takes_effort() {
            fields.push(Field::Effort);
        }
        if self.account.is_some() {
            fields.push(Field::Account);
        }
        fields.extend([Field::Access, Field::Follow]);
        fields.extend(self.editing.is_some().then_some(Field::Secrets));
        fields.push(Field::About);
        fields
    }

    fn focusable(&self) -> Vec<Field> {
        self.fields().into_iter().filter(|field| self.editing.is_none() || *field != Field::Agent).collect()
    }

    pub(crate) fn editor_of(&self, field: Field) -> Option<&Editor> {
        match field {
            Field::Name => Some(&self.name),
            Field::About => Some(&self.about),
            _ => None,
        }
    }

    pub(crate) fn editor(&mut self, field: Field) -> Option<&mut Editor> {
        match field {
            Field::Name => Some(&mut self.name),
            Field::About => Some(&mut self.about),
            _ => None,
        }
    }

    pub(crate) fn combo_of(&self, field: Field) -> Option<&Combo> {
        match field {
            Field::Agent => Some(&self.agent),
            Field::Model => Some(&self.model),
            Field::Effort => Some(&self.effort),
            Field::Account => self.account.as_ref().map(|account| &account.combo),
            Field::Access => Some(&self.access),
            Field::Name | Field::About | Field::Follow | Field::Secrets => None,
        }
    }

    pub(crate) fn combo(&mut self, field: Field) -> Option<&mut Combo> {
        match field {
            Field::Agent => Some(&mut self.agent),
            Field::Model => Some(&mut self.model),
            Field::Effort => Some(&mut self.effort),
            Field::Account => self.account.as_mut().map(|account| &mut account.combo),
            Field::Access => Some(&mut self.access),
            Field::Name | Field::About | Field::Follow | Field::Secrets => None,
        }
    }

    pub(crate) fn opened(&self) -> Option<Field> {
        self.fields().into_iter().find(|field| self.combo_of(*field).is_some_and(Combo::is_open))
    }

    pub(crate) fn close_lists(&mut self) {
        for field in [Field::Agent, Field::Model, Field::Effort, Field::Account, Field::Access] {
            self.combo(field).into_iter().for_each(Combo::close);
        }
    }

    pub(crate) fn focus_on(&mut self, field: Field) {
        self.close_lists();
        if self.editing.is_none() || field != Field::Agent {
            self.focus = field;
        }
    }

    fn step_focus(&mut self, forward: bool) {
        let fields = self.focusable();
        let at = fields.iter().position(|field| *field == self.focus).unwrap_or_default();
        let next = (if forward { at + 1 } else { at + fields.len() - 1 }) % fields.len();
        self.focus_on(fields[next]);
    }

    pub(crate) fn chose(&mut self, field: Field) -> Step {
        if field == Field::Agent {
            self.refit_agent();
        }
        Step::Chose(field)
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize, rows: usize) -> Step {
        self.error = None;
        if let Some(open) = self.opened().filter(|_| !matches!(key.code, KeyCode::Tab | KeyCode::BackTab)) {
            let changed = self.combo(open).is_some_and(|combo| combo.key(key, rows));
            return if changed { self.chose(open) } else { Step::None };
        }
        match key.code {
            KeyCode::Esc => return Step::Cancel,
            KeyCode::Tab => self.step_focus(true),
            KeyCode::BackTab => self.step_focus(false),
            KeyCode::Char(' ') if self.focus == Field::Follow => self.follow = !self.follow,
            KeyCode::Enter if self.focus == Field::Secrets => return Step::Manage,
            KeyCode::Enter if self.focus != Field::About => return Step::Submit,
            _ => self.typed(key, width, rows),
        }
        Step::None
    }

    fn typed(&mut self, key: KeyEvent, width: usize, rows: usize) {
        let focus = self.focus;
        if let Some(combo) = self.combo(focus) {
            combo.key(key, rows);
        } else if focus == Field::About {
            self.about.key(key, width, rows);
        } else if let Some(editor) = self.editor(focus) {
            editor.one_line_key(key, width);
        }
    }

    fn chosen_label(combo: &Combo) -> Option<String> {
        combo.label().filter(|label| *label != DEFAULT).map(str::to_string)
    }

    fn chosen_access(&self) -> Option<&'static str> {
        self.access.chosen().and_then(|at| ACCESSES.get(at)).map(|(id, _)| *id)
    }

    fn account_choice(&self) -> Option<String> {
        self.account.as_ref().map(|account| account.chosen().to_string())
    }

    pub(crate) fn new_bot(&self) -> Result<NewBot, String> {
        let info = self.info().ok_or("choose an agent")?;
        if !info.bot {
            let why = info.why.clone().unwrap_or_else(|| NOT_AVAILABLE.to_string());
            return Err(format!("{} cannot be a bot: {why}", info.title));
        }
        let access = self.chosen_access().ok_or("choose its access")?;
        Ok(NewBot {
            name: self.name.text(),
            agent: Some(info.agent.clone()),
            model: Self::chosen_label(&self.model),
            effort: Self::chosen_label(&self.effort).filter(|_| self.takes_effort()),
            account: self.account_choice().filter(|account| !account.is_empty()),
            access: Some(access.to_string()),
            follow: Some(self.follow),
            about: self.about.text(),
        })
    }

    pub(crate) fn edit_bot(&self) -> Result<EditBot, String> {
        let access = self.chosen_access().ok_or("choose its access")?;
        let about = self.about.text();
        Ok(EditBot {
            bot: self.editing.clone().unwrap_or_default(),
            name: Some(self.name.text()),
            agent: None,
            model: Some(Self::chosen_label(&self.model).unwrap_or_default()),
            effort: self.takes_effort().then(|| Self::chosen_label(&self.effort).unwrap_or_default()),
            account: self.account_choice(),
            access: Some(access.to_string()),
            follow: Some(self.follow),
            about: (about != self.loaded_about).then_some(about),
        })
    }
}

#[cfg(test)]
mod tests;
