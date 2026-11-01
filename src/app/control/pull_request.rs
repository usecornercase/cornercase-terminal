use std::time::Instant;

use super::status::{check_info, pull_request_info};
use super::{Asker, Handled, Pending, Reply, Stage, json};
use crate::app::App;
use crate::app::pull_requests::PullRead;
use crate::control::contract::Refusal;
use crate::control::{Done, PullRequestAnswer, PullRequestInfo, PullRequestRead};
use crate::project::{Hierarchy, WorkspaceAt};
use crate::pull_request::PullRequest;

const NO_WORKSPACE: &str = "no workspace is open here; pass --workspace or --worktree";

impl App {
    pub(super) fn pull_request_request(
        &mut self,
        client: u64,
        asker: Asker,
        request: &PullRequestRead,
        now: Instant,
    ) -> Handled {
        let at = match request.workspace {
            Some(id) => self.usable_workspace(id)?,
            None => self.here(asker).workspace.ok_or(NO_WORKSPACE)?,
        };
        let workspace = self.projects.workspace(at).id;
        if !matches!(self.pull_request_read(workspace), PullRead::Unasked { .. }) {
            self.ask_pull_request_now(workspace, now);
        }
        if let Some(reply) = self.pull_request_reply(workspace) {
            return reply.map(Some).map_err(Refusal::from);
        }
        let key = self.clients.requests.key();
        let stage = Stage::PullRequest(workspace);
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done: Done::default(),
            stage,
        });
        Ok(None)
    }

    pub(super) fn pull_request_reply(&self, workspace: u64) -> Option<Reply> {
        if let Err(refusal) = self.usable_workspace(workspace) {
            return Some(Err(refusal.message));
        }
        let (branch, pull_request, reason) = match self.pull_request_read(workspace) {
            PullRead::Waiting => return None,
            PullRead::Failed(error) => return Some(Err(error)),
            PullRead::Unasked { branch, reason } => (branch, None, Some(reason)),
            PullRead::Read { branch, pull } => {
                let reason = pull.is_none().then(|| format!("no {} for {branch}", self.noun_of_workspace(workspace)));
                (Some(branch.to_string()), pull.map(listed), reason)
            }
        };
        Some(Ok(json(&PullRequestAnswer { workspace, branch, pull_request, reason })))
    }

    fn noun_of_workspace(&self, workspace: u64) -> &'static str {
        let p = self.projects.workspace_index(workspace).map_or(self.active, WorkspaceAt::project);
        self.pull_noun_of(p)
    }
}

fn listed(pull: &PullRequest) -> PullRequestInfo {
    let all = pull.checks.iter().map(check_info).collect();
    PullRequestInfo { all_checks: Some(all), ..pull_request_info(pull) }
}
