use std::path::{Component, Path, PathBuf};

use ratatui::layout::Rect;
use serde_json::Value;

use super::{Asked, Handled, Pending, Stage, json, malformed, sized};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::config;
use crate::control::contract::{Code, Refusal};
use crate::control::{CloneRepository, Cloned, Done, Item};
use crate::git::clone::{self, Failure};
use crate::log::{self, Job, Level};

pub(super) struct Cloning {
    path: PathBuf,
    focus: bool,
    view: Option<u64>,
}

fn refusal(failure: &Failure) -> Refusal {
    let code = match failure {
        Failure::NoName | Failure::GitFolder => Code::Malformed,
        Failure::NotEmpty(_) => Code::Conflict,
        Failure::NotRepository(_) => Code::NotFound,
        Failure::Folder(_) | Failure::AccessDenied(_) | Failure::Failed(_) => Code::Failed,
    };
    Refusal::new(code, failure.to_string())
}

fn checked_url(url: String) -> Result<String, Refusal> {
    if url.is_empty() {
        return Err(malformed("say which repository to clone: its URL is empty"));
    }
    if url.starts_with('-') {
        return Err(malformed("a repository's URL cannot start with `-`"));
    }
    if clone::is_local_path(&url) && !Path::new(&url).is_absolute() {
        return Err(malformed("a repository on the server's machine must be given by its absolute path"));
    }
    Ok(url)
}

fn checked_parent(parent: PathBuf, given: &Path) -> Result<PathBuf, Refusal> {
    if !parent.is_absolute() {
        let given = given.display();
        return Err(malformed(format!("the folder to clone into must be absolute or start with ~/: `{given}`")));
    }
    if parent.components().any(|part| part == Component::ParentDir) {
        return Err(malformed(format!("the folder to clone into cannot go up with `..`: `{}`", given.display())));
    }
    Ok(parent)
}

fn overlapping<'a>(pending: &'a Pending, path: &Path) -> Option<&'a Path> {
    match &pending.stage {
        Stage::Clone(cloning) if cloning.path.starts_with(path) || path.starts_with(&cloning.path) => {
            Some(&cloning.path)
        }
        _ => None,
    }
}

impl App {
    pub(super) fn clone_request(&mut self, asked: Asked, request: &CloneRepository) -> Handled {
        sized(asked.area)?;
        let (url, path) = self.clone_destination(request)?;
        if let Some(running) = self.clients.requests.pending.iter().find_map(|p| overlapping(p, &path)) {
            let message = format!("a clone into `{}` is already running", running.display());
            return Err(Refusal::new(Code::Busy, message));
        }
        let target = clone::prepare(&url, path.clone()).map_err(|failure| refusal(&failure))?;
        let key = self.clients.requests.key();
        let job = Job::new(Level::Info, "git", "clone").with("path", path.display());
        self.spawn_job(
            job,
            move || Ok(clone::run(&target)),
            move |result, job| {
                let result = result.unwrap_or_else(|e| Err(Failure::Failed(e.to_string())));
                if let Err(failure) = &result {
                    job.fail(failure.kind());
                }
                Some(AppEvent::Cloned { request: key, result })
            },
        );
        let cloning = Cloning { path, focus: request.focus, view: asked.asker.view };
        let (client, done) = (Some(asked.client), Done::default());
        self.clients.requests.pending.push(Pending { client, key, timeout: None, done, stage: Stage::Clone(cloning) });
        Ok(None)
    }

    fn clone_destination(&self, request: &CloneRepository) -> Result<(String, PathBuf), Refusal> {
        let home = self.home.as_deref();
        let url = checked_url(config::expand_home(request.url.trim(), home).to_string_lossy().into_owned())?;
        let parent = checked_parent(config::expand_home(&request.parent.to_string_lossy(), home), &request.parent)?;
        let path = clone::destination(&url, &parent).map_err(|failure| refusal(&failure))?;
        Ok((url, path))
    }

    pub(in crate::app) fn clone_answered(&mut self, key: u64, result: Result<(), Failure>, area: Rect) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let Stage::Clone(cloning) = pending.stage else { return };
        let reply = result.map_err(|failure| refusal(&failure)).and_then(|()| self.cloned(&cloning, area));
        self.refuse_or_answer(pending.client, reply);
    }

    fn cloned(&mut self, cloning: &Cloning, area: Rect) -> Result<Value, Refusal> {
        let path = cloning.path.canonicalize().unwrap_or_else(|_| cloning.path.clone());
        let p = self.open_folder(path.clone(), Some(area)).map_err(|e| {
            format!("the repository was cloned into `{}`, but it could not be opened as a project: {e}", path.display())
        })?;
        let ids = self.shown_ids(p);
        let projection = self.show_clone(cloning, self.projects[p].id)?;
        Ok(json(&Cloned { path, ids, projection }))
    }

    fn show_clone(&mut self, cloning: &Cloning, project: u64) -> Result<Option<u64>, Refusal> {
        if let Some(view) = cloning.view
            && cloning.focus
            && self.clients.views.get(view).is_none()
        {
            log::info!("control", "view closed before the clone's focus", view = view, project = project);
            return Ok(None);
        }
        self.show_if(cloning.view, cloning.focus, Item::Project(project))
    }
}

#[cfg(test)]
mod tests;
