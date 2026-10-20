use std::path::PathBuf;

use super::{Handled, Pending, Stage, json};
use crate::app::App;
use crate::config::{self, Config, ProjectSettings};
use crate::control::Done;
use crate::control::config::{Configure, Described, ProjectChange, ProjectEntry, SecretChange};
use crate::control::contract::{Code, Refusal};
use crate::error::Error;
use crate::git;
use crate::issues::{Account, Secret, Source, secrets};
use crate::project::Hierarchy;
use crate::settings::{Action, Page, Refused};
use crate::text;

fn refusal(refused: Refused) -> Refusal {
    match refused {
        Refused::Unknown(message) => Refusal::new(Code::NotFound, message),
        Refused::Invalid(message) => Refusal::new(Code::Malformed, message),
        Refused::Blocked(message) => Refusal::new(Code::Conflict, message),
    }
}

fn failed(message: impl Into<String>) -> Refusal {
    Refusal::new(Code::Failed, message)
}

impl App {
    pub(super) fn config_request(&mut self, client: u64, change: Configure) -> Handled {
        match change {
            Configure::Get => Ok(Some(json(&self.described(Vec::new())))),
            Configure::Set(values) => self.config_set(&values),
            Configure::Project(change) => self.config_project(change),
            Configure::Secret(secret) => self.config_secret(client, &secret),
            Configure::Remove(key) => self.config_remove(&key),
        }
    }

    fn described(&self, notices: Vec<String>) -> Described {
        let model = self.settings_model(Page::default());
        Described {
            revision: self.config_file.revision(),
            path: self.config_file.path().to_path_buf(),
            problem: self.config_file.problem().map(str::to_owned),
            settings: model.describe(),
            connections: model.connections(),
            projects: self.project_settings_entries(),
            notices,
        }
    }

    fn project_settings_entries(&self) -> Vec<ProjectEntry> {
        let home = self.home.as_deref();
        let open =
            |folder: &std::path::Path| self.projects.iter().find(|p| !p.closing && p.path == folder).map(|p| p.id);
        let mut entries: Vec<ProjectEntry> = self
            .config
            .projects
            .iter()
            .map(|(key, settings)| {
                let folder = config::expand_home(key, home);
                let folder = folder.canonicalize().unwrap_or(folder);
                ProjectEntry {
                    project: open(&folder),
                    repository: git::is_repo_root(&folder),
                    key: key.clone(),
                    setup: settings.setup.clone(),
                    teardown: settings.teardown.clone(),
                    folder,
                }
            })
            .collect();
        for project in self.projects.iter().filter(|p| !p.closing && git::is_repo_root(&p.path)) {
            if !entries.iter().any(|entry| entry.folder == project.path) {
                entries.push(ProjectEntry {
                    folder: project.path.clone(),
                    project: Some(project.id),
                    repository: true,
                    ..ProjectEntry::default()
                });
            }
        }
        entries
    }

    fn config_set(&mut self, values: &std::collections::BTreeMap<String, serde_json::Value>) -> Handled {
        self.readable_config()?;
        let mut model = self.settings_model(Page::default());
        let mut notices = Vec::new();
        for (key, value) in values {
            notices.extend(model.apply(key, value).map_err(refusal)?);
        }
        if model.config != self.config {
            self.write_config(&model.config).map_err(|e| self.config_refusal(e))?;
        }
        Ok(Some(json(&self.described(notices))))
    }

    pub(super) fn readable_config(&mut self) -> Result<(), Refusal> {
        self.reload_config().map_err(|e| Refusal::new(Code::Conflict, e))
    }

    pub(super) fn change_config_for(&mut self, change: impl FnOnce(&mut Config)) -> Result<(), Refusal> {
        self.readable_config()?;
        self.change_config(change).map_err(|e| self.config_refusal(e))
    }

    fn config_refusal(&self, message: String) -> Refusal {
        let code = if self.config_file.problem().is_some() { Code::Conflict } else { Code::Failed };
        Refusal::new(code, message)
    }

    fn project_folder(&self, change: &ProjectChange) -> Result<PathBuf, Refusal> {
        match (change.project, &change.path) {
            (Some(id), None) => {
                let found = self.projects.project_index(id);
                let found = found.ok_or_else(|| Refusal::new(Code::NotFound, format!("no project has the id {id}")))?;
                Ok(self.projects[found].path.clone())
            }
            (None, Some(path)) => {
                let given = path.to_str().ok_or_else(|| Refusal::new(Code::Malformed, "the path is not text"))?;
                let folder = config::expand_home(given, self.home.as_deref());
                if !folder.is_absolute() {
                    return Err(Refusal::new(Code::Malformed, "use an absolute path or one starting with ~/"));
                }
                Ok(folder.canonicalize().unwrap_or(folder))
            }
            _ => Err(Refusal::new(Code::Malformed, "give the project's `project` id or its `path`, not both")),
        }
    }

    fn config_project(&mut self, change: ProjectChange) -> Handled {
        let folder = self.project_folder(&change)?;
        let home = self.home.clone();
        let current = self.config.project(&folder, home.as_deref());
        let settings = ProjectSettings {
            setup: change.setup.unwrap_or(current.setup),
            teardown: change.teardown.unwrap_or(current.teardown),
        };
        if !settings.is_empty() && !git::is_repo_root(&folder) {
            let message = format!(
                "{} is not the root of a git repository: setup and teardown run in its worktrees",
                folder.display()
            );
            return Err(Refusal::new(Code::Conflict, message));
        }
        self.change_config(|config| config.set_project(&folder, home.as_deref(), settings))
            .map_err(|e| self.config_refusal(e))?;
        let name = text::display_path(&folder, home.as_deref());
        let saved = if self.config.project(&folder, home.as_deref()).is_empty() { "removed" } else { "saved" };
        Ok(Some(json(&self.described(vec![format!("{name}: setup and teardown {saved}")]))))
    }

    fn config_secret(&mut self, client: u64, secret: &SecretChange) -> Handled {
        let action = self.settings_model(Page::default()).secret(&secret.key, &secret.value).map_err(refusal)?;
        let key = self.clients.requests.key();
        match action {
            Action::CheckToken(source, token) => self.check_token_for(source, token, Some(key)),
            Action::CheckBot(via, token) => self.check_messenger_for(via, token, Some(key)),
            _ => return Err(failed(format!("`{}` takes no secret", secret.key))),
        }
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Reading };
        self.clients.requests.pending.push(pending);
        Ok(None)
    }

    pub(in crate::app) fn secret_answered(&mut self, key: u64, result: Result<String, String>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let reply = match result {
            Ok(notice) => Ok(json(&self.described(vec![notice]))),
            Err(e) => Err(failed(e)),
        };
        self.refuse_or_answer(pending.client, reply);
    }

    pub(in crate::app) fn token_set_by_request(
        &mut self,
        key: u64,
        source: Source,
        token: &Secret,
        result: Result<Account, Error>,
    ) {
        let mut unchosen = None;
        let result = match result {
            Err(Error::Workspaces(found)) => {
                unchosen = Some(found.message.clone());
                Ok(Account { handle: found.handle, workspace: String::new() })
            }
            other => other,
        };
        let saved = result.and_then(|account| {
            let name = source.secret_key().ok_or_else(|| Error::Api("nothing to save".into()))?;
            secrets::write(&self.issues.secrets_path, name, &token.0)
                .map_err(|e| Error::Api(format!("failed to save the {}: {e}", source.token_name())))?;
            Ok(account)
        });
        if let Ok(account) = &saved {
            self.forget_issues(source);
            self.learn_account(source, account);
            if let Some(message) = unchosen {
                self.issues.failures.insert((source, None), message);
            }
            self.refresh_open_settings();
        }
        let answer = saved.map(|account| format!("{} connected as {}", source.name(), account.describe()));
        self.secret_answered(key, answer.map_err(|e| e.to_string()));
    }

    fn config_remove(&mut self, key: &str) -> Handled {
        let model = self.settings_model(Page::default());
        let notice = match model.removal_of(key).map_err(refusal)? {
            None => format!("`{key}` is not set"),
            Some(Action::RemoveToken(source)) => self.remove_token(source).map_err(failed)?,
            Some(Action::RemoveBot(via)) => self.remove_messenger(via).map_err(failed)?,
            Some(Action::ForgetChat(via)) => self.forget_chat(via),
            Some(Action::ForgetBrowsers) => self.forget_browsers().map(String::from).map_err(failed)?,
            Some(_) => return Err(failed(format!("`{key}` cannot be removed"))),
        };
        self.refresh_open_settings();
        Ok(Some(json(&self.described(vec![notice]))))
    }
}
