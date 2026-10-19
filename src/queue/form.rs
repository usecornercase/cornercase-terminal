use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Place;
use crate::accounts::AgentAccounts;
use crate::list::Combo;
use crate::todo::editor::Editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    Prompt,
    When,
    Clock,
    Date,
    Amount,
    Unit,
    Place,
    OpenIn,
    Branch,
    Agent,
    Account,
    Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum When {
    At,
    Tomorrow,
    On,
    In,
    ClaudeReset,
    CodexReset,
}

impl When {
    pub(crate) const ALL: [Self; 6] =
        [Self::At, Self::Tomorrow, Self::On, Self::In, Self::ClaudeReset, Self::CodexReset];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::At => "at",
            Self::Tomorrow => "tomorrow at",
            Self::On => "on",
            Self::In => "in",
            Self::ClaudeReset => "Claude reset",
            Self::CodexReset => "Codex reset",
        }
    }

    pub(crate) fn combo(self) -> Combo {
        Combo::new(
            Self::ALL.iter().map(|when| when.label().to_string()).collect(),
            Self::ALL.iter().position(|w| *w == self),
        )
    }
}

pub(crate) const UNITS: [(&str, char); 3] = [("minutes", 'm'), ("hours", 'h'), ("days", 'd')];

pub(crate) const OPEN_IN: [&str; 2] = ["new tab", "new worktree"];

#[derive(Debug, Clone)]
pub(crate) struct Choice {
    pub(crate) label: String,
    pub(crate) place: Place,
    pub(crate) worktree: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    None,
    Submit,
    Cancel,
    Chose(Control),
}

#[derive(Debug, Clone)]
pub(crate) struct Form {
    pub(crate) id: Option<u64>,
    pub(crate) label: Option<String>,
    pub(crate) prompt: Editor,
    pub(crate) when: Combo,
    pub(crate) clock: Editor,
    pub(crate) date: Editor,
    pub(crate) amount: Editor,
    pub(crate) unit: Combo,
    pub(crate) places: Vec<Choice>,
    pub(crate) place: Combo,
    pub(crate) open_in: Combo,
    pub(crate) branch: Editor,
    pub(crate) agent: Combo,
    pub(crate) account: Option<AgentAccounts>,
    pub(crate) mode: Combo,
    pub(crate) focus: Control,
    pub(crate) zone: String,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
}

impl Form {
    pub(crate) fn new(
        places: Vec<Choice>,
        place: Option<usize>,
        kinds: Vec<String>,
        agent: usize,
        zone: String,
        today: &str,
    ) -> Self {
        Self {
            id: None,
            label: None,
            prompt: Editor::default(),
            when: When::In.combo(),
            clock: Editor::new("09:00"),
            date: Editor::new(today),
            amount: Editor::new("2"),
            unit: Combo::new(UNITS.iter().map(|(name, _)| (*name).to_string()).collect(), Some(1)),
            place: Combo::new(places.iter().map(|choice| choice.label.clone()).collect(), place),
            places,
            open_in: Combo::new(OPEN_IN.map(String::from).into(), Some(0)),
            branch: Editor::default(),
            agent: Combo::new(kinds, Some(agent)),
            account: None,
            mode: Combo::new(vec!["default".into()], Some(0)),
            focus: Control::Prompt,
            zone,
            busy: false,
            error: None,
        }
    }

    pub(crate) fn chosen_account(&self) -> Option<(&str, &str)> {
        self.account.as_ref().map(|account| (account.kind.as_str(), account.chosen()))
    }

    pub(crate) fn kind(&self) -> When {
        self.when.chosen().and_then(|i| When::ALL.get(i)).copied().unwrap_or(When::In)
    }

    pub(crate) fn choice(&self) -> Option<&Choice> {
        self.place.chosen().and_then(|i| self.places.get(i))
    }

    pub(crate) fn can_branch(&self) -> bool {
        self.choice().is_some_and(|choice| choice.worktree)
    }

    pub(crate) fn worktree(&self) -> bool {
        self.can_branch() && self.open_in.chosen() == Some(1)
    }

    pub(crate) fn at(&self) -> String {
        let clock = self.clock.text().trim().to_string();
        match self.kind() {
            When::At => clock,
            When::Tomorrow => format!("tomorrow {clock}"),
            When::On => format!("{} {clock}", self.date.text().trim()),
            When::In => {
                let unit = self.unit.chosen().and_then(|i| UNITS.get(i)).map_or('h', |(_, unit)| *unit);
                format!("in {}{unit}", self.amount.text().trim())
            }
            When::ClaudeReset => "claude reset".into(),
            When::CodexReset => "codex reset".into(),
        }
    }

    pub(crate) fn controls(&self) -> Vec<Control> {
        let mut controls = vec![Control::Prompt, Control::When];
        controls.extend(match self.kind() {
            When::At | When::Tomorrow => &[Control::Clock][..],
            When::On => &[Control::Date, Control::Clock],
            When::In => &[Control::Amount, Control::Unit],
            When::ClaudeReset | When::CodexReset => &[],
        });
        controls.push(Control::Place);
        if self.can_branch() {
            controls.push(Control::OpenIn);
        }
        if self.worktree() {
            controls.push(Control::Branch);
        }
        controls.push(Control::Agent);
        if self.account.is_some() {
            controls.push(Control::Account);
        }
        controls.push(Control::Mode);
        controls
    }

    pub(crate) fn editor_of(&self, control: Control) -> Option<&Editor> {
        match control {
            Control::Prompt => Some(&self.prompt),
            Control::Clock => Some(&self.clock),
            Control::Date => Some(&self.date),
            Control::Amount => Some(&self.amount),
            Control::Branch => Some(&self.branch),
            _ => None,
        }
    }

    pub(crate) fn editor(&mut self, control: Control) -> Option<&mut Editor> {
        match control {
            Control::Prompt => Some(&mut self.prompt),
            Control::Clock => Some(&mut self.clock),
            Control::Date => Some(&mut self.date),
            Control::Amount => Some(&mut self.amount),
            Control::Branch => Some(&mut self.branch),
            _ => None,
        }
    }

    pub(crate) fn combo_of(&self, control: Control) -> Option<&Combo> {
        match control {
            Control::When => Some(&self.when),
            Control::Unit => Some(&self.unit),
            Control::Place => Some(&self.place),
            Control::OpenIn => Some(&self.open_in),
            Control::Agent => Some(&self.agent),
            Control::Account => self.account.as_ref().map(|account| &account.combo),
            Control::Mode => Some(&self.mode),
            _ => None,
        }
    }

    pub(crate) fn combo(&mut self, control: Control) -> Option<&mut Combo> {
        match control {
            Control::When => Some(&mut self.when),
            Control::Unit => Some(&mut self.unit),
            Control::Place => Some(&mut self.place),
            Control::OpenIn => Some(&mut self.open_in),
            Control::Agent => Some(&mut self.agent),
            Control::Account => self.account.as_mut().map(|account| &mut account.combo),
            Control::Mode => Some(&mut self.mode),
            _ => None,
        }
    }

    pub(crate) fn opened(&self) -> Option<Control> {
        self.controls().into_iter().find(|control| self.combo_of(*control).is_some_and(Combo::is_open))
    }

    pub(crate) fn close_lists(&mut self) {
        for control in [
            Control::When,
            Control::Unit,
            Control::Place,
            Control::OpenIn,
            Control::Agent,
            Control::Account,
            Control::Mode,
        ] {
            self.combo(control).into_iter().for_each(Combo::close);
        }
    }

    pub(crate) fn focus_on(&mut self, control: Control) {
        self.close_lists();
        self.focus = control;
    }

    fn step_focus(&mut self, forward: bool) {
        let controls = self.controls();
        let at = controls.iter().position(|control| *control == self.focus).unwrap_or_default();
        let next = (if forward { at + 1 } else { at + controls.len() - 1 }) % controls.len();
        self.focus_on(controls[next]);
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
            KeyCode::Enter => return Step::Submit,
            _ => {
                let focus = self.focus;
                if let Some(combo) = self.combo(focus) {
                    combo.key(key, rows);
                } else if let Some(editor) = self.editor(focus) {
                    edit(editor, key, width);
                }
            }
        }
        Step::None
    }

    pub(crate) fn chose(&mut self, control: Control) -> Step {
        if !self.controls().contains(&self.focus) {
            self.focus = control;
        }
        Step::Chose(control)
    }
}

fn edit(editor: &mut Editor, key: KeyEvent, width: usize) {
    match key.code {
        KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            editor.insert(&c.to_string());
        }
        KeyCode::Backspace => editor.backspace(),
        KeyCode::Delete => editor.delete(),
        KeyCode::Left => editor.left(),
        KeyCode::Right => editor.right(),
        KeyCode::Up => editor.up(width),
        KeyCode::Down => editor.down(width),
        KeyCode::Home => editor.home(),
        KeyCode::End => editor.end(),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
