use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use super::App;
use super::messenger::run_id;
use super::toast::Toast;
use crate::agents::activity::Status;
use crate::control::{Event, Ids, Item, What};
use crate::host::notify::{self, Notification};
use crate::host::sound::{self, Cue};
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
    pub(super) sound: Option<sound::Kind>,
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
        if let Some(kind) = said.sound {
            let path = match kind {
                sound::Kind::NeedsYou => &self.config.sound_waiting,
                sound::Kind::Finished => &self.config.sound_done,
            };
            self.outbox.sounds.push(Cue { kind, path: path.clone() });
        }
        let event = Event {
            time: crate::time::Stamp(std::time::SystemTime::now()).to_string(),
            what: What::Notice {
                token,
                agent: notice.agent.clone(),
                status: notice.status.name().to_string(),
                place: notify::clean(&notice.place),
                message: notice.message.clone(),
                panes: notice.panes.clone(),
                sound: said.sound.map(|kind| kind.id().to_string()),
            },
            ids: Ids { project: Some(notice.project), workspace: Some(notice.workspace), ..Ids::default() },
        };
        notices.sent.push_back(notice);
        self.toast = Some(toast);
        self.events.publish(&[event]);
    }

    pub(in crate::app) fn act_on_notice(&mut self, token: &str) -> Option<Result<(u64, Item), String>> {
        let notice = self.notices.find(token).cloned()?;
        if self.toast.as_ref().is_some_and(|toast| toast.go.as_deref() == Some(token)) {
            self.toast = None;
        }
        for window in self.notices.windows.values_mut().filter(|w| w.missed.as_deref() == Some(token)) {
            window.missed = None;
        }
        if let Some(pane) = self.notice_pane(&notice) {
            return Some(Ok((notice.project, Item::Pane(pane))));
        }
        let open = self
            .projects
            .workspace_in(notice.project, notice.workspace)
            .is_some_and(|(p, w)| !self.projects[p].closing && self.projects[p].workspaces[w].open());
        Some(if open {
            Ok((notice.project, Item::Workspace(notice.workspace)))
        } else {
            Err(format!("{} in {} is gone", notice.agent, notice.place))
        })
    }

    pub(crate) fn notification_clicked(&mut self, token: &str) {
        let Some(went) = self.act_on_notice(token) else { return };
        let (pane, to) = match went {
            Ok((_, Item::Pane(pane))) => {
                self.jump_to_pane(pane);
                (pane, "pane")
            }
            Ok((project, Item::Workspace(workspace))) => {
                self.goto(Goto::Place { project, workspace: Some(workspace), tab: None });
                (0, "workspace")
            }
            Ok(_) => return,
            Err(gone) => {
                self.toast = Some(Toast::new(gone, ui::ToastIcon::Check));
                (0, "gone")
            }
        };
        log::info!("activity", "notification clicked", pane = pane, to = to);
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
            let seen = self.seen_tabs();
            let shown = seen.iter().any(|id| {
                tabs.iter().any(|tab| tab.id == *id && tab.panes.iter().any(|term| notice.panes.contains(&term.id)))
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
