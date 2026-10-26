use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::Value;

use super::{Asked, Asker, Handled, conflict, json, malformed};
use crate::app::files_panel::{self, NO_WORKSPACE, Opened, Watch};
use crate::app::{App, AppEvent, FilesEvent};
use crate::control::contract::Refusal;
use crate::control::files::{FilesSearch as Search, Found, SearchMode};
use crate::control::{Command, FilesList, FilesRead, FilesSearch};
use crate::files::search;
use crate::log::{Job, Level};
use crate::project::Hierarchy;

type Place = (Option<u64>, Option<PathBuf>);

impl App {
    pub(super) fn files_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::FilesList(list) => self.files_list_request(client, asker, &list, now),
            Command::FilesRead(read) => self.files_read_request(client, asker, &read, now),
            Command::FilesSearch(search) => self.files_search_request(client, asker, &search),
            other => return ControlFlow::Continue(other),
        })
    }

    fn files_place(&self, asker: Asker, id: Option<u64>, needed: bool) -> Result<Place, Refusal> {
        let id = id.or_else(|| self.here(asker).workspace.map(|at| self.projects.workspace(at).id));
        let Some(id) = id else {
            return if needed { Err(Refusal::from(NO_WORKSPACE)) } else { Ok((None, None)) };
        };
        let at = self.usable_workspace(id)?;
        if self.projects[at.project()].closing {
            return Err(format!("workspace {id} is closing").into());
        }
        Ok((Some(id), Some(self.projects.workspace(at).path.clone())))
    }

    fn check_files(&self, place: &Place, asked: &[String]) -> Result<(), Refusal> {
        let root = place.1.as_deref();
        asked.iter().try_for_each(|path| files_panel::target(path, root, self.home.as_deref()).map(drop))
    }

    fn start_files(&mut self, client: u64, opened: &Opened, now: Instant) {
        self.start_file_stream(opened, now);
        self.pend_reading(client, opened.key);
    }

    fn already_following(&self, client: u64, follow: bool) -> Result<(), Refusal> {
        if follow && self.clients.file_streams.follows(client) {
            return Err(conflict("this connection already follows files; open another connection to follow more"));
        }
        Ok(())
    }

    fn files_list_request(&mut self, client: u64, asker: Asker, request: &FilesList, now: Instant) -> Handled {
        self.already_following(client, request.follow)?;
        let home = self.home.as_deref();
        let relative = request.folders.is_empty() || request.folders.iter().any(|f| !files_panel::is_absolute(f, home));
        let place = self.files_place(asker, request.workspace, relative)?;
        let folders = if request.folders.is_empty() { vec![String::new()] } else { request.folders.clone() };
        self.check_files(&place, &folders)?;
        let key = self.clients.requests.key();
        let opened = Opened { client, key, follow: request.follow, workspace: place.0, watch: Watch::Folders(folders) };
        self.start_files(client, &opened, now);
        Ok(None)
    }

    fn files_read_request(&mut self, client: u64, asker: Asker, request: &FilesRead, now: Instant) -> Handled {
        self.already_following(client, request.follow)?;
        if request.path.trim().is_empty() {
            return Err(malformed("`files-read` needs the file's path"));
        }
        let relative = !files_panel::is_absolute(&request.path, self.home.as_deref());
        let place = self.files_place(asker, request.workspace, relative)?;
        self.check_files(&place, std::slice::from_ref(&request.path))?;
        let key = self.clients.requests.key();
        let watch = Watch::File(request.path.clone());
        let opened = Opened { client, key, follow: request.follow, workspace: place.0, watch };
        self.start_files(client, &opened, now);
        Ok(None)
    }

    fn files_search_request(&mut self, client: u64, asker: Asker, request: &FilesSearch) -> Handled {
        let (Some(workspace), Some(root)) = self.files_place(asker, request.workspace, true)? else {
            return Err(Refusal::from(NO_WORKSPACE));
        };
        if request.query.trim().is_empty() {
            return Err(malformed("`files-search` needs something to look for"));
        }
        let key = self.clients.requests.key();
        let cancel = self.clients.file_streams.search(client, key);
        let Search { query, mode, .. } = request.clone();
        let job = Job::new(Level::Debug, "files", "search request").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(searched(workspace, &root, &query, mode, &cancel)),
            move |found, _| {
                let reply = found.map_err(|e| Refusal::from(format!("cannot search the files: {e}")));
                Some(AppEvent::Files(FilesEvent::Replied { request: key, reply }))
            },
        );
        self.pend_reading(client, key);
        Ok(None)
    }
}

fn searched(
    workspace: u64,
    root: &std::path::Path,
    query: &str,
    mode: SearchMode,
    cancel: &std::sync::atomic::AtomicBool,
) -> Value {
    let paths = search::index(root, cancel);
    json(&match mode {
        SearchMode::Names => Found::names(workspace, query, &search::names(&paths, query)),
        SearchMode::Text => Found::text(workspace, query, &search::text(root, &paths, query, cancel)),
    })
}
