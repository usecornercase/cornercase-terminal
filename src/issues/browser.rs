use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{
    self as issues, Account, Detail, Issue, People, Person, Query, Secret, Source, Who, branch, jira, plane,
    sort_by_updated,
};
use crate::list::Search;
use crate::markdown;
use crate::time;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuesHit {
    Tab(usize),
    Toggle(usize),
    Item(usize),
    Button(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    pub list_rows: usize,
    pub detail_rows: usize,
    pub detail_width: usize,
}

const WHEEL_ROWS: isize = 3;
const PEOPLE_LABEL_MAX: usize = 40;
const TOKEN_DOTS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    All,
    One(Source),
}

impl Tab {
    fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::One(source) => source.name(),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::One(source) => source.id(),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        std::iter::once(Self::All).chain(Source::ALL.map(Self::One)).find(|t| t.id() == id)
    }
}

pub fn tabs(names: &[String]) -> Vec<Tab> {
    let all = std::iter::once(Tab::All).chain(Source::ALL.map(Tab::One));
    let mut tabs: Vec<Tab> = Vec::new();
    for name in names {
        if let Some(tab) = all.clone().find(|t| t.id().eq_ignore_ascii_case(name.trim()))
            && !tabs.contains(&tab)
        {
            tabs.push(tab);
        }
    }
    if tabs.is_empty() { all.collect() } else { tabs }
}

#[derive(Debug, Default)]
pub struct Listing {
    pub issues: Vec<Issue>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub from_env: bool,
    pub account: Option<Account>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Step {
    Site,
    Email,
    Workspace,
    #[default]
    Token,
}

#[derive(Debug, Default)]
pub struct TokenForm {
    pub input: String,
    pub checking: bool,
    pub error: Option<String>,
    pub step: Step,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStart {
    pub command: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Agents {
    pub kinds: Vec<String>,
    pub default: Option<String>,
    pub running: Option<String>,
    pub chosen: Option<String>,
    pub starts: HashMap<String, AgentStart>,
    pub prompt: String,
    pub submit: bool,
}

impl Agents {
    pub fn pick(&self) -> Option<String> {
        self.chosen.clone().or_else(|| self.default.clone()).or_else(|| self.running.clone())
    }

    fn notes(&self, kind: &str) -> String {
        let default = self.default.as_deref() == Some(kind);
        let running = self.running.as_deref() == Some(kind);
        [default.then_some("default"), running.then_some("running in your tab")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub project: u64,
    pub workspace: Option<u64>,
    pub label: String,
    pub worktree: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Agent,
    Place,
    Person(Source, usize),
}

#[derive(Debug)]
pub struct Picker {
    pub pick: Pick,
    pub search: Search,
    then_start: bool,
}

pub struct Choice {
    pub key: String,
    pub title: String,
    pub meta: String,
    pub who: Option<Who>,
}

#[derive(Debug)]
pub enum Screen {
    List,
    Detail { issue: Box<Issue>, detail: Option<Result<Detail, String>>, scroll: usize, raw: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    Load(Vec<Source>),
    Read(Issue),
    Start(Issue, String, Option<Place>),
    CheckToken(Source, Secret),
    SaveJira { site: String, email: String },
    SavePlane { workspace: String },
    Disconnect(Source),
    Copy(String),
    SetDefaultAgent(String),
    LoadPeople(Vec<Source>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Start,
    Agent,
    Where,
    Done,
    Clear,
    MakeDefault,
    Choose,
    Refresh,
    Raw,
    Rendered,
    Copy,
    Disconnect,
    Connect,
    Next,
    Cancel,
    Back,
}

impl Button {
    fn label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Agent => "agent…",
            Self::Where => "where…",
            Self::Done => "done",
            Self::Clear => "clear",
            Self::MakeDefault => "make default",
            Self::Choose => "choose",
            Self::Refresh => "refresh",
            Self::Raw => "raw",
            Self::Rendered => "rendered",
            Self::Copy => "copy url",
            Self::Disconnect => "disconnect",
            Self::Connect => "connect",
            Self::Next => "next",
            Self::Cancel => "cancel",
            Self::Back => "back",
        }
    }
}

#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts: git checkout, repo root, closed toggle, starting"
)]
pub struct Browser {
    pub project: u64,
    pub project_name: String,
    pub github: bool,
    pub worktrees: bool,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub closed: bool,
    pub people: People,
    pub search: Search,
    pub lists: HashMap<Source, Listing>,
    pub connections: HashMap<Source, Connection>,
    pub forms: HashMap<Source, TokenForm>,
    pub jira_site: String,
    pub jira_email: String,
    pub plane_workspace: String,
    pub screen: Screen,
    pub starting: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub secrets_path: String,
    pub agents: Agents,
    pub picker: Option<Picker>,
    pub places: Vec<Place>,
    pub here: usize,
    pub place: Option<usize>,
    pub filtering: Option<usize>,
    pub members: HashMap<Source, Result<Vec<Person>, String>>,
}

impl Browser {
    pub fn current(&self) -> Tab {
        self.tabs.get(self.tab).copied().unwrap_or(Tab::All)
    }

    pub fn tab_to(&mut self, tab: Tab) {
        if let Some(i) = self.tabs.iter().position(|t| *t == tab) {
            self.tab = i;
        }
    }

    pub fn token_field(&self, source: Source) -> (&'static str, String) {
        let form = self.forms.get(&source);
        let step = self.form_step(source);
        let typed = form.map_or_else(
            || match step {
                Step::Site => self.jira_site.clone(),
                Step::Workspace => self.plane_workspace.clone(),
                _ => String::new(),
            },
            |f| f.input.clone(),
        );
        match step {
            Step::Site => ("site", typed),
            Step::Email => ("email", typed),
            Step::Workspace => ("workspace", typed),
            Step::Token => (source.token_name(), "•".repeat(typed.chars().count().min(TOKEN_DOTS))),
        }
    }

    pub fn token_form(&self) -> Option<Source> {
        match self.current() {
            Tab::One(source) if source != Source::Github && !self.connections.contains_key(&source) => Some(source),
            _ => None,
        }
    }

    fn can_list(&self, source: Source) -> bool {
        if source == Source::Github { self.github } else { self.connections.contains_key(&source) }
    }

    pub fn sources(&self) -> Vec<Source> {
        let wanted: Vec<Source> = match self.current() {
            Tab::All => Source::ALL.to_vec(),
            Tab::One(source) => vec![source],
        };
        wanted.into_iter().filter(|s| self.can_list(*s)).collect()
    }

    pub fn needs_load(&self) -> Action {
        let missing: Vec<Source> = self.sources().into_iter().filter(|s| !self.lists.contains_key(s)).collect();
        if missing.is_empty() { Action::None } else { Action::Load(missing) }
    }

    pub fn query(&self, source: Source) -> Query {
        Query { closed: self.closed, people: self.people.of(source).clone() }
    }

    pub fn loaded(&mut self, source: Source, query: &Query, result: Result<Vec<Issue>, String>) {
        if *query != self.query(source) {
            return;
        }
        let listing = self.lists.entry(source).or_default();
        listing.loading = false;
        match result {
            Ok(issues) => {
                listing.issues = issues;
                listing.error = None;
            }
            Err(e) => listing.error = Some(e),
        }
    }

    pub fn connected(&mut self, source: Source, connection: Connection) {
        self.forms.remove(&source);
        self.connections.insert(source, connection);
    }

    fn form(&mut self, source: Source) -> &mut TokenForm {
        let (site, workspace) = (self.jira_site.clone(), self.plane_workspace.clone());
        self.forms.entry(source).or_insert_with(|| match source {
            Source::Jira => TokenForm { input: site, step: Step::Site, ..TokenForm::default() },
            Source::Plane => TokenForm { input: workspace, step: Step::Workspace, ..TokenForm::default() },
            _ => TokenForm::default(),
        })
    }

    pub fn form_step(&self, source: Source) -> Step {
        match (self.forms.get(&source), source) {
            (Some(form), _) => form.step,
            (None, Source::Jira) => Step::Site,
            (None, Source::Plane) => Step::Workspace,
            (None, _) => Step::Token,
        }
    }

    pub fn token_rejected(&mut self, source: Source, error: String) {
        let form = self.form(source);
        form.checking = false;
        form.error = Some(error);
    }

    pub fn disconnected(&mut self, source: Source) {
        self.connections.remove(&source);
        self.lists.remove(&source);
    }

    pub fn read_done(&mut self, source: Source, key: &str, result: Result<Detail, String>) {
        if let Screen::Detail { issue, detail, .. } = &mut self.screen
            && issue.source == source
            && issue.key == key
        {
            *detail = Some(result);
        }
    }

    pub fn shown(&self) -> Vec<&Issue> {
        let mut issues: Vec<&Issue> =
            self.sources().iter().filter_map(|s| self.lists.get(s)).flat_map(|l| &l.issues).collect();
        if self.current() == Tab::All {
            sort_by_updated(&mut issues);
        }
        super::filter(issues, self.search.query())
    }

    fn selected(&self) -> Option<Issue> {
        let shown = self.shown();
        let last = shown.len().checked_sub(1)?;
        shown.get(self.search.selected().min(last)).map(|issue| (*issue).clone())
    }

    fn buttons(&self) -> Vec<Button> {
        if self.picker.is_some() {
            return vec![Button::Choose, Button::Back];
        }
        if self.filtering.is_some() {
            return vec![Button::Done, Button::Clear];
        }
        match &self.screen {
            Screen::Detail { raw, issue, .. } => {
                let mut buttons = vec![Button::Start, Button::Agent];
                if issue.source != Source::Github && self.places.len() > 1 {
                    buttons.push(Button::Where);
                }
                if self.agents.chosen.is_some() && self.agents.chosen != self.agents.default {
                    buttons.push(Button::MakeDefault);
                }
                buttons.extend([if *raw { Button::Rendered } else { Button::Raw }, Button::Copy, Button::Back]);
                buttons
            }
            Screen::List => match self.token_form().map(|source| self.form_step(source)) {
                Some(Step::Site | Step::Workspace) => vec![Button::Next, Button::Cancel],
                Some(Step::Email) => vec![Button::Next, Button::Back, Button::Cancel],
                Some(Step::Token) if matches!(self.token_form(), Some(Source::Jira | Source::Plane)) => {
                    vec![Button::Connect, Button::Back, Button::Cancel]
                }
                Some(Step::Token) => vec![Button::Connect, Button::Cancel],
                None => self.list_buttons(),
            },
        }
    }

    fn list_buttons(&self) -> Vec<Button> {
        let mut buttons = vec![Button::Start, Button::Refresh];
        if let Tab::One(source) = self.current()
            && self.connections.get(&source).is_some_and(|c| !c.from_env)
        {
            buttons.push(Button::Disconnect);
        }
        buttons.push(Button::Cancel);
        buttons
    }

    pub fn labels(&self) -> Vec<&'static str> {
        self.buttons().into_iter().map(Button::label).collect()
    }

    pub fn tab_names(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name()).collect()
    }

    fn switch(&mut self, delta: isize) -> Action {
        let count = self.tabs.len().max(1);
        let delta = delta.rem_euclid(isize::try_from(count).unwrap_or(1)).unsigned_abs();
        self.tab = (self.tab + delta) % count;
        self.search.reset();
        self.needs_load()
    }

    fn toggle(&mut self, i: usize) -> Action {
        if i == 0 {
            self.closed = !self.closed;
            self.lists.clear();
            self.search.reset();
            return self.needs_load();
        }
        self.open_people()
    }

    fn people_sources(&self) -> Vec<Source> {
        self.sources()
    }

    pub fn people_rows(&self) -> Vec<(Source, usize)> {
        self.people_sources().into_iter().flat_map(|s| [(s, 0), (s, 1)]).collect()
    }

    fn open_people(&mut self) -> Action {
        let sources = self.people_sources();
        if sources.is_empty() {
            self.notice = Some("nothing to filter here".into());
            return Action::None;
        }
        self.filtering = Some(0);
        let missing: Vec<Source> = sources.into_iter().filter(|s| !self.members.contains_key(s)).collect();
        if missing.is_empty() { Action::None } else { Action::LoadPeople(missing) }
    }

    pub fn people_loaded(&mut self, source: Source, result: Result<Vec<Person>, String>) {
        self.members.insert(source, result);
    }

    fn set_person(&mut self, source: Source, field: usize, who: Who) {
        let people = self.people.of_mut(source);
        if people[field] != who {
            people[field] = who;
            self.lists.remove(&source);
            self.search.reset();
        }
    }

    fn close_people(&mut self) -> Action {
        self.filtering = None;
        self.needs_load()
    }

    fn people_label(&self) -> String {
        let parts: Vec<String> = self
            .people_sources()
            .into_iter()
            .filter_map(|s| {
                let text = self.people.describe(s);
                (!text.is_empty())
                    .then(|| if self.current() == Tab::All { format!("{} {text}", s.name()) } else { text })
            })
            .collect();
        if parts.is_empty() {
            "people".into()
        } else {
            crate::text::truncate_right(&format!("people: {}", parts.join(", ")), PEOPLE_LABEL_MAX)
        }
    }

    pub fn toggles(&self) -> (Vec<String>, Vec<bool>) {
        let label = self.people_label();
        let filtered = label != "people";
        (vec!["closed".into(), label], vec![self.closed, filtered])
    }

    fn read(&mut self, issue: Issue) -> Action {
        self.place = None;
        self.screen = Screen::Detail { issue: Box::new(issue.clone()), detail: None, scroll: 0, raw: false };
        Action::Read(issue)
    }

    fn press(&mut self, button: Button) -> Action {
        match button {
            Button::Start => match &self.screen {
                Screen::Detail { issue, .. } => {
                    let issue = (**issue).clone();
                    self.start(issue)
                }
                Screen::List => match self.selected() {
                    Some(issue) if self.ready_to_start(&issue) => self.start(issue),
                    Some(issue) => {
                        let read = self.read(issue.clone());
                        let _ = self.start(issue);
                        read
                    }
                    None => Action::None,
                },
            },
            Button::Agent => {
                self.open_picker(Pick::Agent, false);
                Action::None
            }
            Button::Where => {
                self.open_picker(Pick::Place, false);
                Action::None
            }
            Button::Done => self.close_people(),
            Button::Clear => {
                for source in self.people_sources() {
                    for field in 0..2 {
                        self.set_person(source, field, Who::Anyone);
                    }
                }
                Action::None
            }
            Button::MakeDefault => self.agents.chosen.clone().map_or(Action::None, Action::SetDefaultAgent),
            Button::Choose => self.choose(None),
            Button::Refresh => {
                let sources = self.sources();
                if sources.is_empty() { Action::None } else { Action::Load(sources) }
            }
            Button::Raw | Button::Rendered => {
                if let Screen::Detail { raw, scroll, .. } = &mut self.screen {
                    *raw = !*raw;
                    *scroll = 0;
                }
                Action::None
            }
            Button::Copy => match &self.screen {
                Screen::Detail { issue, .. } if !issue.url.is_empty() => Action::Copy(issue.url.clone()),
                _ => Action::None,
            },
            Button::Disconnect => match self.current() {
                Tab::One(source) => Action::Disconnect(source),
                Tab::All => Action::None,
            },
            Button::Connect | Button::Next => self.submit_token(),
            Button::Cancel => Action::Close,
            Button::Back if self.picker.is_some() => {
                self.picker = None;
                Action::None
            }
            Button::Back if self.token_form().is_some() => {
                self.step_back();
                Action::None
            }
            Button::Back => {
                self.screen = Screen::List;
                Action::None
            }
        }
    }

    fn needs_place(&self, issue: &Issue) -> bool {
        issue.source != Source::Github && self.places.len() > 1 && self.place.is_none()
    }

    fn ready_to_start(&self, issue: &Issue) -> bool {
        !self.needs_place(issue) && self.agents.pick().is_some()
    }

    pub fn place_for(&self, issue: &Issue) -> Option<&Place> {
        (issue.source != Source::Github).then(|| self.places.get(self.place.unwrap_or(self.here))).flatten()
    }

    fn start(&mut self, issue: Issue) -> Action {
        if self.needs_place(&issue) {
            self.open_picker(Pick::Place, true);
            return Action::None;
        }
        if let Some(kind) = self.agents.pick() {
            let place = self.place_for(&issue).cloned();
            Action::Start(issue, kind, place)
        } else {
            self.open_picker(Pick::Agent, true);
            Action::None
        }
    }

    fn open_picker(&mut self, pick: Pick, then_start: bool) {
        let mut search = Search::default();
        let current = match pick {
            Pick::Agent => self.agents.pick().and_then(|k| self.agents.kinds.iter().position(|kind| *kind == k)),
            Pick::Place => Some(self.place.unwrap_or(self.here)),
            Pick::Person(source, field) => {
                let who = &self.people.of(source)[field];
                self.person_choices(source).iter().position(|(w, _)| w == who)
            }
        };
        search.select(current.unwrap_or(0));
        self.picker = Some(Picker { pick, search, then_start });
    }

    fn person_choices(&self, source: Source) -> Vec<(Who, String)> {
        let me = self.connections.get(&source).and_then(|c| c.account.as_ref()).map(|a| a.handle.clone());
        let mut choices = vec![
            (Who::Anyone, "no filter".to_string()),
            (Who::Me, me.map_or_else(|| "you".into(), |h| format!("you (@{h})"))),
        ];
        if let Some(Ok(people)) = self.members.get(&source) {
            choices.extend(people.iter().map(|p| (p.who(), p.name.clone())));
        }
        choices
    }

    pub fn choices(&self) -> Vec<Choice> {
        let Some(picker) = &self.picker else { return Vec::new() };
        let needle = picker.search.query().trim().to_lowercase();
        let all: Vec<Choice> = match picker.pick {
            Pick::Agent => self
                .agents
                .kinds
                .iter()
                .map(|kind| Choice {
                    key: kind.clone(),
                    title: self.agents.starts.get(kind).map(|s| s.command.clone()).unwrap_or_default(),
                    meta: self.agents.notes(kind),
                    who: None,
                })
                .collect(),
            Pick::Place => self
                .places
                .iter()
                .enumerate()
                .map(|(i, place)| Choice {
                    key: place.label.clone(),
                    title: if place.worktree { "a new worktree".into() } else { "a new tab".into() },
                    meta: if i == self.here { "where you are".into() } else { String::new() },
                    who: None,
                })
                .collect(),
            Pick::Person(source, _) => self
                .person_choices(source)
                .into_iter()
                .map(|(who, name)| Choice { key: who.describe(), title: name, meta: String::new(), who: Some(who) })
                .collect(),
        };
        all.into_iter()
            .filter(|c| c.key.to_lowercase().contains(&needle) || c.title.to_lowercase().contains(&needle))
            .collect()
    }

    fn choose(&mut self, index: Option<usize>) -> Action {
        let Some(picker) = &self.picker else { return Action::None };
        let (pick, then_start) = (picker.pick, picker.then_start);
        let choices = self.choices();
        let i = index.unwrap_or_else(|| picker.search.selected()).min(choices.len().saturating_sub(1));
        let Some((key, who)) = choices.get(i).map(|c| (c.key.clone(), c.who.clone())) else { return Action::None };
        self.picker = None;
        match pick {
            Pick::Agent => self.agents.chosen = Some(key),
            Pick::Place => self.place = self.places.iter().position(|p| p.label == key),
            Pick::Person(source, field) => {
                if let Some(who) = who {
                    self.set_person(source, field, who);
                }
            }
        }
        match (&self.screen, then_start) {
            (Screen::Detail { issue, .. }, true) => {
                let issue = (**issue).clone();
                self.start(issue)
            }
            _ => Action::None,
        }
    }

    fn picker_key(&mut self, key: KeyEvent, room: Room) -> Action {
        let rows = room.list_rows;
        let count = self.choices().len();
        let Some(picker) = &mut self.picker else { return Action::None };
        match key.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Enter => return self.choose(None),
            KeyCode::Up => picker.search.move_selection(-1, count, rows),
            KeyCode::Down => picker.search.move_selection(1, count, rows),
            KeyCode::Backspace => picker.search.pop(),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                picker.search.push(c);
            }
            _ => {}
        }
        Action::None
    }

    fn people_key(&mut self, key: KeyEvent) -> Action {
        let rows = self.people_rows();
        let Some(cursor) = &mut self.filtering else { return Action::None };
        match key.code {
            KeyCode::Esc => return self.close_people(),
            KeyCode::Up => *cursor = cursor.saturating_sub(1),
            KeyCode::Down => *cursor = (*cursor + 1).min(rows.len().saturating_sub(1)),
            KeyCode::Enter => {
                if let Some((source, field)) = rows.get(*cursor).copied() {
                    self.open_picker(Pick::Person(source, field), false);
                }
            }
            _ => {}
        }
        Action::None
    }

    pub fn default_agent_set(&mut self, kind: String) {
        self.notice = Some(format!("{kind} is now the default agent"));
        self.agents.default = Some(kind);
    }

    fn submit_token(&mut self) -> Action {
        let Some(source) = self.token_form() else { return Action::None };
        let email = self.jira_email.clone();
        let form = self.form(source);
        let input = form.input.trim().to_string();
        if form.checking {
            return Action::None;
        }
        match form.step {
            Step::Site => match jira::check_site(&input) {
                Ok(site) => {
                    *form = TokenForm { input: email, step: Step::Email, ..TokenForm::default() };
                    self.jira_site = site;
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Email => match jira::check_email(&input) {
                Ok(email) => {
                    *form = TokenForm::default();
                    self.jira_email.clone_from(&email);
                    return Action::SaveJira { site: self.jira_site.clone(), email };
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Workspace => match plane::check_workspace(&input) {
                Ok(workspace) => {
                    *form = TokenForm::default();
                    self.plane_workspace.clone_from(&workspace);
                    return Action::SavePlane { workspace };
                }
                Err(e) => form.error = Some(e.into()),
            },
            Step::Token if input.is_empty() => {}
            Step::Token => {
                form.checking = true;
                form.error = None;
                return Action::CheckToken(source, Secret(input));
            }
        }
        Action::None
    }

    fn step_back(&mut self) {
        let Some(source) = self.token_form() else { return };
        let (site, email, workspace) = (self.jira_site.clone(), self.jira_email.clone(), self.plane_workspace.clone());
        let form = self.form(source);
        if form.checking {
            return;
        }
        *form = match (form.step, source) {
            (Step::Token, Source::Plane) | (Step::Workspace, _) => {
                TokenForm { input: workspace, step: Step::Workspace, ..TokenForm::default() }
            }
            (Step::Token, _) => TokenForm { input: email, step: Step::Email, ..TokenForm::default() },
            (Step::Email | Step::Site, _) => TokenForm { input: site, step: Step::Site, ..TokenForm::default() },
        };
    }

    fn scroll_detail(&mut self, delta: isize, room: Room) {
        let max = self.detail_lines(room).len().saturating_sub(room.detail_rows);
        if let Screen::Detail { scroll, .. } = &mut self.screen {
            *scroll = (*scroll).min(max).saturating_add_signed(delta).min(max);
        }
    }

    pub fn key(&mut self, key: KeyEvent, room: Room) -> Action {
        if self.starting {
            return Action::None;
        }
        self.notice = None;
        let page = isize::try_from(room.detail_rows).unwrap_or(1).max(1);
        if self.picker.is_some() {
            return self.picker_key(key, room);
        }
        if self.filtering.is_some() {
            return self.people_key(key);
        }
        if let Screen::Detail { issue, .. } = &self.screen {
            match key.code {
                KeyCode::Esc | KeyCode::Left => self.screen = Screen::List,
                KeyCode::Enter => {
                    let issue = (**issue).clone();
                    return self.start(issue);
                }
                KeyCode::Up => self.scroll_detail(-1, room),
                KeyCode::Down => self.scroll_detail(1, room),
                KeyCode::PageUp => self.scroll_detail(-page, room),
                KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_detail(page, room),
                KeyCode::Home => self.scroll_detail(isize::MIN / 2, room),
                KeyCode::End => self.scroll_detail(isize::MAX / 2, room),
                _ => {}
            }
            return Action::None;
        }
        let typing = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Tab => return self.switch(1),
            KeyCode::BackTab => return self.switch(-1),
            _ => {}
        }
        if let Some(source) = self.token_form() {
            let form = self.form(source);
            match key.code {
                KeyCode::Enter => return self.submit_token(),
                KeyCode::Backspace if !form.checking => {
                    form.input.pop();
                    form.error = None;
                }
                KeyCode::Char(c) if typing && !form.checking => {
                    form.input.push(c);
                    form.error = None;
                }
                _ => {}
            }
            return Action::None;
        }
        let rows = room.list_rows;
        let shown = self.shown().len();
        match key.code {
            KeyCode::Enter => return self.selected().map_or(Action::None, |issue| self.read(issue)),
            KeyCode::Up => self.search.move_selection(-1, shown, rows),
            KeyCode::Down => self.search.move_selection(1, shown, rows),
            KeyCode::PageUp => self.search.move_selection(-isize::try_from(rows).unwrap_or(1), shown, rows),
            KeyCode::PageDown => self.search.move_selection(isize::try_from(rows).unwrap_or(1), shown, rows),
            KeyCode::Backspace => self.search.pop(),
            KeyCode::Char(c) if typing => self.search.push(c),
            _ => {}
        }
        Action::None
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(picker) = &mut self.picker {
            text.chars().filter(|c| !c.is_control()).for_each(|c| picker.search.push(c));
            return;
        }
        if self.starting || matches!(self.screen, Screen::Detail { .. }) {
            return;
        }
        let chars = text.chars().filter(|c| !c.is_control());
        if let Some(source) = self.token_form() {
            let form = self.form(source);
            if !form.checking {
                form.input.extend(chars.filter(|c| !c.is_whitespace()));
                form.error = None;
            }
            return;
        }
        chars.for_each(|c| self.search.push(c));
    }

    pub fn mouse(&mut self, kind: MouseEventKind, hit: Option<IssuesHit>, room: Room) -> Action {
        if self.starting {
            return Action::None;
        }
        if matches!(kind, MouseEventKind::Down(_)) {
            self.notice = None;
        }
        if self.picker.is_some() || self.filtering.is_some() {
            let rows = room.list_rows;
            let count = if self.picker.is_some() { self.choices().len() } else { self.people_rows().len() };
            match kind {
                MouseEventKind::ScrollUp => {
                    self.picker.iter_mut().for_each(|p| p.search.scroll_by(-WHEEL_ROWS, count, rows));
                }
                MouseEventKind::ScrollDown => {
                    self.picker.iter_mut().for_each(|p| p.search.scroll_by(WHEEL_ROWS, count, rows));
                }
                MouseEventKind::Down(MouseButton::Left) => match hit {
                    Some(IssuesHit::Item(i)) if self.picker.is_some() => return self.choose(Some(i)),
                    Some(IssuesHit::Item(i)) => {
                        self.filtering = Some(i);
                        if let Some((source, field)) = self.people_rows().get(i).copied() {
                            self.open_picker(Pick::Person(source, field), false);
                        }
                    }
                    Some(IssuesHit::Button(i)) => return self.press(self.buttons()[i]),
                    _ => {}
                },
                _ => {}
            }
            return Action::None;
        }
        if matches!(self.screen, Screen::Detail { .. }) {
            match kind {
                MouseEventKind::ScrollUp => self.scroll_detail(-WHEEL_ROWS, room),
                MouseEventKind::ScrollDown => self.scroll_detail(WHEEL_ROWS, room),
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(IssuesHit::Button(i)) = hit {
                        return self.press(self.buttons()[i]);
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        let rows = room.list_rows;
        let shown = if self.token_form().is_some() { 0 } else { self.shown().len() };
        match kind {
            MouseEventKind::ScrollUp => self.search.scroll_by(-WHEEL_ROWS, shown, rows),
            MouseEventKind::ScrollDown => self.search.scroll_by(WHEEL_ROWS, shown, rows),
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(IssuesHit::Tab(i)) => {
                    self.tab = i;
                    self.search.reset();
                    return self.needs_load();
                }
                Some(IssuesHit::Toggle(i)) => return self.toggle(i),
                Some(IssuesHit::Item(i)) => {
                    if let Some(issue) = self.shown().get(i).map(|issue| (*issue).clone()) {
                        return self.read(issue);
                    }
                }
                Some(IssuesHit::Button(i)) => return self.press(self.buttons()[i]),
                None => {}
            },
            _ => {}
        }
        Action::None
    }

    pub fn start_hint(&self, issue: &Issue) -> String {
        match self.place_for(issue) {
            Some(place) if place.worktree => format!("in its own worktree, on {}, in {}", branch(issue), place.label),
            Some(place) => format!("in a new tab in {}", place.label),
            None if self.worktrees => format!("in its own worktree, on {}", branch(issue)),
            None => "in a new tab".into(),
        }
    }

    fn start_lines(&self, issue: &Issue) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let row = |label: &str, spans: Vec<Span<'static>>| {
            Line::from([vec![Span::styled(format!("{label:<8}"), dim)], spans].concat())
        };
        let agent = match self.agents.pick() {
            Some(kind) => {
                let start = self.agents.starts.get(&kind);
                let mut spans = vec![Span::styled(kind.clone(), Style::default().add_modifier(Modifier::BOLD))];
                if let Some(mode) = start.and_then(|s| s.mode.clone()) {
                    let style = if crate::agents::is_dangerous(&mode) {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default()
                    };
                    spans.push(Span::raw(" · "));
                    spans.push(Span::styled(mode, style));
                }
                if let Some(start) = start {
                    spans.push(Span::styled(format!("  {}", start.command), dim));
                }
                let notes = self.agents.notes(&kind);
                if !notes.is_empty() {
                    spans.push(Span::styled(format!("  ({notes})"), dim));
                }
                spans
            }
            None => vec![Span::styled("none chosen yet: start asks which one", dim)],
        };
        let prompt = super::prompt(&self.agents.prompt, issue, &branch(issue));
        let send = if self.agents.submit { "sent for you" } else { "typed, you press enter" };
        vec![
            row("start", vec![Span::raw(self.start_hint(issue))]),
            row("agent", agent),
            row("prompt", vec![Span::raw(crate::text::one_line(&prompt)), Span::styled(format!("  ({send})"), dim)]),
        ]
    }

    pub fn detail_lines(&self, room: Room) -> Vec<Line<'static>> {
        let Screen::Detail { issue, detail, raw, .. } = &self.screen else { return Vec::new() };
        let width = room.detail_width;
        let start = self.start_lines(issue);
        detail_lines(issue, detail.as_ref().and_then(|d| d.as_ref().ok()), width, time::now(), *raw, start)
    }
}

fn picker_view(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let picker = browser.picker.as_ref();
    let choices = browser.choices();
    let selected = choices.len().checked_sub(1).map(|last| picker.map_or(0, |p| p.search.selected()).min(last));
    let items = choices.into_iter().map(|c| ui::IssueRow { key: c.key, title: c.title, meta: c.meta }).collect();
    let body = ui::IssuesBody::List {
        filter: picker.map(|p| p.search.query().to_string()).unwrap_or_default(),
        items,
        selected,
        scroll: picker.map_or(0, |p| p.search.scroll()),
        empty: "nothing matches".into(),
    };
    let (hint, note) = match picker.map(|p| p.pick) {
        Some(Pick::Place) => {
            ("where should it be worked on? a repository gets a worktree, others a new tab".to_string(), None)
        }
        Some(Pick::Person(source, field)) => {
            let note = match browser.members.get(&source) {
                None => Some(ui::Note::Busy("loading people…")),
                Some(Err(e)) => Some(ui::Note::Error(e.clone())),
                Some(Ok(_)) => None,
            };
            (
                format!(
                    "{} {}: anyone, you, or someone else · type to find them",
                    source.name(),
                    source.people_fields()[field]
                ),
                note,
            )
        }
        _ => ("which agent should take it? settings set how each one starts".to_string(), None),
    };
    (body, note, hint)
}

fn people_view(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let rows = browser.people_rows();
    let items = rows
        .iter()
        .map(|(source, field)| ui::IssueRow {
            key: format!("{} {}", source.name(), source.people_fields()[*field]),
            title: browser.people.of(*source)[*field].describe(),
            meta: String::new(),
        })
        .collect();
    let body = ui::IssuesBody::List {
        filter: String::new(),
        items,
        selected: browser.filtering.map(|c| c.min(rows.len().saturating_sub(1))),
        scroll: 0,
        empty: "nothing to filter here".into(),
    };
    (body, None, "enter picks a person · the filter goes into each tracker's search and is remembered".into())
}

pub fn view(browser: &Browser, room: Room, now: i64) -> ui::Overlay {
    let (body, note, hint) = match &browser.screen {
        _ if browser.picker.is_some() => picker_view(browser),
        _ if browser.filtering.is_some() => people_view(browser),
        Screen::Detail { detail, scroll, .. } => {
            let lines = browser.detail_lines(room);
            let rows = room.detail_rows;
            let scroll = (*scroll).min(lines.len().saturating_sub(rows));
            let note = match detail {
                None => Some(ui::Note::Busy("loading…")),
                Some(Err(e)) => Some(ui::Note::Error(e.clone())),
                Some(Ok(_)) => None,
            };
            let more = if lines.len() > rows {
                format!(" · lines {}-{} of {}", scroll + 1, (scroll + rows).min(lines.len()), lines.len())
            } else {
                String::new()
            };
            let hint = format!("enter starts it{more}");
            (ui::IssuesBody::Detail { lines, scroll }, note, hint)
        }
        Screen::List => match browser.token_form() {
            Some(source) => token_view(browser, source),
            None => list_view(browser, now),
        },
    };
    let hint = browser.notice.clone().unwrap_or(hint);
    let (toggles, on) = browser.toggles();
    let note = if browser.starting {
        Some(ui::Note::Busy("starting…"))
    } else if let Some(e) = &browser.error {
        Some(ui::Note::Error(e.clone()))
    } else {
        note
    };
    ui::Overlay::Issues(ui::Issues {
        title: format!("Issues · {}", browser.project_name),
        tabs: browser.tab_names(),
        tab: browser.tab,
        toggles,
        on,
        body,
        note,
        hint,
        buttons: browser.labels(),
    })
}

fn token_view(browser: &Browser, source: Source) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let form = browser.forms.get(&source);
    let step = browser.form_step(source);
    let (label, input) = browser.token_field(source);
    let mut help = vec![format!("Connect {}.", source.name()), String::new()];
    match step {
        Step::Site => {
            help.push("Type your Jira Cloud site, such as acme.atlassian.net, and press Enter.".into());
        }
        Step::Email => {
            help.push("Type the email you sign in to Atlassian with and press Enter.".into());
        }
        Step::Workspace => {
            help.push("Type your Plane workspace, the part after the server's name, such as acme.".into());
            help.push("Then press Enter.".into());
        }
        Step::Token => {
            match source {
                Source::Jira => help
                    .extend([format!("Signing in to {} as {}.", browser.jira_site, browser.jira_email), String::new()]),
                Source::Plane => {
                    let line = format!("Connecting to the Plane workspace {}.", browser.plane_workspace);
                    help.extend([line, String::new()]);
                }
                _ => {}
            }
            help.push(source.token_help().into());
        }
    }
    match (step, source.token_env()) {
        (Step::Token, Some(env)) => {
            help.push(String::new());
            help.push(format!(
                "It is saved in {} (only you can read it); you can also paste it in settings. {env}, when set, \
                 takes precedence.",
                browser.secrets_path
            ));
        }
        _ if source == Source::Plane => {
            help.extend([String::new(), "The workspace is saved in your settings.".into()]);
        }
        _ => help.extend([String::new(), "The site and the email are saved in your settings.".into()]),
    }
    let note = match form {
        Some(f) if f.checking => Some(ui::Note::Busy("checking…")),
        Some(TokenForm { error: Some(e), .. }) => Some(ui::Note::Error(e.clone())),
        _ => None,
    };
    (ui::IssuesBody::Token { label, input, help }, note, String::new())
}

fn list_view(browser: &Browser, now: i64) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let shown = browser.shown();
    let selected = shown.len().checked_sub(1).map(|last| browser.search.selected().min(last));
    let listings: Vec<(Source, &Listing)> =
        browser.sources().into_iter().filter_map(|s| browser.lists.get(&s).map(|l| (s, l))).collect();
    let loading = listings.iter().any(|(_, l)| l.loading);
    let error = listings.iter().find_map(|(s, l)| {
        l.error.as_ref().map(|e| if browser.current() == Tab::All { format!("{}: {e}", s.name()) } else { e.clone() })
    });
    let empty = if !browser.search.query().trim().is_empty() {
        "no matches".to_string()
    } else if loading || error.is_some() {
        String::new()
    } else if browser.current() == Tab::One(Source::Github) && !browser.github {
        "this project is not in a git repository".into()
    } else if browser.sources().is_empty() {
        "nothing to list here: connect Shortcut, Linear, Jira or Plane in their tabs".into()
    } else if browser.closed {
        "no issues".into()
    } else {
        "no open issues".into()
    };
    let note = match error {
        Some(e) => Some(ui::Note::Error(e)),
        None if loading && shown.is_empty() => Some(ui::Note::Busy("loading…")),
        None => None,
    };
    let account = match browser.current() {
        Tab::One(source) => browser.connections.get(&source).and_then(|c| c.account.as_ref()).map(Account::describe),
        Tab::All => None,
    };
    let action =
        selected.map(|i| format!("enter reads {} · start works on it {}", shown[i].key, browser.start_hint(shown[i])));
    let refreshing = (loading && !shown.is_empty()).then(|| "refreshing…".to_string());
    let hint = refreshing.into_iter().chain(account).chain(action).collect::<Vec<_>>().join(" · ");
    let items = shown
        .iter()
        .map(|issue| ui::IssueRow {
            key: issue.key.clone(),
            title: issue.title.clone(),
            meta: issues::meta(issue, now),
        })
        .collect();
    let body = ui::IssuesBody::List {
        filter: browser.search.query().to_string(),
        items,
        selected,
        scroll: browser.search.scroll(),
        empty,
    };
    (body, note, hint)
}

fn raw_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    text.replace('\t', "    ")
        .split('\n')
        .flat_map(|line| {
            let chars: Vec<char> = line.trim_end().chars().collect();
            if chars.is_empty() {
                return vec![Line::default()];
            }
            chars.chunks(width).map(|chunk| Line::from(chunk.iter().collect::<String>())).collect()
        })
        .collect()
}

pub fn detail_lines(
    issue: &Issue,
    detail: Option<&Detail>,
    width: usize,
    now: i64,
    raw: bool,
    start: Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    let body = |text: &str| if raw { raw_lines(text, width) } else { markdown::render(text, width) };
    let dim = Style::default().fg(Color::DarkGray);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let segments = [(issue.key.clone(), bold.fg(Color::Cyan)), (format!(" {}", issue.title), bold)];
    let mut lines = markdown::wrap_segments(&segments, width);
    let mut info: Vec<String> = detail.map_or_else(|| vec![issue.state.clone()], |d| d.info.clone());
    let age = super::age_of(&issue.updated_at, now);
    info.extend((!age.is_empty()).then(|| format!("updated {age} ago")));
    lines.extend(markdown::wrap_text(&info.join(" · "), dim, width));
    lines.extend(markdown::wrap_text(&issue.url, dim, width));
    if !start.is_empty() {
        lines.push(Line::default());
        lines.extend(start);
    }
    let Some(detail) = detail else { return lines };
    lines.push(Line::default());
    if detail.body.trim().is_empty() {
        lines.push(Line::from(Span::styled("No description.", dim)));
    } else {
        lines.extend(body(&detail.body));
    }
    for comment in &detail.comments {
        lines.push(Line::default());
        let age = super::age_of(&comment.created_at, now);
        let head = if age.is_empty() {
            format!("── @{} ", comment.author)
        } else {
            format!("── @{} · {age} ago ", comment.author)
        };
        let fill = width.saturating_sub(head.chars().count());
        lines.push(Line::from(Span::styled(format!("{head}{}", "─".repeat(fill)), dim)));
        lines.extend(body(&comment.body));
    }
    lines
}

#[cfg(test)]
pub(crate) mod tests;
