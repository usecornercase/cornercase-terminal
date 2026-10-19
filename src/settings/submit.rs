use crate::agents;
use crate::config::{self, Config};
use crate::issues::{Secret, Source, asana, gitlab, jira, naming, plane};

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
            Row::DefaultAgent => {
                let notice = format!("default agent: {value}");
                self.save(Config { agent: value, ..self.config.clone() }, notice)
            }
            Row::Sidebar => {
                let notice = format!("sidebar: {value}");
                self.save(Config { sidebar: value, ..self.config.clone() }, notice)
            }
            Row::Tabs => {
                let notice = format!("tabs: {value}");
                self.save(Config { tabs: value, ..self.config.clone() }, notice)
            }
            Row::WorkingIcon => {
                let notice = format!("working icon: {value}");
                self.save(Config { working_icon: value, ..self.config.clone() }, notice)
            }
            Row::Notifications => {
                let notice = format!("desktop notifications: {value}");
                self.save(Config { desktop_notifications: value, ..self.config.clone() }, notice)
            }
            Row::Kind(kind) if value == EXTRA_ARGS => {
                self.edit_extra(&kind);
                Action::None
            }
            Row::Kind(kind) => {
                let modes = agents::modes(&self.config, &kind);
                let mode = (value != "default").then_some(value.as_str());
                let args = agents::with_mode(&kind, &agents::args(&self.config, &kind), &modes, mode);
                let notice = starts_with(&kind, &args);
                self.save(with_args(&self.config, &kind, args), notice)
            }
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
            self.submit_checked(asana::check_workspace, |config, value| {
                let notice = if value.is_empty() {
                    "the Asana workspace was cleared".into()
                } else {
                    format!("Asana workspace: {value}")
                };
                (Config { asana_workspace: value, ..config.clone() }, notice)
            })
        } else {
            self.submit_checked(asana::check_projects, |config, value| {
                let notice =
                    if value.is_empty() { "Asana lists your tasks".into() } else { format!("Asana projects: {value}") };
                (Config { asana_projects: value, ..config.clone() }, notice)
            })
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
        let Some(edit) = &mut self.edit else { return Action::None };
        match edit.row.clone() {
            Row::Folder => self.submit_folder(),
            Row::IssueBranch => self.submit_issue_branch(),
            Row::Fetch => match config::check_fetch_minutes(&edit.input) {
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
            },
            Row::Token(Source::Jira) if self.config.jira_site.is_empty() || self.config.jira_email.is_empty() => {
                edit.error = Some("set the Jira site and email first".into());
                Action::None
            }
            Row::Token(Source::Plane) if self.config.plane_workspace.is_empty() => {
                edit.error = Some("set the Plane workspace first".into());
                Action::None
            }
            Row::Token(source) => {
                let token = edit.input.trim().to_string();
                if token.is_empty() {
                    edit.error = Some(format!("paste the {} first", source.token_name()));
                    return Action::None;
                }
                edit.error = None;
                self.checking.push(source);
                Action::CheckToken(source, Secret(token))
            }
            Row::GitlabUrl => self.submit_checked(gitlab::check_url, gitlab_url),
            Row::JiraSite => self.submit_checked(jira::check_site, |config, value| {
                let notice =
                    if value.is_empty() { "the Jira site was cleared".into() } else { format!("Jira site: {value}") };
                (Config { jira_site: value, ..config.clone() }, notice)
            }),
            Row::JiraEmail => self.submit_checked(jira::check_email, |config, value| {
                let notice =
                    if value.is_empty() { "the Jira email was cleared".into() } else { format!("Jira email: {value}") };
                (Config { jira_email: value, ..config.clone() }, notice)
            }),
            Row::PlaneWorkspace => self.submit_checked(plane::check_workspace, |config, value| {
                let notice = if value.is_empty() {
                    "the Plane workspace was cleared".into()
                } else {
                    format!("Plane workspace: {value}")
                };
                (Config { plane_workspace: value, ..config.clone() }, notice)
            }),
            Row::PlaneUrl => self.submit_checked(plane::check_url, |config, value| {
                let notice = if value.is_empty() {
                    "Plane is the cloud at app.plane.so".into()
                } else {
                    format!("Plane server: {value}")
                };
                (Config { plane_url: value, ..config.clone() }, notice)
            }),
            Row::PlaneFilter => self
                .submit_filter("Plane", "work item", |config, plane_filter| Config { plane_filter, ..config.clone() }),
            row @ (Row::AsanaWorkspace | Row::AsanaProjects) => self.submit_asana(&row),
            row @ (Row::SoundWaiting | Row::SoundDone) => self.submit_sound(&row),
            Row::JiraJql => {
                self.submit_filter("Jira", "issue", |config, jira_jql| Config { jira_jql, ..config.clone() })
            }
            Row::Bot(via) => {
                let token = edit.input.trim().to_string();
                if token.is_empty() {
                    edit.error = Some("paste the bot's token first".into());
                    return Action::None;
                }
                edit.error = None;
                self.messenger_mut(via).checking = true;
                Action::CheckBot(via, Secret(token))
            }
            Row::Kind(kind) => {
                let extra = agents::split_args(&edit.input);
                self.edit = None;
                let modes = agents::modes(&self.config, &kind);
                let args = agents::with_extra(&kind, &agents::args(&self.config, &kind), &modes, extra);
                let notice = starts_with(&kind, &args);
                self.save(with_args(&self.config, &kind, args), notice)
            }
            _ => self.submit_account(),
        }
    }
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
