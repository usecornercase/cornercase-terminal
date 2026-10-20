use crate::agents;
use crate::config::{self, Config};
use crate::issues::{Secret, Source, asana, gitlab, jira, naming, plane};
use crate::messenger::Via;

use super::{Action, EXTRA_ARGS, PickItem, Row, Settings};

impl Settings {
    pub(crate) fn pick_choices(&self) -> Vec<&PickItem> {
        let Some(pick) = &self.pick else { return Vec::new() };
        let needle = pick.search.query().trim().to_lowercase();
        pick.items.iter().filter(|item| item.value.to_lowercase().contains(&needle)).collect()
    }

    pub(crate) fn choose(&mut self, index: Option<usize>) -> Action {
        let Some(pick) = &self.pick else { return Action::None };
        let choices = self.pick_choices();
        let i = index.unwrap_or_else(|| pick.search.selected()).min(choices.len().saturating_sub(1));
        let Some(value) = choices.get(i).map(|item| item.value.clone()) else { return Action::None };
        let row = pick.row.clone();
        self.pick = None;
        match row {
            Row::DefaultAgent => self.chosen("default agent", value, |c, agent| Config { agent, ..c.clone() }),
            Row::Sidebar => self.chosen("sidebar", value, |c, sidebar| Config { sidebar, ..c.clone() }),
            Row::Tabs => self.chosen("tabs", value, |c, tabs| Config { tabs, ..c.clone() }),
            Row::WorkingIcon => {
                self.chosen("working icon", value, |c, working_icon| Config { working_icon, ..c.clone() })
            }
            Row::Notifications => self.chosen("desktop notifications", value, |c, desktop_notifications| Config {
                desktop_notifications,
                ..c.clone()
            }),
            Row::Kind(kind) => self.chosen_mode(&kind, &value),
            Row::AddAgent => {
                self.added.push(value.clone());
                if let Some(i) = self.rows().iter().position(|r| *r == Row::Kind(value.clone())) {
                    self.cursor = i;
                }
                self.activate()
            }
            _ => Action::None,
        }
    }

    fn chosen(&mut self, label: &str, value: String, apply: fn(&Config, String) -> Config) -> Action {
        let notice = format!("{label}: {value}");
        let config = apply(&self.config, value);
        self.save(config, notice)
    }

    fn chosen_mode(&mut self, kind: &str, value: &str) -> Action {
        if value == EXTRA_ARGS {
            self.edit_extra(kind);
            return Action::None;
        }
        let modes = agents::modes(&self.config, kind);
        let mode = (value != "default").then_some(value);
        let args = agents::with_mode(kind, &agents::args(&self.config, kind), &modes, mode);
        let notice = starts_with(kind, &args);
        self.save(with_args(&self.config, kind, args), notice)
    }

    fn submit_checked(
        &mut self,
        check: fn(&str) -> Result<String, &'static str>,
        apply: impl FnOnce(&Config, String) -> (Config, String),
    ) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let input = edit.input.trim();
        let checked = if input.is_empty() { Ok(String::new()) } else { check(input) };
        match checked {
            Ok(value) => {
                self.edit = None;
                let (config, notice) = apply(&self.config, value);
                self.save(config, notice)
            }
            Err(message) => {
                edit.error = Some(message.into());
                Action::None
            }
        }
    }

    fn submit_filter(&mut self, tracker: &str, item: &str, apply: impl FnOnce(&Config, String) -> Config) -> Action {
        let Some(edit) = self.edit.take() else { return Action::None };
        let filter = edit.input.trim().to_string();
        let notice = if filter.is_empty() {
            format!("{tracker} lists every {item} you can see")
        } else {
            format!("{tracker} lists only: {filter}")
        };
        let config = apply(&self.config, filter);
        self.save(config, notice)
    }

    fn submit_asana(&mut self, row: &Row) -> Action {
        if *row == Row::AsanaWorkspace {
            self.submit_checked(asana::check_workspace, asana_workspace)
        } else {
            self.submit_checked(asana::check_projects, asana_projects)
        }
    }

    fn submit_sound(&mut self, row: &Row) -> Action {
        if *row == Row::SoundWaiting {
            self.submit_checked(config::check_sound_file, |config, value| {
                let notice = sound_notice("needs you", &value);
                (Config { sound_waiting: value, ..config.clone() }, notice)
            })
        } else {
            self.submit_checked(config::check_sound_file, |config, value| {
                let notice = sound_notice("finishes", &value);
                (Config { sound_done: value, ..config.clone() }, notice)
            })
        }
    }

    fn submit_issue_branch(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        match naming::check(&edit.input) {
            Ok(issue_branch) => {
                self.edit = None;
                let notice = if issue_branch.is_empty() {
                    "issue branches follow each tracker's own name".into()
                } else {
                    format!("issue branches are named {issue_branch}")
                };
                self.save(Config { issue_branch, ..self.config.clone() }, notice)
            }
            Err(message) => {
                edit.error = Some(message);
                Action::None
            }
        }
    }

    pub(super) fn submit_edit(&mut self) -> Action {
        let Some(edit) = &self.edit else { return Action::None };
        match edit.row.clone() {
            Row::Folder => self.submit_folder(),
            Row::IssueBranch => self.submit_issue_branch(),
            Row::Fetch => self.submit_fetch(),
            Row::Token(source) => self.submit_token(source),
            Row::Bot(via) => self.submit_bot(via),
            Row::Kind(kind) => self.submit_kind(&kind),
            row => self.submit_field(&row),
        }
    }

    fn submit_field(&mut self, row: &Row) -> Action {
        match row {
            Row::GitlabUrl => self.submit_checked(gitlab::check_url, gitlab_url),
            Row::JiraSite => self.submit_checked(jira::check_site, jira_site),
            Row::JiraEmail => self.submit_checked(jira::check_email, jira_email),
            Row::PlaneWorkspace => self.submit_checked(plane::check_workspace, plane_workspace),
            Row::PlaneUrl => self.submit_checked(plane::check_url, plane_url),
            Row::PlaneFilter => self
                .submit_filter("Plane", "work item", |config, plane_filter| Config { plane_filter, ..config.clone() }),
            Row::AsanaWorkspace | Row::AsanaProjects => self.submit_asana(row),
            Row::SoundWaiting | Row::SoundDone => self.submit_sound(row),
            Row::JiraJql => {
                self.submit_filter("Jira", "issue", |config, jira_jql| Config { jira_jql, ..config.clone() })
            }
            _ => self.submit_account(),
        }
    }

    fn submit_folder(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        match config::check_worktrees_dir(&edit.input, self.home.as_deref()) {
            Ok(worktrees_dir) => {
                self.edit = None;
                let notice = format!("new worktrees go in {worktrees_dir}/<repo>/<branch>");
                self.save(Config { worktrees_dir, ..self.config.clone() }, notice)
            }
            Err(message) => {
                edit.error = Some(message.into());
                Action::None
            }
        }
    }

    fn submit_fetch(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        match config::check_fetch_minutes(&edit.input) {
            Ok(fetch_minutes) => {
                self.edit = None;
                let notice = if fetch_minutes == 0 {
                    "branches are not fetched: commits to pull are not shown".into()
                } else {
                    format!("branches are fetched every {fetch_minutes} min")
                };
                self.save(Config { fetch_minutes, ..self.config.clone() }, notice)
            }
            Err(message) => {
                edit.error = Some(message.into());
                Action::None
            }
        }
    }

    fn submit_token(&mut self, source: Source) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let config = &self.config;
        let missing = match source {
            Source::Jira if config.jira_site.is_empty() || config.jira_email.is_empty() => {
                Some("set the Jira site and email first".to_string())
            }
            Source::Plane if config.plane_workspace.is_empty() => Some("set the Plane workspace first".to_string()),
            _ => None,
        };
        if missing.is_some() {
            edit.error = missing;
            return Action::None;
        }
        let token = edit.input.trim().to_string();
        if token.is_empty() {
            edit.error = Some(format!("paste the {} first", source.token_name()));
            return Action::None;
        }
        edit.error = None;
        self.checking.push(source);
        Action::CheckToken(source, Secret(token))
    }

    fn submit_bot(&mut self, via: Via) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let token = edit.input.trim().to_string();
        if token.is_empty() {
            edit.error = Some("paste the bot's token first".into());
            return Action::None;
        }
        edit.error = None;
        self.messenger_mut(via).checking = true;
        Action::CheckBot(via, Secret(token))
    }

    fn submit_kind(&mut self, kind: &str) -> Action {
        let Some(edit) = &self.edit else { return Action::None };
        let extra = agents::split_args(&edit.input);
        self.edit = None;
        let modes = agents::modes(&self.config, kind);
        let args = agents::with_extra(kind, &agents::args(&self.config, kind), &modes, extra);
        let notice = starts_with(kind, &args);
        self.save(with_args(&self.config, kind, args), notice)
    }
}

fn cleared_or(value: &str, cleared: &str, label: &str) -> String {
    if value.is_empty() { cleared.into() } else { format!("{label}: {value}") }
}

fn jira_site(config: &Config, value: String) -> (Config, String) {
    let notice = cleared_or(&value, "the Jira site was cleared", "Jira site");
    (Config { jira_site: value, ..config.clone() }, notice)
}

fn jira_email(config: &Config, value: String) -> (Config, String) {
    let notice = cleared_or(&value, "the Jira email was cleared", "Jira email");
    (Config { jira_email: value, ..config.clone() }, notice)
}

fn plane_workspace(config: &Config, value: String) -> (Config, String) {
    let notice = cleared_or(&value, "the Plane workspace was cleared", "Plane workspace");
    (Config { plane_workspace: value, ..config.clone() }, notice)
}

fn plane_url(config: &Config, value: String) -> (Config, String) {
    let notice = cleared_or(&value, "Plane is the cloud at app.plane.so", "Plane server");
    (Config { plane_url: value, ..config.clone() }, notice)
}

fn asana_workspace(config: &Config, value: String) -> (Config, String) {
    let notice = cleared_or(&value, "the Asana workspace was cleared", "Asana workspace");
    (Config { asana_workspace: value, ..config.clone() }, notice)
}

fn asana_projects(config: &Config, value: String) -> (Config, String) {
    let notice = cleared_or(&value, "Asana lists your tasks", "Asana projects");
    (Config { asana_projects: value, ..config.clone() }, notice)
}

fn gitlab_url(config: &Config, value: String) -> (Config, String) {
    let notice = match value.as_str() {
        "" => "GitLab is gitlab.com".into(),
        url if gitlab::unencrypted(url) => format!("GitLab server: {url}; {}", gitlab::UNENCRYPTED),
        url => format!("GitLab server: {url}"),
    };
    (Config { gitlab_url: value, ..config.clone() }, notice)
}

fn sound_notice(when: &str, file: &str) -> String {
    if file.is_empty() {
        format!("an agent that {when} plays the built-in sound")
    } else {
        format!("an agent that {when} plays {file}")
    }
}

fn starts_with(kind: &str, args: &[String]) -> String {
    let args = if args.is_empty() { "no arguments".into() } else { agents::join_args(args) };
    format!("{kind} starts with: {args}")
}

fn with_args(config: &Config, kind: &str, args: Vec<String>) -> Config {
    let mut agent_args = config.agent_args.clone();
    if args.is_empty() {
        agent_args.remove(kind);
    } else {
        agent_args.insert(kind.to_string(), args);
    }
    Config { agent_args, ..config.clone() }
}
