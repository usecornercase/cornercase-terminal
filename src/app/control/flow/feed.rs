use super::super::{Handled, conflict, json, none};
use crate::agents::activity::Status;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::events::Streamed;
use crate::control::contract::Refusal;
use crate::control::{
    self, FlowBacklog, FlowMessage, FlowRuns, Response, RoleState, RunFile, RunFiles, RunState, StepInfo,
};
use crate::error::Result;
use crate::flows::folder;
use crate::flows::run::Run;
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, Workspace};
use crate::time::Stamp;
use crate::version;

impl App {
    pub(in crate::app) fn workspace_run_dir(&self, workspace: &Workspace) -> Option<std::path::PathBuf> {
        let run = self.runs.of_workspace(workspace.id);
        run.and_then(|run| run.dir.clone()).or_else(|| workspace.run_dirs.last().cloned())
    }

    pub(in crate::app) fn flow_runs_state(&self) -> FlowRuns {
        let runs = self.runs.all().iter().filter_map(|run| self.run_state(run)).collect();
        FlowRuns { version: version::CURRENT.to_string(), boot: control::server_token().to_string(), runs }
    }

    fn run_state(&self, run: &Run) -> Option<RunState> {
        let at = self.projects.workspace_index(run.workspace)?;
        let workspace = self.projects.workspace(at);
        let roles: Vec<RoleState> = std::iter::once(&run.flow.orchestrator)
            .chain(&run.flow.roles)
            .map(|role| self.run_role(run, workspace, &role.name))
            .collect();
        let ended = roles.first().and_then(|role| role.status.as_deref()) == Some(Status::Pending.name());
        let progress = &run.progress;
        Some(RunState {
            run: run.id,
            project: self.projects[at.project()].id,
            workspace: run.workspace,
            flow: (&run.flow).into(),
            task: run.task.clone(),
            dir: run.dir.clone(),
            roles,
            stage: progress.active,
            rounds: (0..run.flow.stages.len()).map(|stage| progress.rounds(stage)).collect(),
            holder: progress.holder.clone(),
            last: progress.last.as_ref().map(StepInfo::from),
            outside: progress.outside.map(Into::into),
            ended,
            messages: run.recorded,
        })
    }

    fn run_role(&self, run: &Run, workspace: &Workspace, role: &str) -> RoleState {
        let tree = workspace.role_worktrees.iter().find(|tree| tree.role == role);
        let mut state = RoleState {
            role: role.to_string(),
            worktree: tree.map(|tree| tree.path.clone()),
            branch: tree.map(|tree| tree.branch.clone()),
            ..RoleState::default()
        };
        let Some(pane) = run.pane_of(role) else { return state };
        state.pane = Some(pane);
        match self.projects.pane_by(pane) {
            Some(term) => {
                state.status = term.agent.status().map(|status| status.name().to_string());
                state.tab = self.projects.tab_of_pane(pane).map(|at| self.projects.tab(at).id);
            }
            None => state.closed = true,
        }
        state
    }

    fn flow_workspace(&self, caller: Option<u64>, workspace: Option<u64>) -> std::result::Result<u64, Refusal> {
        if let Some(id) = workspace {
            return self.projects.workspace_index(id).map(|_| id).ok_or_else(|| none("workspace", id));
        }
        self.here(caller)
            .workspace
            .map(|at| self.projects.workspace(at).id)
            .ok_or_else(|| conflict("no workspace is open here; pass --workspace ID or --worktree BRANCH"))
    }

    pub(in crate::app) fn flow_messages_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        messages: &control::FlowMessages,
    ) -> Handled {
        let workspace = self.flow_workspace(caller, messages.workspace)?;
        let Some(run) = self.run_of(workspace) else {
            return Err(conflict(format!("workspace {workspace} runs no flow; `cornercase flow runs` lists the runs")));
        };
        if self.clients.events.follows_flow_messages(client) {
            return Err(conflict("this connection already follows a run's messages; open another connection"));
        }
        let backlog = FlowBacklog {
            boot: control::server_token().to_string(),
            run: run.id,
            workspace,
            messages: run.messages.iter().map(|message| FlowMessage::of(run, message)).collect(),
        };
        log::info!(
            "control",
            "following flow messages",
            client = client,
            run = backlog.run,
            workspace = workspace,
            kept = backlog.messages.len()
        );
        self.clients.events.follow_flow_messages(client, backlog.run);
        Ok(Some(json(&backlog)))
    }

    pub(in crate::app) fn publish_flow_message(&mut self, message: &FlowMessage) {
        let followers = self.clients.events.flow_message_followers(message.run);
        if followers.is_empty() {
            return;
        }
        log::debug!("control", "flow message", run = message.run, seq = message.seq, bytes = message.bytes);
        let Some(text) =
            serde_json::to_value(message).ok().and_then(|value| serde_json::to_string(&Response::Ok(value)).ok())
        else {
            return;
        };
        for client in followers {
            self.clients.events.push(client, Streamed::Event(text.clone()));
        }
    }

    pub(in crate::app) fn end_flow_messages(&mut self) {
        let runs = &self.runs;
        let ended = self.clients.events.stop_flow_messages(|run| runs.has(run));
        for client in ended {
            log::info!("control", "flow messages ended", client = client, reason = "run ended");
            self.clients.events.push(client, Streamed::End);
        }
    }

    pub(in crate::app) fn flow_files_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        files: &control::FlowFiles,
    ) -> Handled {
        let workspace = self.flow_workspace(caller, files.workspace)?;
        let found = self.projects.workspace_by(workspace).ok_or_else(|| none("workspace", workspace))?;
        let Some(dir) = self.workspace_run_dir(found) else {
            return Err(conflict(format!("workspace {workspace} has no run folder: it never ran a flow")));
        };
        let run = self.run_of(workspace).map(|run| run.id);
        let key = self.clients.requests.key();
        let job = Job::new(Level::Debug, "flow", "list files").with("workspace", workspace);
        let listed = dir.clone();
        self.spawn_job(
            job,
            move || Ok(folder::list(&listed)?),
            move |result: Result<folder::Listing>, job| {
                if let Ok(listing) = &result {
                    job.note("files", listing.files.len());
                }
                let result = match result {
                    Ok(listing) => Ok(RunFiles {
                        workspace,
                        run,
                        files: listing
                            .files
                            .into_iter()
                            .map(|file| RunFile {
                                name: file.name,
                                path: file.path,
                                bytes: file.bytes,
                                modified: file.modified.map(|at| Stamp(at).to_string()),
                            })
                            .collect(),
                        more: listing.more,
                        dir: dir.clone(),
                    }),
                    Err(e) => Err(format!("cannot list the run's folder {}: {e}", dir.display())),
                };
                Some(AppEvent::FlowFiles { request: key, result })
            },
        );
        self.pend_reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn flow_files_listed(&mut self, key: u64, result: std::result::Result<RunFiles, String>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        self.answer(pending.client, result.map(|files| json(&files)));
    }
}
