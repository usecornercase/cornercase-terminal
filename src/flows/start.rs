use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Flow;
use super::dialog::{CANCEL, DIAGRAM, EDIT, FORM, agents_count, project_risks, scrolled};
use super::store::{self, Listing, Origin, Saved};
use crate::accounts::AgentAccounts;
use crate::issues::naming::check_branch;
use crate::list::COMBO_ROWS;
use crate::todo::editor::Editor;

pub(crate) const START: &str = "start";
const ACCOUNT_HINT: &str = "only this run: new tabs keep the one in use · enter opens the list";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    List,
    Prompt,
    Branch,
    Workspace,
    Account(usize),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Edit(Origin, String),
    Start {
        flow: Box<Flow>,
        task: String,
        branch: String,
        workspace: Option<String>,
        accounts: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Row(usize),
    Prompt { line: usize, col: usize },
    Branch,
    Workspace,
    Account(usize),
    Choose(usize),
    Close,
    Button(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) name: String,
    pub(crate) tag: Option<&'static str>,
    pub(crate) agents: String,
    pub(crate) line: String,
    pub(crate) red: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Room {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) wide: bool,
}

#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts: repository root, branch typed, diagram shown, run starting"
)]
pub(crate) struct Start {
    pub(crate) project: u64,
    pub(crate) project_name: String,
    pub(crate) repo: bool,
    pub(crate) issue: Option<String>,
    pub(crate) flows: Vec<Saved>,
    builtins: Vec<String>,
    pub(crate) cursor: usize,
    pub(crate) preview: u16,
    pub(crate) prompt: Editor,
    pub(crate) branch: String,
    pub(crate) workspace: String,
    named: bool,
    pub(crate) accounts: Vec<AgentAccounts>,
    pub(crate) focus: Focus,
    pub(crate) diagram: bool,
    pub(crate) notice: Option<String>,
    pub(crate) waiting: bool,
}

impl Start {
    pub(crate) fn new(project: u64, project_name: String, repo: bool, listing: Listing) -> Self {
        let mut start = Self {
            project,
            project_name,
            repo,
            issue: None,
            flows: listing.flows,
            builtins: listing.builtins,
            cursor: 0,
            preview: 0,
            prompt: Editor::default(),
            branch: String::new(),
            workspace: String::new(),
            named: false,
            accounts: Vec::new(),
            focus: Focus::Prompt,
            diagram: false,
            notice: None,
            waiting: false,
        };
        let startable = |saved: &&Saved| start.problems(saved).is_empty();
        let first_builtin = start.builtins.first().map(|name| name.to_lowercase());
        let named = |saved: &Saved| first_builtin.as_deref() == Some(saved.flow.name.trim().to_lowercase().as_str());
        start.cursor = start
            .flows
            .iter()
            .position(|saved| named(saved) && startable(&saved))
            .or_else(|| start.flows.iter().position(|saved| startable(&saved)))
            .unwrap_or(0);
        start
    }

    pub(crate) fn for_issue(mut self, issue: String, prompt: &str, branch: String) -> Self {
        self.workspace.clone_from(&issue);
        self.issue = Some(issue);
        self.prompt = Editor::new(prompt);
        self.prompt.home();
        self.branch = branch;
        self.named = true;
        self
    }

    pub(crate) fn with_accounts(mut self, accounts: Vec<AgentAccounts>) -> Self {
        self.accounts = accounts;
        self
    }

    pub(crate) fn seed_accounts(&mut self, chosen: &BTreeMap<String, String>) {
        for agent in &mut self.accounts {
            if let Some(name) = chosen.get(&agent.kind) {
                agent.choose_named(name);
            }
        }
    }

    pub(crate) fn shown_accounts(&self) -> Vec<usize> {
        let Some(saved) = self.selected() else { return Vec::new() };
        let flow = &saved.flow;
        let team = |kind: &str| std::iter::once(&flow.orchestrator).chain(&flow.roles).any(|role| role.agent == kind);
        (0..self.accounts.len()).filter(|i| team(&self.accounts[*i].kind)).collect()
    }

    fn chosen_accounts(&self) -> BTreeMap<String, String> {
        self.shown_accounts()
            .into_iter()
            .map(|i| (self.accounts[i].kind.clone(), self.accounts[i].chosen().to_string()))
            .collect()
    }

    fn open_account(&self) -> Option<usize> {
        self.accounts.iter().position(|agent| agent.combo.is_open())
    }

    pub(crate) fn set_listing(&mut self, listing: Listing, select: Option<&Origin>) {
        let selected = self.selected().map(|saved| saved.origin.clone());
        self.flows = listing.flows;
        self.builtins = listing.builtins;
        let find = |origin: &Origin| self.flows.iter().position(|saved| saved.origin == *origin && origin.editable());
        let cursor = select
            .and_then(find)
            .or_else(|| selected.as_ref().and_then(find))
            .unwrap_or_else(|| self.cursor.min(self.flows.len().saturating_sub(1)));
        self.select(cursor);
    }

    pub(crate) fn selected(&self) -> Option<&Saved> {
        self.flows.get(self.cursor)
    }

    fn problems(&self, saved: &Saved) -> Vec<String> {
        store::conflict(&self.flows, &self.builtins, saved).into_iter().chain(saved.flow.problems()).collect()
    }

    pub(crate) fn items(&self) -> Vec<Item> {
        let item = |saved: &Saved| {
            let problems = self.problems(saved);
            let line = problems.first().cloned().unwrap_or_else(|| saved.flow.description.trim().to_string());
            Item {
                name: saved.flow.name.trim().to_string(),
                tag: store::tag(&self.builtins, saved),
                agents: agents_count(&saved.flow),
                line,
                red: !problems.is_empty(),
            }
        };
        self.flows.iter().map(item).collect()
    }

    pub(crate) fn title(&self) -> String {
        match &self.issue {
            Some(issue) => format!("Start a flow › {issue}"),
            None => "Start a flow".into(),
        }
    }

    pub(crate) fn heading(&self) -> String {
        let name = self.selected().map_or("a flow", |saved| saved.flow.name.trim());
        format!("{name} in {}, in a worktree of its own", self.project_name)
    }

    pub(crate) fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.repo {
            out.push(format!(
                "{} is not the root of a git repository, and a flow works in a worktree of its own",
                self.project_name
            ));
        }
        if let Some(saved) = self.selected() {
            if matches!(saved.origin, Origin::Project(_)) {
                out.extend(project_risks(&saved.flow));
            }
            out.extend(self.problems(saved));
        }
        out
    }

    pub(crate) fn hint(&self) -> String {
        match self.focus {
            Focus::List => "↑↓ picks a flow · tab moves to the prompt",
            Focus::Prompt => "what the team should do · enter starts a new line · tab moves to the branch",
            Focus::Branch if self.issue.is_some() => {
                "the run's worktree and branch · tab moves to the workspace's name · enter starts the flow"
            }
            Focus::Branch => "the run's worktree and branch · enter starts the flow",
            Focus::Workspace => "the run's workspace in the sidebar · enter starts the flow",
            Focus::Account(i) => {
                let kind = self.accounts.get(i).map_or("", |agent| agent.kind.as_str());
                return format!("the {kind} account this team's {kind} agents start with · {ACCOUNT_HINT}");
            }
        }
        .into()
    }

    pub(crate) fn buttons(&self, room: Room) -> Vec<&'static str> {
        let mut buttons = Vec::new();
        if self.selected().is_some() {
            buttons.push(EDIT);
        }
        if !room.wide {
            buttons.push(if self.diagram { FORM } else { DIAGRAM });
        }
        buttons.extend([CANCEL, START]);
        buttons
    }

    pub(crate) fn press(&mut self, label: &str) -> Action {
        self.notice = None;
        match label {
            EDIT => self
                .selected()
                .map_or(Action::None, |saved| Action::Edit(saved.origin.clone(), saved.flow.name.clone())),
            CANCEL => Action::Close,
            START => self.start(),
            DIAGRAM | FORM => {
                self.diagram = !self.diagram;
                Action::None
            }
            _ => Action::None,
        }
    }

    pub(crate) fn start(&mut self) -> Action {
        let refused = match self.selected() {
            _ if !self.repo => Some("a flow needs a project that is the root of a git repository"),
            None => Some("there is no flow to start: make one, or bring a built-in back, in Settings › Flows"),
            Some(saved) if !self.problems(saved).is_empty() => Some("fix this flow's problems first: edit it"),
            Some(_) if self.prompt.text().trim().is_empty() => Some("say what the team should do in the prompt"),
            Some(_) if self.branch.trim().is_empty() => Some("the run needs a branch for its worktree"),
            Some(_) => None,
        };
        if let Some(why) = refused {
            self.notice = Some(why.into());
            return Action::None;
        }
        let branch = match check_branch(&self.branch) {
            Ok(branch) => branch,
            Err(why) => {
                self.notice = Some(why);
                return Action::None;
            }
        };
        let Some(saved) = self.selected() else { return Action::None };
        let workspace = self.issue.as_ref().map(|issue| {
            let name = self.workspace.trim();
            if name.is_empty() { issue.clone() } else { name.to_string() }
        });
        Action::Start {
            flow: Box::new(saved.flow.clone()),
            task: self.prompt.text().trim().to_string(),
            branch,
            workspace,
            accounts: self.chosen_accounts(),
        }
    }

    fn field(&mut self) -> &mut String {
        if self.focus == Focus::Workspace {
            &mut self.workspace
        } else {
            self.named = true;
            &mut self.branch
        }
    }

    fn typed(&mut self) {
        if !self.named {
            self.branch = super::branch(&self.prompt.text());
        }
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < self.flows.len() && i != self.cursor {
            self.cursor = i;
            self.preview = 0;
            self.close_accounts();
            if matches!(self.focus, Focus::Account(n) if !self.shown_accounts().contains(&n)) {
                self.focus = Focus::Branch;
            }
        }
    }

    pub(crate) fn scroll_preview(&mut self, delta: isize, most: u16) {
        self.preview = scrolled(self.preview, delta, most);
    }

    fn step_focus(&mut self, back: bool) {
        self.close_accounts();
        let order: Vec<Focus> = [Focus::List, Focus::Prompt, Focus::Branch]
            .into_iter()
            .chain(self.issue.is_some().then_some(Focus::Workspace))
            .chain(self.shown_accounts().into_iter().map(Focus::Account))
            .collect();
        let at = order.iter().position(|focus| *focus == self.focus).unwrap_or_default();
        let next = if back { at + order.len() - 1 } else { at + 1 };
        self.focus = order[next % order.len()];
    }

    pub(crate) fn accounts_open(&self) -> bool {
        self.open_account().is_some()
    }

    fn close_accounts(&mut self) {
        self.accounts.iter_mut().for_each(|agent| agent.combo.close());
    }

    pub(crate) fn scroll_accounts(&mut self, delta: isize, rows: usize) {
        if let Some(i) = self.open_account() {
            self.accounts[i].combo.scroll_by(delta, rows);
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> Action {
        if self.waiting {
            return Action::None;
        }
        self.notice = None;
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if let Focus::Account(i) = self.focus
            && !matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
            && let Some(agent) = self.accounts.get_mut(i)
            && (agent.combo.is_open() || key.code != KeyCode::Esc)
        {
            agent.combo.key(key, COMBO_ROWS);
            return Action::None;
        }
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Tab => self.step_focus(false),
            KeyCode::BackTab => self.step_focus(true),
            _ => return self.focused_key(key, typing, room),
        }
        Action::None
    }

    fn focused_key(&mut self, key: KeyEvent, typing: bool, room: Room) -> Action {
        match self.focus {
            Focus::List => match key.code {
                KeyCode::Up => self.select(self.cursor.saturating_sub(1)),
                KeyCode::Down => self.select(self.cursor + 1),
                KeyCode::Enter => self.focus = Focus::Prompt,
                _ => {}
            },
            Focus::Prompt => {
                if self.prompt.key(key, room.width, room.height) {
                    self.typed();
                }
            }
            Focus::Branch | Focus::Workspace => match key.code {
                KeyCode::Enter => return self.start(),
                KeyCode::Backspace => {
                    self.field().pop();
                }
                KeyCode::Char(c) if typing => self.field().push(c),
                _ => {}
            },
            Focus::Account(_) => {}
        }
        Action::None
    }

    pub(crate) fn paste(&mut self, text: &str, room: Room) {
        if self.waiting {
            return;
        }
        match self.focus {
            Focus::Account(_) => {}
            Focus::Prompt | Focus::List => {
                self.focus = Focus::Prompt;
                self.prompt.insert_lines(text);
                self.prompt.reveal(room.width, room.height);
                self.typed();
            }
            Focus::Branch | Focus::Workspace => self.field().extend(text.chars().filter(|c| !c.is_control())),
        }
    }

    pub(crate) fn hit(&mut self, hit: Hit, room: Room) -> Action {
        if self.waiting {
            return Action::None;
        }
        self.notice = None;
        let open = self.open_account();
        if !matches!(hit, Hit::Choose(_)) && !matches!(hit, Hit::Account(i) if Some(i) == open) {
            self.close_accounts();
        }
        match hit {
            Hit::Account(i) => self.toggle_account(i),
            Hit::Choose(i) => {
                if let Some(at) = open {
                    self.accounts[at].combo.choose(i);
                }
            }
            Hit::Workspace if self.issue.is_some() => self.focus = Focus::Workspace,
            Hit::Close | Hit::Workspace => {}
            Hit::Row(i) => {
                self.select(i);
                self.focus = Focus::List;
            }
            Hit::Prompt { line, col } => {
                self.focus = Focus::Prompt;
                let scroll = self.prompt.scroll();
                self.prompt.place(room.width, scroll + line, col);
            }
            Hit::Branch => self.focus = Focus::Branch,
            Hit::Button(label) => return self.press(label),
        }
        Action::None
    }

    fn toggle_account(&mut self, i: usize) {
        if let Some(agent) = self.accounts.get_mut(i) {
            self.focus = Focus::Account(i);
            if agent.combo.is_open() { agent.combo.close() } else { agent.combo.open() }
        }
    }

    pub(crate) fn failed(&mut self, why: &str) {
        self.waiting = false;
        self.notice = Some(format!("the flow did not start: {why}"));
    }

    pub(crate) fn scroll_prompt(&mut self, delta: isize, room: Room) {
        self.prompt.scroll_by(delta, room.width, room.height);
    }

    pub(crate) fn scroll_list(&mut self, delta: isize) {
        self.select(self.cursor.saturating_add_signed(delta).min(self.flows.len().saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests;
