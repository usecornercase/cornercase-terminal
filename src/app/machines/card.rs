use std::time::{Duration, Instant};

use super::drag::RemoteDrag;
use super::rows::{status, statuses};
use super::{Flow, Held, OLDER};
use crate::agents::activity::Status;
use crate::app::App;
use crate::app::interaction::Drag;
use crate::control::TabInfo;
use crate::machines::State;
use crate::machines::link::Down;
use crate::text;
use crate::ui;

impl App {
    pub(in crate::app) fn remote_card(&self, now: Instant) -> Option<ui::RemoteCard> {
        let stale = self.remote_screen().map(|tab| tab.down);
        if stale == Some(false) {
            return None;
        }
        let shown = self.shown_remote()?;
        let held = self.machines.held.iter().find(|held| held.link.id() == shown.link)?;
        let seen = held.seen.as_deref()?;
        let workspace = seen.projects.iter().flat_map(|p| &p.workspaces).find(|w| w.id == shown.workspace)?;
        let tab = shown.tab.and_then(|id| workspace.tabs.iter().find(|tab| tab.id == id));
        let fallen = stale.is_some() || self.watched().is_some_and(|watched| matches!(watched.flow, Flow::Down(_)));
        Some(ui::RemoteCard {
            machine: held.name.clone(),
            agent: tab.and_then(agent),
            folder: text::display_path(&shown.path, seen.home.as_deref()),
            down: (held.status.state != State::Linked)
                .then(|| self.retry(held, now))
                .or_else(|| fallen.then_some(ui::Retry::Now)),
        })
    }

    pub(in crate::app) fn remote_screen(&self) -> Option<ui::RemoteTab> {
        let watched = self.watched()?;
        Some(ui::RemoteTab {
            screen: watched.screen.clone()?,
            dim_inactive: self.config.dim_inactive_panes,
            down: watched.flow != Flow::Live,
            dragging: match &self.screen.drag {
                Some(Drag::Remote(RemoteDrag::Divider { path, .. })) => Some(path.clone()),
                _ => None,
            },
        })
    }

    pub(in crate::app) fn card_wake(&self, now: Instant) -> Option<Duration> {
        let shown = self.shown_remote()?;
        let held = self.machines.held.iter().find(|held| held.link.id() == shown.link)?;
        let left = countdown(held, now).filter(|left| !left.is_zero())?;
        let rest = Duration::from_nanos(u64::from(left.subsec_nanos()));
        Some(if rest.is_zero() { Duration::from_secs(1) } else { rest })
    }

    pub(in crate::app) fn remote_working(&self) -> bool {
        let linked = self.machines.held.iter().filter(|held| held.status.state == State::Linked);
        let workspaces = linked.filter_map(|held| held.seen.as_deref()).flat_map(|seen| &seen.projects);
        statuses(workspaces.flat_map(|p| &p.workspaces)).any(|status| status == Some(Status::Working))
    }

    fn retry(&self, held: &Held, now: Instant) -> ui::Retry {
        match (&held.status.down, countdown(held, now)) {
            (Some(Down::Unreachable(_)), Some(left)) if left.is_zero() => ui::Retry::Now,
            (Some(Down::Unreachable(_)), Some(left)) => {
                ui::Retry::In(left.as_secs() + u64::from(left.subsec_nanos() > 0))
            }
            (Some(_), _) => ui::Retry::Because(self.machines.reason(held).unwrap_or_else(|| OLDER.into())),
            (None, _) => ui::Retry::Now,
        }
    }
}

fn countdown(held: &Held, now: Instant) -> Option<Duration> {
    let (Some(down @ Down::Unreachable(_)), Some((at, failed))) = (&held.status.down, held.down_at) else {
        return None;
    };
    Some((at + down.wait(failed)).saturating_duration_since(now))
}

fn agent(tab: &TabInfo) -> Option<(String, Option<Status>)> {
    let agents = || tab.panes.iter().filter(|pane| pane.agent.is_some());
    let pane = agents().find(|pane| pane.active).or_else(|| agents().next())?;
    Some((pane.agent.clone()?, status(pane.status.as_deref())))
}
