use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::MachineEvent;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::git::remote::Repo;
use crate::log::{Job, Level};
use crate::machines::repo::{self, Hostnames};
use crate::project::Hierarchy;

const READ_EVERY: Duration = Duration::from_secs(30);

pub(in crate::app) struct Repos {
    hostnames: Hostnames,
    read: HashMap<u64, Instant>,
    reading: HashSet<u64>,
}

impl Repos {
    pub(super) fn new() -> Self {
        Self { hostnames: Hostnames::new(repo::ssh()), read: HashMap::new(), reading: HashSet::new() }
    }

    #[cfg(test)]
    pub(in crate::app) fn set_resolver(&mut self, resolve: repo::Resolve) {
        self.hostnames = Hostnames::new(resolve);
    }

    fn due(&self, project: u64, now: Instant) -> bool {
        !self.reading.contains(&project)
            && self.read.get(&project).is_none_or(|at| now.saturating_duration_since(*at) >= READ_EVERY)
    }
}

impl App {
    pub(in crate::app) fn read_repos(&mut self, now: Instant) {
        let projects = &self.projects;
        let repos = &mut self.machines.repos;
        repos.read.retain(|id, _| projects.project_index(*id).is_some());
        let due: Vec<(u64, PathBuf)> =
            projects.iter().filter(|p| !p.closing && repos.due(p.id, now)).map(|p| (p.id, p.path.clone())).collect();
        for (project, path) in due {
            self.machines.repos.read.insert(project, now);
            self.machines.repos.reading.insert(project);
            let hostnames = self.machines.repos.hostnames.clone();
            self.spawn_job(
                Job::new(Level::Debug, "machines", "repo").with("project", project),
                move || Ok(hostnames.repo(&path)),
                move |repo, _| Some(AppEvent::Machine(MachineEvent::Repo { project, repo: repo.ok() })),
            );
        }
    }

    pub(super) fn repo_read(&mut self, project: u64, repo: Option<Repo>) {
        self.machines.repos.reading.remove(&project);
        if let (Some(p), Some(repo)) = (self.projects.project_index(project), repo) {
            self.projects[p].repo = repo;
        }
        self.follow_pairing();
    }
}
