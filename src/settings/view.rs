use crate::agents;
use crate::config::{Config, Sidebar, Tabs};
use crate::shortcuts::Prefix;

use super::{Page, Row, RowText, Settings, Status};

const RESUME_NOTE: &str = "Claude Code, Codex and Gemini, in their tabs";

impl Settings {
    fn keep_awake_row(&self) -> (String, String, String, bool) {
        let value = if self.config.keep_awake { "[x] while an agent works" } else { "[ ] never" };
        ("keep awake".into(), value.into(), self.awake.clone(), false)
    }

    pub fn row_text(&self, row: &Row) -> RowText {
        let config = &self.config;
        let (label, value, note, dangerous) = match row {
            Row::Folder => (
                "worktrees folder".to_string(),
                config.worktrees_dir.clone(),
                "new worktrees go in <folder>/<repo>/<branch>".to_string(),
                false,
            ),
            Row::Fetch => {
                let value =
                    if config.fetch_minutes == 0 { "off".into() } else { format!("{} min", config.fetch_minutes) };
                ("fetch branches every".into(), value, "commits to pull show as ↓n".into(), false)
            }
            Row::Token(source) => {
                let label = format!("{} {}", source.name(), source.token_name());
                let (value, note) = match self.status(*source) {
                    Some(Status::Saved(Some(account))) => (account.describe(), "saved".to_string()),
                    Some(Status::Saved(None)) => ("connected".into(), "saved".into()),
                    Some(Status::Env) => {
                        ("connected".into(), format!("from {}", source.token_env().unwrap_or_default()))
                    }
                    _ => ("not connected".into(), "enter pastes one".into()),
                };
                (label, value, note, false)
            }
            Row::JiraSite | Row::JiraEmail | Row::JiraJql => jira_row(config, row),
            Row::PlaneWorkspace | Row::PlaneUrl | Row::PlaneFilter => plane_row(config, row),
            Row::Tab(id) => self.tab_row(id),
            Row::DefaultAgent => {
                let note = if config.agent == agents::AUTO {
                    "the agent in your tab, otherwise ask".into()
                } else {
                    String::new()
                };
                ("default agent".into(), config.agent.clone(), note, false)
            }
            Row::Submit => {
                let value = if config.submit { "[x] sent for you" } else { "[ ] typed, you press Enter" };
                ("send the prompt".into(), value.into(), String::new(), false)
            }
            Row::AgentsSection => {
                let value = if config.agents_section { "[x] shown" } else { "[ ] hidden" };
                ("agents section".into(), value.into(), "every running agent in the sidebar".into(), false)
            }
            Row::Counts => {
                let value = if config.counts { "[x] shown" } else { "[ ] hidden" };
                ("counts".into(), value.into(), "what a project or collapsed group holds, such as (3)".into(), false)
            }
            Row::TodoDot => {
                let value = if config.todo_dot { "[x] shown" } else { "[ ] hidden" };
                ("TODO dot".into(), value.into(), "while items are pending".into(), false)
            }
            Row::DimPanes => {
                let value = if config.dim_inactive_panes { "[x] dimmed" } else { "[ ] as bright as the active one" };
                ("inactive panes".into(), value.into(), "in a split tab".into(), false)
            }
            Row::Detail(detail) => {
                let value = if detail.on(config) { "[x] shown" } else { "[ ] hidden" };
                (detail.label().into(), value.into(), detail.note().into(), false)
            }
            Row::Sidebar | Row::Tabs => layout_row(config, row),
            Row::Notifications => (
                "desktop notifications".into(),
                config.desktop_notifications.clone(),
                "when an agent in another tab needs you or finishes".into(),
                false,
            ),
            Row::Updates => {
                let value = if config.check_updates { "[x] every hour" } else { "[ ] never" };
                ("check for updates".into(), value.into(), "asks GitHub for the latest release".into(), false)
            }
            Row::Trust => {
                let value = if config.accept_trust_prompts { "[x] accepted for you" } else { "[ ] left to you" };
                ("trust prompts".into(), value.into(), "saying yes runs the repo's agent config".into(), false)
            }
            Row::Resume => {
                let value = if config.resume_agents { "[x] after a restart" } else { "[ ] never" };
                ("resume conversations".into(), value.into(), RESUME_NOTE.into(), false)
            }
            Row::KeepAwake => self.keep_awake_row(),
            Row::Prefix => self.prefix_row(),
            Row::Remote => {
                let value = if config.remote { "[x] over Tailscale" } else { "[ ] off" };
                ("remote access".into(), value.into(), self.remote.clone(), false)
            }
            Row::Browsers => {
                ("paired browsers".into(), self.browsers.to_string(), "enter forgets them all".into(), true)
            }
            Row::Kind(kind) => kind_row(config, kind),
            Row::AddAgent => ("+ another agent…".into(), String::new(), String::new(), false),
        };
        RowText { label, value, note, dangerous }
    }

    fn tab_row(&self, id: &str) -> (String, String, String, bool) {
        let on = self.shown_tabs().contains(&id);
        let name = match id {
            "all" => "All",
            "github" => "GitHub",
            "shortcut" => "Shortcut",
            "linear" => "Linear",
            "jira" => "Jira",
            _ => "Plane",
        };
        let note = if id == "all" { "every source together".into() } else { String::new() };
        (format!("{} {name}", if on { "[x]" } else { "[ ]" }), String::new(), note, false)
    }

    fn prefix_row(&self) -> (String, String, String, bool) {
        let prefix = self.config.prefix();
        let clash = prefix.and_then(Prefix::clash);
        let value = match prefix {
            _ if self.capturing => "press a key…".to_string(),
            Some(prefix) => prefix.to_string(),
            None => "off".to_string(),
        };
        let note = clash.map_or_else(|| "opens a menu of keyboard shortcuts".to_string(), |c| format!("also {c}"));
        ("prefix key".into(), value, note, clash.is_some())
    }

    pub fn edit_field(&self) -> Option<(String, String)> {
        self.edit.as_ref().map(|edit| {
            let token = matches!(edit.row, Row::Token(_));
            let label = match &edit.row {
                Row::Folder => "worktrees folder".to_string(),
                Row::Fetch => "fetch branches every (minutes, 0 turns it off)".to_string(),
                Row::Token(source) => format!("{} {}", source.name(), source.token_name()),
                Row::JiraSite => "Jira site, such as acme.atlassian.net".to_string(),
                Row::JiraEmail => "Jira email".to_string(),
                Row::JiraJql => "Jira filter (JQL, such as project = SHOP; empty lists everything)".to_string(),
                Row::PlaneWorkspace => "Plane workspace, the part after app.plane.so/, such as acme".to_string(),
                Row::PlaneUrl => "Plane URL for a self-hosted server (empty is Plane Cloud)".to_string(),
                Row::PlaneFilter => "Plane filter (query parameters, such as priority=high)".to_string(),
                Row::Kind(kind) => format!("{kind} extra arguments"),
                _ => String::new(),
            };
            let value = if token { "•".repeat(edit.input.chars().count()) } else { edit.input.clone() };
            (label, value)
        })
    }

    pub fn pick_title(&self) -> String {
        let Some(pick) = &self.pick else { return String::new() };
        match &pick.row {
            Row::DefaultAgent => "Which agent takes an issue by default?".to_string(),
            Row::Sidebar => "How should projects, workspaces and tabs be laid out?".to_string(),
            Row::Tabs => "Where should a workspace's tabs go?".to_string(),
            Row::Notifications => "How should your terminal notify you?".to_string(),
            Row::Kind(kind) => format!("How should {kind} start?"),
            _ => "Which agent do you want to set up?".to_string(),
        }
    }

    pub fn pick_selected(&self) -> Option<usize> {
        let pick = self.pick.as_ref()?;
        let choices = self.pick_choices();
        choices.len().checked_sub(1).map(|last| pick.search.selected().min(last))
    }

    pub fn hint(&self) -> String {
        self.notice.clone().unwrap_or_else(|| {
            if self.capturing {
                "press the keys you want · backspace turns them off · esc cancels".into()
            } else if self.edit.is_some() {
                "enter saves · esc cancels".into()
            } else if self.pick.is_some() {
                "enter picks · type to filter · esc goes back".into()
            } else if self.page == Page::Issues {
                "enter changes the selected setting · shift+↑↓ moves a source · tab or ←→ switches tabs".into()
            } else {
                "enter changes the selected setting · tab or ←→ switches tabs · every change is saved at once".into()
            }
        })
    }
}

fn layout_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, note) = if *row == Row::Tabs {
        ("tabs", Tabs::from_setting(&config.tabs).id(), "where a workspace's tabs are listed")
    } else {
        ("sidebar", Sidebar::from_setting(&config.sidebar).id(), "how projects, workspaces and tabs are laid out")
    };
    (label.into(), value.into(), note.into(), false)
}

fn jira_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, empty, note) = match row {
        Row::JiraSite => ("Jira site", &config.jira_site, "not set", "such as acme.atlassian.net"),
        Row::JiraEmail => ("Jira email", &config.jira_email, "not set", "the one you sign in with"),
        _ => ("Jira filter", &config.jira_jql, "none", "JQL, such as project = SHOP"),
    };
    let value = if value.is_empty() { empty.to_string() } else { value.clone() };
    (label.into(), value, note.into(), false)
}

fn plane_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, empty, note) = match row {
        Row::PlaneWorkspace => ("Plane workspace", &config.plane_workspace, "not set", "the part after app.plane.so/"),
        Row::PlaneUrl => ("Plane URL", &config.plane_url, "Plane Cloud", "for a self-hosted server"),
        _ => ("Plane filter", &config.plane_filter, "none", "query parameters, such as priority=high"),
    };
    let value = if value.is_empty() { empty.to_string() } else { value.clone() };
    (label.into(), value, note.into(), false)
}

fn kind_row(config: &Config, kind: &str) -> (String, String, String, bool) {
    let args = agents::args(config, kind);
    let modes = agents::modes(config, kind);
    let mode = agents::mode_of(kind, &args, &modes);
    let extra = agents::extra_args(kind, &args, &modes);
    let mut value = mode.clone().unwrap_or_else(|| "default".into());
    if !extra.is_empty() {
        value = format!("{value} + {}", agents::join_args(&extra));
    }
    let is_default = agents::resolve(config, None, None).as_deref() == Some(kind);
    let note = if is_default { "the default agent".into() } else { String::new() };
    (kind.to_string(), value, note, mode.as_deref().is_some_and(agents::is_dangerous))
}
