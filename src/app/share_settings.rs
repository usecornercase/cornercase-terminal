use std::path::{Path, PathBuf};

use super::App;
use crate::config::{self, ProjectSettings};
use crate::control::config::SetupExported;
use crate::git;
use crate::log;
use crate::settings::Status;
use crate::settings::share::{self, Repository, Shared};
use crate::text;

const NEVER_SHARED: [&str; 4] =
    ["tokens, bots, paired chats and browsers", "jira_email", "accounts and profiles", "remote access and keep awake"];

impl App {
    pub(super) fn export_setup(&self) -> Result<SetupExported, String> {
        let mut left_out: Vec<String> = NEVER_SHARED.map(String::from).to_vec();
        let shared = Shared {
            settings: share::settings_of(&self.config),
            repositories: self.shared_repositories(&mut left_out),
            connect: self.connected_trackers(),
            unknown: Vec::new(),
        };
        let text = share::to_text(&shared)?;
        let (settings, repositories) = (shared.settings.len(), shared.repositories.len());
        log::info!("config", "config exported", bytes = text.len(), settings = settings, repositories = repositories);
        Ok(SetupExported { text, settings, repositories, left_out, warnings: share::secrets::warnings(&shared) })
    }

    fn shared_repositories(&self, left_out: &mut Vec<String>) -> Vec<Repository> {
        let home = self.home.as_deref();
        let mut found = Vec::new();
        for (key, settings) in &self.config.projects {
            let folder = config::expand_home(key, home);
            let folder = folder.canonicalize().unwrap_or(folder);
            left_out.extend(share_or_name(&mut found, folder, settings, home));
        }
        found.into_iter().map(|(_, repository)| repository).collect()
    }

    fn connected_trackers(&self) -> Vec<String> {
        self.token_statuses()
            .into_iter()
            .filter(|(_, status)| matches!(status, Status::Saved(_) | Status::Env))
            .map(|(source, _)| source.id().to_string())
            .collect()
    }
}

fn share_or_name(
    found: &mut Vec<(PathBuf, Repository)>,
    folder: PathBuf,
    settings: &ProjectSettings,
    home: Option<&Path>,
) -> Option<String> {
    let repository = match repository_of(&folder, settings) {
        Ok(repository) => repository,
        Err(why) => return Some(not_shared(&folder, home, why)),
    };
    if let Some(first) = sharing(found, &repository) {
        let (name, first) = (text::display_path(&folder, home), text::display_path(first, home));
        return Some(format!("{name} shares a remote with {first}, so only the first is shared"));
    }
    found.push((folder, repository));
    None
}

fn not_shared(folder: &Path, home: Option<&Path>, why: &str) -> String {
    format!("{} {why}, so its setup is not shared", text::display_path(folder, home))
}

fn sharing<'a>(found: &'a [(PathBuf, Repository)], repository: &Repository) -> Option<&'a Path> {
    let shares = |first: &Repository| first.remotes.iter().any(|remote| repository.remotes.contains(remote));
    found.iter().find(|(_, first)| shares(first)).map(|(folder, _)| folder.as_path())
}

fn repository_of(folder: &Path, settings: &ProjectSettings) -> Result<Repository, &'static str> {
    if !git::is_repo_root(folder) {
        return Err("is not a repository any more");
    }
    let mut remotes = git::remote::of(folder);
    remotes.sort_by_key(|remote| remote.name != "origin");
    let mut names: Vec<String> = Vec::new();
    for remote in remotes.into_iter().filter(|remote| !remote.path.is_empty()) {
        let name = format!("{}/{}", remote.host, remote.path);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        return Err("has no git remote");
    }
    Ok(Repository { remotes: names, setup: settings.setup.clone(), teardown: settings.teardown.clone() })
}
