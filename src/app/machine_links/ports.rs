use std::time::Instant;

use super::acting::Aim;
use super::panels::RemoteAim;
use super::selection::RemoteItem;
use crate::app::App;
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::control::{Command, Ports, ProjectInfo, StopPort, WorkspaceInfo};
use crate::log;
use crate::machines::follow::{Event, Follow, Followed};
use crate::machines::heard;
use crate::ports::Port;
use crate::ports::dialog::{Dialog, There};
use crate::ports::stop::Target;
use crate::process::escalation::KILLED_BECAUSE;
use crate::ui;

pub(in crate::app) const STOPS_THERE: &str = "The machine stops only that process: it asks it to end, \
     then ends it if it still holds its port after 3 s.";

const UNSTARTED: &str =
    "the machine did not say when it started, so it could not be told from a program that took its pid since";

#[derive(Default)]
pub(in crate::app) struct RemotePorts {
    listing: Option<(Follow, u64)>,
    stops: Vec<(Follow, Target, String)>,
}

impl App {
    pub(in crate::app) fn open_remote_ports(&mut self, aim: Aim) {
        let Some(held) = self.held_by(aim.link) else { return };
        if held.era != aim.era {
            let why = format!("{} restarted", held.name);
            return self.not_sent(&why);
        }
        let Some(boot) = held.seen.as_deref().map(|seen| seen.boot.clone()) else { return };
        let there = There { link: aim.link, boot, machine: held.name.clone() };
        let generation = self.panels.ports.refresh.open();
        self.machines.ports.listing = None;
        self.overlay = Some(Overlay::Ports(Box::new(Dialog::on(aim.workspace, generation, Some(there)))));
        self.watch_ports_dialog(Instant::now());
    }

    pub(in crate::app) fn watch_remote_ports(&mut self, workspace: u64, generation: u64, there: &There, now: Instant) {
        if self.remote_ports_place(there, workspace).is_none() {
            self.overlay = None;
            self.machines.ports.listing = None;
            return;
        }
        if !self.panels.ports.refresh.due(now, crate::app::ports_dialog::REFRESH_EVERY) {
            return;
        }
        self.panels.ports.refresh.begin(now);
        if let Err(why) = self.linked_held(there.link).map(|_| ()) {
            self.panels.ports.refresh.answered(generation);
            self.ports_listed(generation, Err(why));
            return;
        }
        let aim = RemoteAim { link: there.link, boot: there.boot.clone(), workspace };
        let command = Command::Ports(Ports { workspace: Some(workspace), all: false });
        match self.follow_machine(&aim, command, true) {
            Some(follow) => self.machines.ports.listing = Some((follow, generation)),
            None => {
                self.panels.ports.refresh.answered(generation);
            }
        }
    }

    pub(in crate::app) fn remote_ports_place(
        &self,
        there: &There,
        workspace: u64,
    ) -> Option<(&ProjectInfo, &WorkspaceInfo)> {
        let held = self.held_by(there.link)?;
        let seen = held.seen.as_deref().filter(|seen| seen.boot == there.boot)?;
        seen.projects
            .iter()
            .find_map(|project| project.workspaces.iter().find(|w| w.id == workspace).map(|w| (project, w)))
    }

    pub(in crate::app) fn remote_ports_heard(&mut self, followed: &Followed) -> bool {
        if let Some((follow, generation)) = &self.machines.ports.listing
            && follow.id() == followed.id
        {
            let generation = *generation;
            self.machines.ports.listing = None;
            let result = match &followed.event {
                Event::Line(text) => heard::ports(text).map_err(|refusal| refusal.message),
                Event::Ended(why) => Err(why.clone()),
            };
            self.ports_listed(generation, result);
            return true;
        }
        let Some(at) = self.machines.ports.stops.iter().position(|(follow, ..)| follow.id() == followed.id) else {
            return false;
        };
        let (_, target, machine) = self.machines.ports.stops.remove(at);
        self.remote_port_stopped(&target, &machine, &followed.event);
        true
    }

    fn remote_port_stopped(&mut self, target: &Target, machine: &str, event: &Event) {
        self.panels.ports.stopping = self.panels.ports.stopping.saturating_sub(1);
        self.panels.ports.refresh.again();
        let said = match event {
            Event::Line(text) => heard::port_stopped(text).map_err(|refusal| refusal.message),
            Event::Ended(why) => Err(why.clone()),
        };
        let (message, icon) = match said {
            Ok(stopped) if stopped.outcome == "stopped" => {
                (format!("stopped {} on {machine}", target.label()), ui::ToastIcon::Check)
            }
            Ok(stopped) if stopped.outcome == "killed" => {
                (format!("killed {} on {machine}: {KILLED_BECAUSE}", target.label()), ui::ToastIcon::Check)
            }
            Ok(_) => (format!("{} is still running on {machine}", target.label()), ui::ToastIcon::Bug),
            Err(why) => (format!("did not stop {} on {machine}: {why}", target.label()), ui::ToastIcon::Bug),
        };
        log::info!("machines", "port stopped there", port = target.port, stopped = icon == ui::ToastIcon::Check);
        self.messages.toast = Some(Toast::new(message, icon));
    }

    pub(in crate::app) fn stop_remote_ports(&mut self, there: &There, workspace: u64, targets: Vec<Target>) {
        let aim = RemoteAim { link: there.link, boot: there.boot.clone(), workspace };
        if let Err(why) = self.linked_held(there.link).map(|_| ()) {
            self.not_sent(&why);
            return;
        }
        for target in targets {
            let Some(started) = target.started else {
                let message = format!("did not stop {} on {}: {UNSTARTED}", target.label(), there.machine);
                self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
                continue;
            };
            let stop = StopPort { port: target.port, pid: Some(target.pid), started: Some(started) };
            let Some(follow) = self.follow_machine(&aim, Command::StopPort(stop), true) else { continue };
            self.panels.ports.stopping += 1;
            self.machines.ports.stops.push((follow, target, there.machine.clone()));
        }
    }

    pub(in crate::app) fn remote_port_place(&self, there: &There, workspace: u64, port: &Port) -> String {
        let tab = |pane: u64| {
            let (_, place) = self.remote_ports_place(there, workspace)?;
            let tab = place.tabs.iter().find(|tab| tab.panes.iter().any(|p| p.id == pane))?;
            Some(tab.custom_name.clone().unwrap_or_else(|| tab.name.clone()))
        };
        let pane = |id: u64| tab(id).map_or_else(|| format!("pane {id}"), |tab| format!("pane {id} · {tab}"));
        crate::app::ports_dialog::place_text(port, pane)
    }

    pub(in crate::app) fn go_remote_port(&mut self, there: &There, workspace: u64, pane: u64) {
        let tab = self.remote_ports_place(there, workspace).and_then(|(_, place)| {
            place.tabs.iter().find(|tab| tab.panes.iter().any(|p| p.id == pane)).map(|tab| tab.id)
        });
        self.overlay = None;
        if let Some(tab) = tab {
            self.select_on(there.link, RemoteItem::Tab(tab));
        }
    }

    pub(in crate::app) fn remote_ports_title(&self, there: &There, workspace: u64) -> String {
        let label = self
            .remote_ports_place(there, workspace)
            .map(|(_, w)| w.custom_name.clone().unwrap_or_else(|| w.name.clone()));
        label.map_or_else(
            || format!("Ports › {}", there.machine),
            |label| format!("Ports › {} › {label}", there.machine),
        )
    }
}
