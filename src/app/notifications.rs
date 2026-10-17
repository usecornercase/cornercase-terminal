use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use super::App;
use super::messenger::run_id;
use super::toast::Toast;
use crate::agents::activity::Status;
use crate::host::notify::{self, Notification};
use crate::log;
use crate::project::Hierarchy;
use crate::project::search::Goto;
use crate::ui;

#[derive(Debug, Clone)]
struct Notice {
    token: String,
    agent: String,
    place: String,
    message: String,
    status: Status,
    project: u64,
    workspace: u64,
    panes: Vec<u64>,
}

#[derive(Debug, Default)]
struct Window {
    unfocused: bool,
    missed: Option<String>,
}

#[derive(Debug)]
pub(super) struct Notices {
    run: String,
    next: u64,
    sent: VecDeque<Notice>,
    windows: HashMap<u64, Window>,
}

impl Default for Notices {
    fn default() -> Self {
        Self { run: run_id(), next: 0, sent: VecDeque::new(), windows: HashMap::new() }
    }
}

impl Notices {
    pub(super) fn forget(&mut self, client: u64) {
        self.windows.remove(&client);
    }

    fn find(&self, token: &str) -> Option<&Notice> {
        self.sent.iter().find(|notice| notice.token == token)
    }
}

pub(super) struct Said<'a> {
    pub(super) agent: &'a str,
    pub(super) status: Status,
    pub(super) place: String,
    pub(super) message: String,
    pub(super) project: u64,
    pub(super) workspace: u64,
    pub(super) panes: Vec<u64>,
}

impl App {
    pub(super) fn tell(&mut self, said: Said<'_>) {
        let notices = &mut self.notices;
        notices.next += 1;
        let token = format!("cc-{}-{}", notices.run, notices.next);
        if notices.sent.len() >= notify::KEPT {
            notices.sent.pop_front();
        }
        let notice = Notice {
            token: token.clone(),
            agent: said.agent.to_string(),
            place: said.place,
            message: said.message,
            status: said.status,
            project: said.project,
            workspace: said.workspace,
            panes: said.panes,
        };
        let toast = Toast {
            go: Some(token.clone()),
            ..Toast::new(notice.message.clone(), ui::ToastIcon::Agent(notice.status))
        };
        if let Some(n) = Notification::new(&notice.message, &self.config.desktop_notifications) {
            self.outbox.notifications.push(Notification { token: Some(token.clone()), ..n });
            for window in notices.windows.values_mut().filter(|w| w.unfocused) {
                window.missed = Some(token.clone());
            }
        }
        notices.sent.push_back(notice);
        self.toast = Some(toast);
    }

    pub(crate) fn notification_clicked(&mut self, token: &str) {
        let Some(notice) = self.notices.find(token).cloned() else { return };
        if self.toast.as_ref().is_some_and(|toast| toast.go.as_deref() == Some(token)) {
            self.toast = None;
        }
        for window in self.notices.windows.values_mut().filter(|w| w.missed.as_deref() == Some(token)) {
            window.missed = None;
        }
        let pane = self.notice_pane(&notice);
        let workspace_open = self
            .projects
            .workspace_in(notice.project, notice.workspace)
            .is_some_and(|(p, w)| !self.projects[p].closing && self.projects[p].workspaces[w].open());
        let to = if let Some(pane) = pane {
            self.jump_to_pane(pane);
            "pane"
        } else if workspace_open {
            self.goto(Goto::Place { project: notice.project, workspace: Some(notice.workspace), tab: None });
            "workspace"
        } else {
            self.toast =
                Some(Toast::new(format!("{} in {} is gone", notice.agent, notice.place), ui::ToastIcon::Check));
            "gone"
        };
        log::info!("activity", "notification clicked", pane = pane.unwrap_or_default(), to = to);
    }

    fn notice_pane(&self, notice: &Notice) -> Option<u64> {
        let (p, w) = self.projects.workspace_in(notice.project, notice.workspace)?;
        let project = &self.projects[p];
        let workspace = &project.workspaces[w];
        if project.closing || !workspace.open() {
            return None;
        }
        let kept: Vec<_> =
            workspace.tabs.iter().flat_map(|tab| &tab.panes).filter(|term| notice.panes.contains(&term.id)).collect();
        kept.iter().find(|term| term.agent.status().is_some_and(Status::needs_you)).or(kept.first()).map(|term| term.id)
    }

    pub(crate) fn window_focus(&mut self, client: u64, focused: bool) {
        let window = self.notices.windows.entry(client).or_default();
        window.unfocused = !focused;
        if !focused {
            return;
        }
        let Some(token) = window.missed.take() else { return };
        let Some(notice) = self.notices.find(&token).cloned() else { return };
        let needs = self.projects.workspace_in(notice.project, notice.workspace).is_some_and(|(p, w)| {
            let tabs = &self.projects[p].workspaces[w].tabs;
            let terms = || tabs.iter().flat_map(|tab| &tab.panes).filter(|term| notice.panes.contains(&term.id));
            let shown = self.visible_tab().is_some_and(|id| {
                tabs.iter().any(|tab| tab.id == id && tab.panes.iter().any(|term| notice.panes.contains(&term.id)))
            });
            !shown && terms().any(|term| term.agent.status().is_some_and(Status::needs_you))
        });
        if needs {
            self.toast = Some(Toast {
                at: Instant::now(),
                go: Some(token),
                ..Toast::new(notice.message, ui::ToastIcon::Agent(notice.status))
            });
            log::info!("activity", "notification shown again", client = client);
        }
    }
}
