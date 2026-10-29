use std::path::PathBuf;

use ratatui::layout::Rect;
use serde_json::Value;

use super::{Asked, Handled, Pending, Stage, json, malformed, sized};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::config;
use crate::control::contract::{Code, Refusal};
use crate::control::{CloneRepository, Cloned, Done, Item};
use crate::git::clone::{self, Failure, Target};
use crate::log::{Job, Level};

pub(super) struct Cloning {
    path: PathBuf,
    focus: bool,
    view: Option<u64>,
}

fn refusal(failure: &Failure) -> Refusal {
    let code = match failure {
        Failure::NoName(_) => Code::Malformed,
        Failure::NotEmpty(_) => Code::Conflict,
        Failure::NotRepository { .. } => Code::NotFound,
        Failure::AccessDenied { .. } | Failure::Failed(_) => Code::Failed,
    };
    Refusal::new(code, failure.to_string())
}

impl App {
    pub(super) fn clone_request(&mut self, asked: Asked, request: &CloneRepository) -> Handled {
        sized(asked.area)?;
        let target = self.clone_target(request)?;
        let path = target.path.clone();
        if self.clients.requests.pending.iter().any(|p| matches!(&p.stage, Stage::Clone(c) if c.path == path)) {
            return Err(Refusal::new(Code::Busy, format!("a clone into `{}` is already running", path.display())));
        }
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

    fn clone_target(&self, request: &CloneRepository) -> Result<Target, Refusal> {
        let url = request.url.trim();
        if url.is_empty() {
            return Err(malformed("say which repository to clone: its URL is empty"));
        }
        if url.starts_with('-') {
            return Err(malformed(format!("`{url}` is not a repository's URL")));
        }
        let parent = config::expand_home(&request.parent.to_string_lossy(), self.home.as_deref());
        if !parent.is_absolute() {
            let given = request.parent.display();
            return Err(malformed(format!("the folder to clone into must be absolute or start with ~/: `{given}`")));
        }
        clone::target(url, &parent).map_err(|failure| refusal(&failure))
    }

    pub(in crate::app) fn clone_answered(&mut self, key: u64, result: Result<(), Failure>, area: Rect) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let Stage::Clone(cloning) = pending.stage else { return };
        let reply = result.map_err(|failure| refusal(&failure)).and_then(|()| self.cloned(&cloning, area));
        self.refuse_or_answer(pending.client, reply);
    }

    fn cloned(&mut self, cloning: &Cloning, area: Rect) -> Result<Value, Refusal> {
        let path = cloning.path.canonicalize().unwrap_or_else(|_| cloning.path.clone());
        let p = self.open_folder(path.clone(), Some(area))?;
        let ids = self.shown_ids(p);
        let projection = self.show_if(cloning.view, cloning.focus, Item::Project(self.projects[p].id))?;
        Ok(json(&Cloned { path, ids, projection }))
    }
}
