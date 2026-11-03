use super::Held;
use crate::app::App;
use crate::control::ProjectInfo;
use crate::machines::link::Down;
use crate::machines::run_on::{NEEDS_REMOTE, RunOn, Target};
use crate::machines::{self, CONNECTING, NOT_SEEN, State};
use crate::project::Hierarchy;
use crate::text;

const UNREACHABLE: &str = "unreachable";

impl App {
    pub(in crate::app) fn run_on_for(&self, project: u64) -> Option<RunOn> {
        let held = self.machines_held();
        if held.is_empty() {
            return None;
        }
        let target = |(info, held): (machines::Info, &Held)| Target {
            name: held.name.clone(),
            icon: machines::known_icon(&info.icon),
            link: held.link.id(),
            why: self.run_on_why(held),
        };
        let blocked = self.identity(project).is_none().then_some(NEEDS_REMOTE);
        Some(RunOn { targets: held.into_iter().map(target).collect(), blocked, ..RunOn::default() })
    }

    pub(in crate::app) fn project_there(&self, link: u64, project: u64) -> Option<&ProjectInfo> {
        let identity = self.identity(project)?;
        let seen = self.machines.held().iter().find(|held| held.link.id() == link)?.seen.as_ref()?;
        let same = |there: &&ProjectInfo| there.repo.as_deref().is_some_and(|repo| repo.eq_ignore_ascii_case(identity));
        seen.projects.iter().find(same)
    }

    pub(in crate::app) fn where_there(&self, link: u64, project: u64, worktree: bool) -> String {
        self.placed_there(link, project, if worktree { "in its own worktree" } else { "in a tab" })
    }

    pub(in crate::app) fn placed_there(&self, link: u64, project: u64, how: &str) -> String {
        let Some(held) = self.machines.held().iter().find(|held| held.link.id() == link) else { return String::new() };
        let name = &held.name;
        let Some(there) = self.project_there(link, project) else { return format!("not on {name} · clone…") };
        let home = held.seen.as_ref().and_then(|seen| seen.home.as_deref());
        format!("{how} on {name}, in {}", text::display_path(&there.path, home))
    }

    fn identity(&self, project: u64) -> Option<&str> {
        let at = self.projects.project_index(project)?;
        self.projects[at].repo.identity.as_deref()
    }

    fn run_on_why(&self, held: &Held) -> Option<String> {
        match (held.status.state, &held.status.down) {
            (State::Linked, _) if held.seen.is_some() => None,
            (State::Linked, _) => Some(NOT_SEEN.into()),
            (State::Connecting, _) => Some(CONNECTING.into()),
            (_, Some(Down::Unreachable(_)) | None) => Some(UNREACHABLE.into()),
            _ => Some(self.unlinked(held)),
        }
    }
}
