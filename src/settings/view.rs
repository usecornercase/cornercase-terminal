use crate::agents;
use crate::config::{Config, Sidebar, Tabs, WorkingIcon};
use crate::issues::Source;
use crate::messenger::Via;
use crate::shortcuts::Prefix;

use super::{Page, Row, RowText, Settings, Status};

const RESUME_NOTE: &str = "Claude, Codex, Gemini, Grok, Cursor, in their tabs";

const ICON_BAR_NOTE: &str = "the buttons under the lists, as icons; needs a Nerd Font";

impl Settings {
    fn keep_awake_row(&self) -> (String, String, String, bool) {
        let value = if self.config.keep_awake { "[x] while an agent works" } else { "[ ] never" };
        ("keep awake".into(), value.into(), self.awake.clone(), false)
    }

    fn worktrees_row(&self, row: &Row) -> (String, String, String, bool) {
        let config = &self.config;
        match row {
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
            Row::MergedPullRequests => {
                let value = if config.merged_pull_requests { "[x] shown" } else { "[ ] hidden" };
                ("merged PRs in stats".into(), value.into(), self.merged.clone(), false)
            }
            _ => {
                let value = if config.pull_requests { "[x] shown" } else { "[ ] hidden" };
                ("pull requests".into(), value.into(), self.pulls.clone(), false)
            }
        }
    }

    fn messenger_row(&self, row: &Row, via: Via) -> (String, String, String, bool) {
        let messenger = self.messenger(via);
        if *row == Row::Bot(via) {
            let note = if messenger.checking { "checking…".into() } else { messenger.note.clone() };
            return ("bot".into(), messenger.bot.clone().unwrap_or_else(|| "not set".into()), note, false);
        }
        let (value, note) = match &messenger.chat {
            Some(chat) => (chat.clone(), "agents message it while you are away · enter pairs another"),
            None if via.pairs_by_owner() => ("not paired".into(), "enter shows a link to add the app to your account"),
            None => ("not paired".into(), "enter shows a code to scan with your phone"),
        };
        ("phone".into(), value, note.into(), false)
    }

    pub(crate) fn row_text(&self, row: &Row) -> RowText {
        let config = &self.config;
        let (label, value, note, dangerous) = match row {
            Row::Folder | Row::Fetch | Row::PullRequests | Row::MergedPullRequests => self.worktrees_row(row),
            Row::Token(source) => self.token_row(*source),
            Row::GitlabUrl
            | Row::JiraSite
            | Row::JiraEmail
            | Row::JiraJql
            | Row::PlaneWorkspace
            | Row::PlaneUrl
            | Row::PlaneFilter => tracker_row(config, row),
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
            Row::WorkingIcon => (
                "working icon".into(),
                WorkingIcon::from_setting(&config.working_icon).id().into(),
                "on the tab of an agent that is working".into(),
                false,
            ),
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
            Row::Sidebar | Row::Tabs | Row::IconBar => layout_row(config, row),
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
            Row::StarPrompt => {
                let value = if config.star_prompt { "[x] now and then" } else { "[ ] never" };
                ("star prompt".into(), value.into(), "asks for a GitHub star after two weeks of use".into(), false)
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
            Row::Flows => {
                ("flows".into(), self.flows.clone(), "teams of agents that work through a plan".into(), false)
            }
            Row::Bot(via) | Row::Chat(via) => self.messenger_row(row, *via),
        };
        RowText { label, value, note, dangerous }
    }

    fn token_row(&self, source: Source) -> (String, String, String, bool) {
        let label = format!("{} {}", source.name(), source.token_name());
        let (value, note) = match self.status(source) {
            Some(Status::Saved(Some(account))) => (account.describe(), "saved".to_string()),
            Some(Status::Saved(None)) => ("connected".into(), "saved".into()),
            Some(Status::Env) => ("connected".into(), format!("from {}", source.token_env().unwrap_or_default())),
            _ => ("not connected".into(), "enter pastes one".into()),
        };
        (label, value, note, false)
    }

    fn tab_row(&self, id: &str) -> (String, String, String, bool) {
        let on = self.shown_tabs().contains(&id);
        let name = match id {
            "all" => "All",
            "github" => "GitHub",
            "gitlab" => "GitLab",
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

    pub(crate) fn edit_field(&self) -> Option<(String, String)> {
        self.edit.as_ref().map(|edit| {
            let token = matches!(edit.row, Row::Token(_) | Row::Bot(_));
            let label = match &edit.row {
                Row::Folder => "worktrees folder".to_string(),
                Row::Fetch => "fetch branches every (minutes, 0 turns it off)".to_string(),
                Row::Token(source) => format!("{} {}", source.name(), source.token_name()),
                Row::GitlabUrl => "GitLab URL for a self-hosted server (empty is gitlab.com)".to_string(),
                Row::JiraSite => "Jira site, such as acme.atlassian.net".to_string(),
                Row::JiraEmail => "Jira email".to_string(),
                Row::JiraJql => "Jira filter (JQL, such as project = SHOP; empty lists everything)".to_string(),
                Row::PlaneWorkspace => "Plane workspace, the part after app.plane.so/, such as acme".to_string(),
                Row::PlaneUrl => "Plane URL for a self-hosted server (empty is Plane Cloud)".to_string(),
                Row::PlaneFilter => "Plane filter (query parameters, such as priority=high)".to_string(),
                Row::Kind(kind) => format!("{kind} extra arguments"),
                Row::Bot(via) => via.token_field().to_string(),
                _ => String::new(),
            };
            let value = if token { "•".repeat(edit.input.chars().count()) } else { edit.input.clone() };
            (label, value)
        })
    }

    pub(crate) fn pick_title(&self) -> String {
        let Some(pick) = &self.pick else { return String::new() };
        match &pick.row {
            Row::DefaultAgent => "Which agent takes an issue by default?".to_string(),
            Row::Sidebar => "How should projects, workspaces and tabs be laid out?".to_string(),
            Row::Tabs => "Where should a workspace's tabs go?".to_string(),
            Row::WorkingIcon => "What should a working agent's tab show?".to_string(),
            Row::Notifications => "How should your terminal notify you?".to_string(),
            Row::Kind(kind) => format!("How should {kind} start?"),
            _ => "Which agent do you want to set up?".to_string(),
        }
    }

    pub(crate) fn pick_selected(&self) -> Option<usize> {
        let pick = self.pick.as_ref()?;
        let choices = self.pick_choices();
        choices.len().checked_sub(1).map(|last| pick.search.selected().min(last))
    }

    pub(crate) fn hint(&self) -> String {
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
    let (label, value, note) = match row {
        Row::Tabs => ("tabs", Tabs::from_setting(&config.tabs).id(), "where a workspace's tabs are listed"),
        Row::IconBar => {
            let value = if config.icon_bar { "[x] on the left" } else { "[ ] off" };
            ("icon bar", value, ICON_BAR_NOTE)
        }
        _ => ("sidebar", Sidebar::from_setting(&config.sidebar).id(), "how projects, workspaces and tabs are laid out"),
    };
    (label.into(), value.into(), note.into(), false)
}

fn tracker_row(config: &Config, row: &Row) -> (String, String, String, bool) {
    let (label, value, empty, note) = match row {
        Row::GitlabUrl => ("GitLab URL", &config.gitlab_url, "gitlab.com", "for a self-hosted server"),
        Row::JiraSite => ("Jira site", &config.jira_site, "not set", "such as acme.atlassian.net"),
        Row::JiraEmail => ("Jira email", &config.jira_email, "not set", "the one you sign in with"),
        Row::JiraJql => ("Jira filter", &config.jira_jql, "none", "JQL, such as project = SHOP"),
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
