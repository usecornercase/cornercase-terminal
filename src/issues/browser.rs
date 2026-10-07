use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{Account, Detail, Issue, People, Person, Query, Secret, Source, Who, branch, jira, sort_by_updated};
use crate::markdown;
use crate::search::Search;
use crate::ui::{self, IssuesHit};

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

struct Choice {
    key: String,
    title: String,
    meta: String,
    who: Option<Who>,
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

    fn token_form(&self) -> Option<Source> {
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
        let site = self.jira_site.clone();
        self.forms.entry(source).or_insert_with(|| match source {
            Source::Jira => TokenForm { input: site, step: Step::Site, ..TokenForm::default() },
            _ => TokenForm::default(),
        })
    }

    fn form_step(&self, source: Source) -> Step {
        match (self.forms.get(&source), source) {
            (Some(form), _) => form.step,
            (None, Source::Jira) => Step::Site,
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
                Some(Step::Site) => vec![Button::Next, Button::Cancel],
                Some(Step::Email) => vec![Button::Next, Button::Back, Button::Cancel],
                Some(Step::Token) if self.token_form() == Some(Source::Jira) => {
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

    fn labels(&self) -> Vec<&'static str> {
        self.buttons().into_iter().map(Button::label).collect()
    }

    fn tab_names(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name()).collect()
    }

    fn rows(area: Rect) -> usize {
        usize::from(ui::issues_list(ui::issues_area(area)).height)
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

    fn people_rows(&self) -> Vec<(Source, usize)> {
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
            ui::truncate_right(&format!("people: {}", parts.join(", ")), PEOPLE_LABEL_MAX)
        }
    }

    fn toggles(&self) -> (Vec<String>, Vec<bool>) {
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

    fn choices(&self) -> Vec<Choice> {
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

    fn picker_key(&mut self, key: KeyEvent, area: Rect) -> Action {
        let rows = Self::rows(area);
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
        let (site, email) = (self.jira_site.clone(), self.jira_email.clone());
        let form = self.form(source);
        if form.checking {
            return;
        }
        *form = match form.step {
            Step::Token => TokenForm { input: email, step: Step::Email, ..TokenForm::default() },
            Step::Email | Step::Site => TokenForm { input: site, step: Step::Site, ..TokenForm::default() },
        };
    }

    fn detail_rows(area: Rect) -> usize {
        usize::from(ui::issue_detail(ui::issues_area(area)).height)
    }

    fn scroll_detail(&mut self, delta: isize, area: Rect) {
        let max = self.detail_lines(area).len().saturating_sub(Self::detail_rows(area));
        if let Screen::Detail { scroll, .. } = &mut self.screen {
            *scroll = (*scroll).min(max).saturating_add_signed(delta).min(max);
        }
    }

    pub fn key(&mut self, key: KeyEvent, area: Rect) -> Action {
        if self.starting {
            return Action::None;
        }
        self.notice = None;
        let page = isize::try_from(Self::detail_rows(area)).unwrap_or(1).max(1);
        if self.picker.is_some() {
            return self.picker_key(key, area);
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
                KeyCode::Up => self.scroll_detail(-1, area),
                KeyCode::Down => self.scroll_detail(1, area),
                KeyCode::PageUp => self.scroll_detail(-page, area),
                KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_detail(page, area),
                KeyCode::Home => self.scroll_detail(isize::MIN / 2, area),
                KeyCode::End => self.scroll_detail(isize::MAX / 2, area),
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
        let rows = Self::rows(area);
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

    pub fn mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Action {
        if self.starting {
            return Action::None;
        }
        if matches!(ev.kind, MouseEventKind::Down(_)) {
            self.notice = None;
        }
        let labels = self.labels();
        let (toggle_labels, _) = self.toggles();
        let toggles: Vec<&str> = toggle_labels.iter().map(String::as_str).collect();
        if self.picker.is_some() || self.filtering.is_some() {
            let rows = Self::rows(area);
            let count = if self.picker.is_some() { self.choices().len() } else { self.people_rows().len() };
            let scroll = self.picker.as_ref().map_or(0, |p| p.search.scroll());
            match ev.kind {
                MouseEventKind::ScrollUp => {
                    self.picker.iter_mut().for_each(|p| p.search.scroll_by(-WHEEL_ROWS, count, rows));
                }
                MouseEventKind::ScrollDown => {
                    self.picker.iter_mut().for_each(|p| p.search.scroll_by(WHEEL_ROWS, count, rows));
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    match ui::issues_hit(area, &self.tab_names(), &toggles, &labels, count, scroll, pos) {
                        Some(IssuesHit::Item(i)) if self.picker.is_some() => return self.choose(Some(i)),
                        Some(IssuesHit::Item(i)) => {
                            self.filtering = Some(i);
                            if let Some((source, field)) = self.people_rows().get(i).copied() {
                                self.open_picker(Pick::Person(source, field), false);
                            }
                        }
                        Some(IssuesHit::Button(i)) => return self.press(self.buttons()[i]),
                        _ => {}
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        if matches!(self.screen, Screen::Detail { .. }) {
            match ev.kind {
                MouseEventKind::ScrollUp => self.scroll_detail(-WHEEL_ROWS, area),
                MouseEventKind::ScrollDown => self.scroll_detail(WHEEL_ROWS, area),
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(i) = ui::issue_button_hit(area, &labels, pos) {
                        return self.press(self.buttons()[i]);
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        let rows = Self::rows(area);
        let shown = if self.token_form().is_some() { 0 } else { self.shown().len() };
        match ev.kind {
            MouseEventKind::ScrollUp => self.search.scroll_by(-WHEEL_ROWS, shown, rows),
            MouseEventKind::ScrollDown => self.search.scroll_by(WHEEL_ROWS, shown, rows),
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = ui::issues_hit(area, &self.tab_names(), &toggles, &labels, shown, self.search.scroll(), pos);
                match hit {
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
                }
            }
            _ => {}
        }
        Action::None
    }

    fn start_hint(&self, issue: &Issue) -> String {
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
            row("prompt", vec![Span::raw(super::one_line(&prompt)), Span::styled(format!("  ({send})"), dim)]),
        ]
    }

    fn detail_lines(&self, area: Rect) -> Vec<Line<'static>> {
        let Screen::Detail { issue, detail, raw, .. } = &self.screen else { return Vec::new() };
        let width = usize::from(ui::issue_detail(ui::issues_area(area)).width);
        let start = self.start_lines(issue);
        detail_lines(issue, detail.as_ref().and_then(|d| d.as_ref().ok()), width, super::now(), *raw, start)
    }

    fn picker_view(&self) -> (ui::IssuesBody, Option<ui::Note>, String) {
        let picker = self.picker.as_ref();
        let choices = self.choices();
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
                let note = match self.members.get(&source) {
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

    fn people_view(&self) -> (ui::IssuesBody, Option<ui::Note>, String) {
        let rows = self.people_rows();
        let items = rows
            .iter()
            .map(|(source, field)| ui::IssueRow {
                key: format!("{} {}", source.name(), source.people_fields()[*field]),
                title: self.people.of(*source)[*field].describe(),
                meta: String::new(),
            })
            .collect();
        let body = ui::IssuesBody::List {
            filter: String::new(),
            items,
            selected: self.filtering.map(|c| c.min(rows.len().saturating_sub(1))),
            scroll: 0,
            empty: "nothing to filter here".into(),
        };
        (body, None, "enter picks a person · the filter goes into each tracker's search and is remembered".into())
    }

    pub fn view(&self, area: Rect, now: i64) -> ui::Overlay {
        let (body, note, hint) = match &self.screen {
            _ if self.picker.is_some() => self.picker_view(),
            _ if self.filtering.is_some() => self.people_view(),
            Screen::Detail { detail, scroll, .. } => {
                let lines = self.detail_lines(area);
                let rows = Self::detail_rows(area);
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
            Screen::List => match self.token_form() {
                Some(source) => self.token_view(source),
                None => self.list_view(now),
            },
        };
        let hint = self.notice.clone().unwrap_or(hint);
        let (toggles, on) = self.toggles();
        let note = if self.starting {
            Some(ui::Note::Busy("starting…"))
        } else if let Some(e) = &self.error {
            Some(ui::Note::Error(e.clone()))
        } else {
            note
        };
        ui::Overlay::Issues(ui::Issues {
            title: format!("issues · {}", self.project_name),
            tabs: self.tab_names(),
            tab: self.tab,
            toggles,
            on,
            body,
            note,
            hint,
            buttons: self.labels(),
        })
    }

    fn token_view(&self, source: Source) -> (ui::IssuesBody, Option<ui::Note>, String) {
        let form = self.forms.get(&source);
        let step = self.form_step(source);
        let typed = form.map_or_else(
            || if step == Step::Site { self.jira_site.clone() } else { String::new() },
            |f| f.input.clone(),
        );
        let mut help = vec![format!("Connect {}.", source.name()), String::new()];
        let (label, input) = match step {
            Step::Site => {
                help.push("Type your Jira Cloud site, such as acme.atlassian.net, and press Enter.".into());
                ("site", typed)
            }
            Step::Email => {
                help.push("Type the email you sign in to Atlassian with and press Enter.".into());
                ("email", typed)
            }
            Step::Token => {
                if source == Source::Jira {
                    help.extend([format!("Signing in to {} as {}.", self.jira_site, self.jira_email), String::new()]);
                }
                help.push(source.token_help().into());
                (source.token_name(), "•".repeat(typed.chars().count().min(TOKEN_DOTS)))
            }
        };
        match (step, source.token_env()) {
            (Step::Token, Some(env)) => {
                help.push(String::new());
                help.push(format!(
                    "It is saved in {} (only you can read it); you can also paste it in settings. {env}, when set, \
                     takes precedence.",
                    self.secrets_path
                ));
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

    fn list_view(&self, now: i64) -> (ui::IssuesBody, Option<ui::Note>, String) {
        let shown = self.shown();
        let selected = shown.len().checked_sub(1).map(|last| self.search.selected().min(last));
        let listings: Vec<(Source, &Listing)> =
            self.sources().into_iter().filter_map(|s| self.lists.get(&s).map(|l| (s, l))).collect();
        let loading = listings.iter().any(|(_, l)| l.loading);
        let error = listings.iter().find_map(|(s, l)| {
            l.error.as_ref().map(|e| if self.current() == Tab::All { format!("{}: {e}", s.name()) } else { e.clone() })
        });
        let empty = if !self.search.query().trim().is_empty() {
            "no matches".to_string()
        } else if loading || error.is_some() {
            String::new()
        } else if self.current() == Tab::One(Source::Github) && !self.github {
            "this project is not in a git repository".into()
        } else if self.sources().is_empty() {
            "nothing to list here: connect Shortcut, Linear or Jira in their tabs".into()
        } else if self.closed {
            "no issues".into()
        } else {
            "no open issues".into()
        };
        let note = match error {
            Some(e) => Some(ui::Note::Error(e)),
            None if loading && shown.is_empty() => Some(ui::Note::Busy("loading…")),
            None => None,
        };
        let account = match self.current() {
            Tab::One(source) => self.connections.get(&source).and_then(|c| c.account.as_ref()).map(Account::describe),
            Tab::All => None,
        };
        let action =
            selected.map(|i| format!("enter reads {} · start works on it {}", shown[i].key, self.start_hint(shown[i])));
        let refreshing = (loading && !shown.is_empty()).then(|| "refreshing…".to_string());
        let hint = refreshing.into_iter().chain(account).chain(action).collect::<Vec<_>>().join(" · ");
        let items = shown
            .iter()
            .map(|issue| ui::IssueRow {
                key: issue.key.clone(),
                title: issue.title.clone(),
                meta: super::meta(issue, now),
            })
            .collect();
        let body = ui::IssuesBody::List {
            filter: self.search.query().to_string(),
            items,
            selected,
            scroll: self.search.scroll(),
            empty,
        };
        (body, note, hint)
    }
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
mod tests {
    use crossterm::event::KeyEventKind;

    use super::*;
    use crate::issues::{Comment, issue};

    const AREA: Rect = Rect { x: 0, y: 0, width: 120, height: 34 };

    fn browser() -> Browser {
        Browser {
            project: 1,
            project_name: "shop".into(),
            github: true,
            worktrees: true,
            tabs: tabs(&[]),
            tab: 0,
            closed: false,
            people: People::default(),
            search: Search::default(),
            lists: HashMap::new(),
            connections: HashMap::new(),
            forms: HashMap::new(),
            jira_site: String::new(),
            jira_email: String::new(),
            screen: Screen::List,
            starting: false,
            error: None,
            notice: None,
            secrets_path: "~/.config/cornercase/secrets.json".into(),
            agents: agents(Some("claude")),
            picker: None,
            places: vec![shop()],
            here: 0,
            place: None,
            filtering: None,
            members: HashMap::new(),
        }
    }

    fn shop() -> Place {
        Place { project: 1, workspace: None, label: "shop".into(), worktree: true }
    }

    fn notes() -> Place {
        Place { project: 2, workspace: Some(20), label: "notes › default".into(), worktree: false }
    }

    fn agents(default: Option<&str>) -> Agents {
        let start =
            |command: &str, mode: Option<&str>| AgentStart { command: command.into(), mode: mode.map(String::from) };
        Agents {
            kinds: vec!["claude".into(), "codex".into()],
            default: default.map(String::from),
            running: None,
            chosen: None,
            starts: HashMap::from([
                (
                    "claude".to_string(),
                    start("claude --dangerously-skip-permissions", Some("skip permissions (dangerous)")),
                ),
                ("codex".to_string(), start("codex", None)),
            ]),
            prompt: "{url}".into(),
            submit: false,
        }
    }

    fn with_lists() -> Browser {
        let mut b = browser();
        b.connected(Source::Shortcut, Connection { from_env: false, account: None });
        let old = Issue { updated_at: "2026-01-01T00:00:00Z".into(), ..issue(Source::Github, 1, "old bug") };
        let new = Issue { updated_at: "2026-09-01T00:00:00Z".into(), ..issue(Source::Shortcut, 2, "new story") };
        b.loaded(Source::Github, &Query::default(), Ok(vec![old]));
        b.loaded(Source::Shortcut, &Query::default(), Ok(vec![new]));
        b
    }

    fn press(b: &mut Browser, code: KeyCode) -> Action {
        b.key(
            KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::NONE,
            },
            AREA,
        )
    }

    fn type_text(b: &mut Browser, text: &str) {
        for c in text.chars() {
            press(b, KeyCode::Char(c));
        }
    }

    fn keys(b: &Browser) -> Vec<String> {
        b.shown().iter().map(|i| i.key.clone()).collect()
    }

    fn click(b: &mut Browser, pos: Position) -> Action {
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: pos.x,
            row: pos.y,
            modifiers: KeyModifiers::NONE,
        };
        b.mouse(ev, pos, AREA)
    }

    fn button(b: &Browser, label: &str) -> Position {
        let labels = b.labels();
        let i = labels.iter().position(|l| *l == label).expect("the button is there");
        ui::issue_buttons(ui::issues_area(AREA), &labels)[i].as_position()
    }

    mod tabs {
        use super::*;

        #[test]
        fn are_all_five_by_default() {
            assert_eq!(
                tabs(&[]),
                [
                    Tab::All,
                    Tab::One(Source::Github),
                    Tab::One(Source::Shortcut),
                    Tab::One(Source::Linear),
                    Tab::One(Source::Jira)
                ]
            );
        }

        #[test]
        fn follow_the_config_and_skip_unknown_names() {
            let names = ["linear", "nope", "GitHub", "linear"].map(String::from);
            assert_eq!(tabs(&names), [Tab::One(Source::Linear), Tab::One(Source::Github)]);
        }

        #[test]
        fn all_lists_every_source_newest_first() {
            assert_eq!(keys(&with_lists()), ["sc-2", "#1"]);
        }

        #[test]
        fn a_source_tab_lists_only_its_issues() {
            let mut b = with_lists();
            b.tab_to(Tab::One(Source::Github));
            assert_eq!(keys(&b), ["#1"]);
        }

        #[test]
        fn tab_moves_to_the_next_one_and_asks_for_its_issues() {
            let mut b = browser();
            assert_eq!(
                (press(&mut b, KeyCode::Tab), b.current()),
                (Action::Load(vec![Source::Github]), Tab::One(Source::Github))
            );
        }

        #[test]
        fn opening_all_loads_github_and_the_connected_trackers() {
            let mut b = browser();
            b.connected(Source::Linear, Connection { from_env: true, account: None });
            assert_eq!(b.needs_load(), Action::Load(vec![Source::Github, Source::Linear]));
        }

        #[test]
        fn outside_git_github_is_not_asked() {
            let b = Browser { github: false, ..browser() };
            assert_eq!(b.needs_load(), Action::None);
        }

        #[test]
        fn a_click_on_a_tab_switches_to_it() {
            let mut b = browser();
            let names = b.tab_names();
            let pos = ui::issue_tabs(ui::issues_area(AREA), &names)[2].as_position();
            click(&mut b, pos);
            assert_eq!(b.current(), Tab::One(Source::Shortcut));
        }
    }

    mod filter {
        use super::*;

        #[test]
        fn typing_filters_the_list() {
            let mut b = with_lists();
            type_text(&mut b, "bug");
            assert_eq!(keys(&b), ["#1"]);
        }

        #[test]
        fn closed_reloads_everything_with_the_new_query() {
            let mut b = with_lists();
            let toggle = ui::issue_toggles(ui::issues_area(AREA), &["closed", "people"])[0].as_position();

            let action = click(&mut b, toggle);

            assert_eq!(
                (action, b.closed, keys(&b)),
                (Action::Load(vec![Source::Github, Source::Shortcut]), true, Vec::<String>::new())
            );
        }

        #[test]
        fn results_for_an_old_query_are_dropped() {
            let mut b = browser();
            b.people.github[0] = Who::Me;
            b.loaded(Source::Github, &Query::default(), Ok(vec![issue(Source::Github, 1, "x")]));
            assert_eq!(b.shown(), Vec::<&Issue>::new());
        }
    }

    mod detail {
        use super::*;

        #[test]
        fn enter_reads_the_selected_issue() {
            let mut b = with_lists();
            let action = press(&mut b, KeyCode::Enter);
            assert!(
                matches!((&action, &b.screen), (Action::Read(i), Screen::Detail { .. }) if i.key == "sc-2"),
                "{action:?}"
            );
        }

        #[test]
        fn a_click_on_an_issue_reads_it() {
            let mut b = with_lists();
            let row = ui::issues_list(ui::issues_area(AREA));
            let action = click(&mut b, Position::new(row.x + 2, row.y + 1));
            assert!(matches!(action, Action::Read(i) if i.key == "#1"));
        }

        #[test]
        fn enter_in_the_issue_starts_it() {
            let mut b = with_lists();
            press(&mut b, KeyCode::Enter);
            assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, _) if i.key == "sc-2"));
        }

        #[test]
        fn esc_goes_back_to_the_list() {
            let mut b = with_lists();
            press(&mut b, KeyCode::Enter);
            assert_eq!((press(&mut b, KeyCode::Esc), matches!(b.screen, Screen::List)), (Action::None, true));
        }

        #[test]
        fn the_answer_for_another_issue_is_ignored() {
            let mut b = with_lists();
            press(&mut b, KeyCode::Enter);
            b.read_done(Source::Github, "#1", Ok(Detail::default()));
            assert!(matches!(b.screen, Screen::Detail { detail: None, .. }));
        }

        #[test]
        fn scrolling_stops_at_the_end() {
            let mut b = with_lists();
            press(&mut b, KeyCode::Enter);
            let body = (1..200).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n\n");
            b.read_done(Source::Shortcut, "sc-2", Ok(Detail { body, ..Detail::default() }));
            press(&mut b, KeyCode::End);
            press(&mut b, KeyCode::Down);
            let Screen::Detail { scroll, .. } = b.screen else { panic!("not in the detail") };
            assert_eq!(scroll, b.detail_lines(AREA).len() - Browser::detail_rows(AREA));
        }

        #[test]
        fn shows_the_title_meta_body_and_comments() {
            let detail = Detail {
                info: vec!["open".into(), "bug".into()],
                body: "It **crashes**.".into(),
                comments: vec![Comment { author: "bo".into(), created_at: String::new(), body: "Same".into() }],
            };
            let issue = issue(Source::Github, 7, "Login fails");
            let text: Vec<String> = detail_lines(&issue, Some(&detail), 30, 0, false, Vec::new())
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                .collect();
            assert_eq!(
                text,
                [
                    "#7 Login fails",
                    "open · bug",
                    "https://github.com/acme/shop/i",
                    "ssues/7",
                    "",
                    "It crashes.",
                    "",
                    "── @bo ───────────────────────",
                    "Same"
                ]
            );
        }
    }

    mod tokens {
        use super::*;

        fn on_shortcut() -> Browser {
            let mut b = browser();
            b.tab_to(Tab::One(Source::Shortcut));
            b
        }

        #[test]
        fn a_tracker_without_a_token_asks_for_one() {
            let b = on_shortcut();
            assert_eq!((b.token_form(), b.labels()), (Some(Source::Shortcut), vec!["connect", "cancel"]));
        }

        #[test]
        fn enter_checks_the_typed_token() {
            let mut b = on_shortcut();
            type_text(&mut b, "t0k");
            assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Shortcut, Secret("t0k".into())));
        }

        #[test]
        fn the_token_is_never_shown() {
            let mut b = on_shortcut();
            type_text(&mut b, "t0k");
            let ui::Overlay::Issues(view) = b.view(AREA, 0) else { panic!("not the issues view") };
            assert!(matches!(view.body, ui::IssuesBody::Token { input, .. } if input == "•••"));
        }

        #[test]
        fn an_empty_token_is_not_sent() {
            let mut b = on_shortcut();
            assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
        }

        #[test]
        fn once_connected_the_tab_lists_and_offers_to_disconnect() {
            let mut b = on_shortcut();
            b.connected(Source::Shortcut, Connection { from_env: false, account: None });
            assert_eq!(
                (b.needs_load(), b.labels()),
                (Action::Load(vec![Source::Shortcut]), vec!["start", "refresh", "disconnect", "cancel"])
            );
        }

        #[test]
        fn a_token_from_the_environment_cannot_be_disconnected() {
            let mut b = on_shortcut();
            b.connected(Source::Shortcut, Connection { from_env: true, account: None });
            assert_eq!(b.labels(), ["start", "refresh", "cancel"]);
        }

        mod jira {
            use super::*;

            fn on_jira() -> Browser {
                let mut b = browser();
                b.jira_site = "acme.atlassian.net".into();
                b.jira_email = "ana@acme.dev".into();
                b.tab_to(Tab::One(Source::Jira));
                b
            }

            fn field(b: &Browser) -> (&'static str, String) {
                let ui::Overlay::Issues(view) = b.view(AREA, 0) else { panic!("not the issues view") };
                let ui::IssuesBody::Token { label, input, .. } = view.body else { panic!("not the form") };
                (label, input)
            }

            fn to_the_token(b: &mut Browser) -> Action {
                press(b, KeyCode::Enter);
                press(b, KeyCode::Enter)
            }

            #[test]
            fn asks_the_site_first_with_the_saved_one_typed() {
                let b = on_jira();
                assert_eq!((field(&b), b.labels()), (("site", "acme.atlassian.net".into()), vec!["next", "cancel"]));
            }

            #[test]
            fn then_the_email_and_saves_both() {
                let mut b = on_jira();
                press(&mut b, KeyCode::Enter);
                assert_eq!((field(&b), b.labels()), (("email", "ana@acme.dev".into()), vec!["next", "back", "cancel"]));
                let saved = press(&mut b, KeyCode::Enter);
                assert_eq!(saved, Action::SaveJira { site: "acme.atlassian.net".into(), email: "ana@acme.dev".into() });
            }

            #[test]
            fn a_site_typed_as_a_name_gets_its_domain() {
                let mut b = on_jira();
                b.jira_site.clear();
                type_text(&mut b, "shop");
                press(&mut b, KeyCode::Enter);
                assert_eq!(b.jira_site, "shop.atlassian.net");
            }

            #[test]
            fn a_bad_email_stays_with_its_error() {
                let mut b = on_jira();
                b.jira_email = "ana".into();
                let action = to_the_token(&mut b);
                let error = b.forms.get(&Source::Jira).and_then(|f| f.error.clone());
                assert_eq!(
                    (action, field(&b).0, error.as_deref()),
                    (Action::None, "email", Some("type the email of your Atlassian account"))
                );
            }

            #[test]
            fn the_token_comes_last_and_is_hidden() {
                let mut b = on_jira();
                to_the_token(&mut b);
                type_text(&mut b, "t0k");
                assert_eq!((field(&b), b.labels()), (("API token", "•••".into()), vec!["connect", "back", "cancel"]));
                assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Jira, Secret("t0k".into())));
            }

            #[test]
            fn back_returns_to_the_email() {
                let mut b = on_jira();
                to_the_token(&mut b);
                let pos = button(&b, "back");
                click(&mut b, pos);
                assert_eq!(field(&b), ("email", "ana@acme.dev".into()));
            }
        }

        #[test]
        fn disconnect_asks_to_forget_the_token() {
            let mut b = on_shortcut();
            b.connected(Source::Shortcut, Connection { from_env: false, account: None });
            let pos = button(&b, "disconnect");
            assert_eq!(click(&mut b, pos), Action::Disconnect(Source::Shortcut));
        }
    }

    mod buttons {
        use super::*;

        fn reading() -> Browser {
            let mut b = with_lists();
            press(&mut b, KeyCode::Enter);
            let body = "# Title\n\n    indented **bold**".to_string();
            b.read_done(Source::Shortcut, "sc-2", Ok(Detail { body, ..Detail::default() }));
            b
        }

        fn body_text(b: &Browser) -> Vec<String> {
            let lines: Vec<String> =
                b.detail_lines(AREA).iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect();
            let prompt = lines.iter().position(|l| l.starts_with("prompt")).expect("the prompt row");
            lines[prompt + 2..].to_vec()
        }

        #[test]
        fn the_issue_offers_raw_copy_and_back_after_the_agent() {
            assert_eq!(reading().labels(), ["start", "agent…", "raw", "copy url", "back"]);
        }

        #[test]
        fn the_list_offers_refresh() {
            assert_eq!(with_lists().labels(), ["start", "refresh", "cancel"]);
        }

        #[test]
        fn refresh_reloads_what_the_tab_shows() {
            let mut b = with_lists();
            let pos = button(&b, "refresh");
            assert_eq!(click(&mut b, pos), Action::Load(vec![Source::Github, Source::Shortcut]));
        }

        #[test]
        fn raw_shows_the_markdown_as_written() {
            let mut b = reading();
            let pos = button(&b, "raw");
            click(&mut b, pos);
            assert_eq!(
                (b.labels().contains(&"rendered"), body_text(&b)),
                (true, vec!["# Title".to_string(), String::new(), "    indented **bold**".into()])
            );
        }

        #[test]
        fn rendered_goes_back_to_the_formatted_text() {
            let mut b = reading();
            let raw = button(&b, "raw");
            click(&mut b, raw);
            let rendered = button(&b, "rendered");
            click(&mut b, rendered);
            assert_eq!(body_text(&b)[0], "Title");
        }

        #[test]
        fn copy_asks_to_copy_the_url() {
            let mut b = reading();
            let pos = button(&b, "copy url");
            assert_eq!(click(&mut b, pos), Action::Copy("https://github.com/acme/shop/issues/2".into()));
        }

        #[test]
        fn a_notice_shows_until_the_next_key() {
            let mut b = reading();
            b.notice = Some("copied".into());
            let hint = |b: &Browser| match b.view(AREA, 0) {
                ui::Overlay::Issues(v) => v.hint,
                _ => String::new(),
            };
            let before = hint(&b);
            press(&mut b, KeyCode::Down);
            assert_eq!((before.as_str(), hint(&b) == "copied"), ("copied", false));
        }

        #[test]
        fn tabs_round_trip_through_their_id() {
            let tabs = [Tab::All, Tab::One(Source::Linear)];
            assert_eq!(tabs.map(|t| Tab::from_id(t.id())), tabs.map(Some));
        }
    }

    mod agent {
        use super::*;

        fn reading(default: Option<&str>) -> Browser {
            let mut b = Browser { agents: agents(default), ..with_lists() };
            press(&mut b, KeyCode::Enter);
            b
        }

        fn line_with(b: &Browser, label: &str) -> Line<'static> {
            b.detail_lines(AREA)
                .into_iter()
                .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == label))
                .expect("the row is there")
        }

        #[test]
        fn start_uses_the_default_agent() {
            let mut b = reading(Some("claude"));
            assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(_, kind, _) if kind == "claude"));
        }

        #[test]
        fn the_agent_running_in_the_tab_is_used_when_the_default_is_auto() {
            let mut b = reading(None);
            b.agents.running = Some("codex".into());
            assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(_, kind, _) if kind == "codex"));
        }

        #[test]
        fn with_nothing_decided_start_asks_which_agent() {
            let mut b = reading(None);
            let action = press(&mut b, KeyCode::Enter);
            assert_eq!((action, b.picker.as_ref().map(|p| p.pick)), (Action::None, Some(Pick::Agent)));
        }

        #[test]
        fn the_picked_agent_takes_the_issue() {
            let mut b = reading(None);
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "cod");
            press(&mut b, KeyCode::Enter);
            assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(_, kind, _) if kind == "codex"));
        }

        #[test]
        fn another_agent_can_become_the_default() {
            let mut b = reading(Some("claude"));
            b.agents.chosen = Some("codex".into());
            let pos = button(&b, "make default");
            assert_eq!(click(&mut b, pos), Action::SetDefaultAgent("codex".into()));
        }

        #[test]
        fn the_issue_shows_how_the_agent_starts() {
            let b = reading(Some("claude"));
            let text: String = line_with(&b, "agent").spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(
                text,
                "agent   claude · skip permissions (dangerous)  claude --dangerously-skip-permissions  (default)"
            );
        }

        #[test]
        fn a_dangerous_mode_is_red() {
            let b = reading(Some("claude"));
            let line = line_with(&b, "agent");
            let mode = line.spans.iter().find(|s| s.content.contains("dangerous")).expect("the mode");
            assert_eq!(mode.style.fg, Some(Color::Red));
        }

        #[test]
        fn the_issue_shows_the_prompt_and_how_it_is_sent() {
            let b = reading(Some("claude"));
            let text: String = line_with(&b, "prompt").spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(text, "prompt  https://github.com/acme/shop/issues/2  (typed, you press enter)");
        }
    }

    mod place {
        use super::*;

        fn two_places() -> Browser {
            let mut b = Browser { places: vec![shop(), notes()], ..with_lists() };
            press(&mut b, KeyCode::Enter);
            b
        }

        #[test]
        fn a_story_asks_where_first_with_the_current_project_preselected() {
            let mut b = two_places();
            let first = press(&mut b, KeyCode::Enter);
            let started = press(&mut b, KeyCode::Enter);
            assert_eq!(first, Action::None);
            assert!(matches!(&started, Action::Start(_, _, Some(place)) if *place == shop()), "{started:?}");
        }

        #[test]
        fn another_place_can_be_picked() {
            let mut b = two_places();
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "notes");
            let started = press(&mut b, KeyCode::Enter);
            assert!(matches!(&started, Action::Start(_, _, Some(place)) if *place == notes()), "{started:?}");
        }

        #[test]
        fn the_issue_shows_where_it_starts() {
            let mut b = two_places();
            b.place = Some(1);
            let start: String = b
                .detail_lines(AREA)
                .into_iter()
                .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == "start"))
                .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                .expect("the start row");
            assert_eq!(start, "start   in a new tab in notes › default");
        }

        #[test]
        fn where_is_offered_for_stories_only() {
            let mut b = two_places();
            let story = b.labels();
            press(&mut b, KeyCode::Esc);
            press(&mut b, KeyCode::Down);
            press(&mut b, KeyCode::Enter);
            assert_eq!((story.contains(&"where…"), b.labels().contains(&"where…")), (true, false));
        }

        #[test]
        fn a_github_issue_starts_in_its_repository() {
            let mut b = two_places();
            press(&mut b, KeyCode::Esc);
            press(&mut b, KeyCode::Down);
            press(&mut b, KeyCode::Enter);
            assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(i, _, None) if i.key == "#1"));
        }
    }

    mod people {
        use super::*;

        fn toggle() -> Position {
            let b = with_lists();
            let (labels, _) = b.toggles();
            let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
            ui::issue_toggles(ui::issues_area(AREA), &labels)[1].as_position()
        }

        fn filtering() -> Browser {
            let mut b = with_lists();
            click(&mut b, toggle());
            b
        }

        #[test]
        fn the_toggle_opens_the_filter_and_asks_for_the_people() {
            let mut b = with_lists();
            let action = click(&mut b, toggle());
            assert_eq!((action, b.filtering), (Action::LoadPeople(vec![Source::Github, Source::Shortcut]), Some(0)));
        }

        #[test]
        fn each_source_has_its_two_fields() {
            let b = filtering();
            let ui::Overlay::Issues(view) = b.view(AREA, 0) else { panic!("not the issues") };
            let ui::IssuesBody::List { items, .. } = view.body else { panic!("not a list") };
            let rows: Vec<String> = items.iter().map(|i| format!("{} {}", i.key, i.title)).collect();
            assert_eq!(
                rows,
                [
                    "GitHub assignee anyone",
                    "GitHub author anyone",
                    "Shortcut owner anyone",
                    "Shortcut requester anyone"
                ]
            );
        }

        #[test]
        fn me_filters_that_source_and_reloads_it_when_done() {
            let mut b = filtering();
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "me");
            press(&mut b, KeyCode::Enter);
            let done = press(&mut b, KeyCode::Esc);
            assert_eq!((b.people.github[0].clone(), done), (Who::Me, Action::Load(vec![Source::Github])));
        }

        #[test]
        fn members_can_be_picked_by_name() {
            let mut b = filtering();
            b.people_loaded(
                Source::Shortcut,
                Ok(vec![Person { handle: "bo".into(), name: "Bo Diddley".into(), id: None }]),
            );
            press(&mut b, KeyCode::Down);
            press(&mut b, KeyCode::Down);
            press(&mut b, KeyCode::Down);
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "diddley");
            press(&mut b, KeyCode::Enter);
            assert_eq!(b.people.shortcut[1], Who::Person("bo".into()));
        }

        #[test]
        fn jira_people_are_picked_by_account_even_with_the_same_name() {
            let mut b = browser();
            b.tab_to(Tab::One(Source::Jira));
            b.connected(Source::Jira, Connection { from_env: false, account: None });
            b.open_people();
            let twin = |id: &str, email: &str| Person { handle: "Ana".into(), name: email.into(), id: Some(id.into()) };
            b.people_loaded(Source::Jira, Ok(vec![twin("a1", "ana@one.dev"), twin("a2", "ana@two.dev")]));
            press(&mut b, KeyCode::Enter);
            type_text(&mut b, "two");
            press(&mut b, KeyCode::Enter);
            assert_eq!(b.people.jira[0], Who::User { id: "a2".into(), name: "Ana".into() });
            assert_eq!(b.people.describe(Source::Jira), "assignee @Ana");
        }

        #[test]
        fn the_toggle_says_who_filters() {
            let mut b = with_lists();
            b.people.shortcut[0] = Who::Me;
            assert_eq!(
                b.toggles(),
                (vec!["closed".to_string(), "people: Shortcut owner me".into()], vec![false, true])
            );
        }

        #[test]
        fn clear_goes_back_to_anyone() {
            let mut b = filtering();
            b.people.github = [Who::Me, Who::Person("x".into())];
            let pos = button(&b, "clear");
            click(&mut b, pos);
            assert_eq!(b.people, People::default());
        }

        #[test]
        fn the_query_of_a_source_carries_its_people() {
            let mut b = with_lists();
            b.people.linear = [Who::Anyone, Who::Me];
            assert_eq!(b.query(Source::Linear), Query { closed: false, people: [Who::Anyone, Who::Me] });
        }
    }

    #[test]
    fn nothing_reacts_while_starting() {
        let mut b = with_lists();
        b.starting = true;
        assert_eq!(press(&mut b, KeyCode::Esc), Action::None);
    }

    #[test]
    fn the_start_button_starts_the_selected_issue() {
        let mut b = with_lists();
        let pos = button(&b, "start");
        assert!(matches!(click(&mut b, pos), Action::Start(i, _, _) if i.key == "sc-2"));
    }
}
