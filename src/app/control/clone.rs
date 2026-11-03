use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ratatui::layout::Rect;
use serde_json::Value;

use super::{Asked, Handled, Pending, Stage, json, malformed, sized};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::config;
use crate::control::contract::{Code, Refusal};
use crate::control::{CloneFile, CloneRepository, Cloned, Done, Item, Unwritten};
use crate::git::clone::files::{self, File, MAX_TOTAL_BYTES};
use crate::git::clone::{self, Failure, Target};
use crate::log::{self, Job, Level};
use crate::machines::send;

pub(super) struct Cloning {
    path: PathBuf,
    focus: bool,
    view: Option<u64>,
}

fn refusal(failure: &Failure) -> Refusal {
    let code = match failure {
        Failure::NoName | Failure::GitFolder | Failure::Files(_) => Code::Malformed,
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

fn decoded(files: &[CloneFile]) -> Result<Vec<File>, Failure> {
    let mut total = 0;
    let mut decoded = Vec::with_capacity(files.len());
    for file in files {
        let bytes = STANDARD
            .decode(&file.data)
            .map_err(|e| Failure::Files(format!("the `data` of `{}` is not base64: {e}", file.path.display())))?;
        total += bytes.len() as u64;
        if total > MAX_TOTAL_BYTES {
            let limit = MAX_TOTAL_BYTES >> 20;
            return Err(Failure::Files(format!("the files are over the {limit} MiB a clone takes")));
        }
        decoded.push(File { path: file.path.clone(), bytes, mode: file.mode });
    }
    Ok(decoded)
}

fn clone_with(target: &Target, files: &[CloneFile]) -> Result<Vec<files::Unwritten>, Failure> {
    let files = decoded(files).inspect_err(|_| target.undo())?;
    clone::run(target)?;
    Ok(files::write(&target.path, &files))
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
    pub(super) fn clone_request(&mut self, asked: Asked, request: CloneRepository) -> Handled {
        sized(asked.area)?;
        let (url, path) = self.clone_destination(&request)?;
        if let Some(running) = self.clients.requests.pending.iter().find_map(|p| overlapping(p, &path)) {
            let message = format!("a clone into `{}` is already running", running.display());
            return Err(Refusal::new(Code::Busy, message));
        }
        let target = clone::prepare(&url, path.clone()).map_err(|failure| refusal(&failure))?;
        let key = self.clients.requests.key();
        let job = Job::new(Level::Info, "git", "clone").with("path", path.display());
        let job = job.with("files", request.files.len()).with("bytes", send::bytes(&request.files));
        let files = request.files;
        self.spawn_job(
            job,
            move || Ok(clone_with(&target, &files)),
            move |result, job| {
                let result = result.unwrap_or_else(|e| Err(Failure::Failed(e.to_string())));
                match &result {
                    Ok(unwritten) if !unwritten.is_empty() => job.note("unwritten", unwritten.len()),
                    Ok(_) => {}
                    Err(failure) => job.fail(failure.kind()),
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

    pub(in crate::app) fn clone_answered(
        &mut self,
        key: u64,
        result: Result<Vec<files::Unwritten>, Failure>,
        area: Rect,
    ) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let Stage::Clone(cloning) = pending.stage else { return };
        let reply = result.map_err(|failure| refusal(&failure)).and_then(|unwritten| {
            let unwritten = unwritten.into_iter().map(|file| Unwritten { path: file.path, why: file.why }).collect();
            self.cloned(&cloning, unwritten, area)
        });
        self.refuse_or_answer(pending.client, reply);
    }

    fn cloned(&mut self, cloning: &Cloning, unwritten: Vec<Unwritten>, area: Rect) -> Result<Value, Refusal> {
        let path = cloning.path.canonicalize().unwrap_or_else(|_| cloning.path.clone());
        let p = self.open_folder(path.clone(), Some(area)).map_err(|e| {
            format!("the repository was cloned into `{}`, but it could not be opened as a project: {e}", path.display())
        })?;
        let ids = self.shown_ids(p);
        let projection = self.show_clone(cloning, self.projects[p].id)?;
        Ok(json(&Cloned { path, ids, projection, unwritten }))
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
