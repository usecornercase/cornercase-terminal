use ratatui::layout::Position;
use serde_json::Value;

use super::relay::Asking;
use super::selection::RemoteItem;
use super::stream::Watched;
use super::{Held, MachineEvent, OLDER};
use crate::app::App;
use crate::app::dialogs::named;
use crate::app::event::AppEvent;
use crate::app::menus::{MenuAction, RENAME_WORKSPACE_LABEL};
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::app::worktrees;
use crate::control::contract::{Code, Refusal};
use crate::control::{self, WorkspaceInfo};
use crate::log::{self, Job, Level};
use crate::machines::act::{Act, Scope, WARNING};
use crate::machines::link::Down;
use crate::machines::send::folding;
use crate::machines::stream::orders::Order;
use crate::machines::{self, State};
use crate::ui;

pub(in crate::app) mod dialog;

const NOTHING_SENT: &str = "nothing was sent";
const UNLISTED: &str = "that machine is no longer listed";
const NOTHING_REMOVED: &str = "nothing was removed";
const WAITING: &str = "is still waiting for its last action";
const PANE: &str = "the pane";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct Aim {
    pub(in crate::app) link: u64,
    pub(in crate::app) era: u64,
    pub(in crate::app) workspace: u64,
}

#[derive(Default)]
pub(in crate::app) struct Acting {
    busy: Vec<(Aim, Act)>,
}

impl Acting {
    pub(in crate::app) fn busy(&self, link: u64, workspace: u64) -> bool {
        let on = |aim: &Aim, act: &Act| aim.workspace == workspace && act.scope() == Scope::Workspace;
        self.busy.iter().any(|(aim, act)| aim.link == link && on(aim, act))
    }

    pub(in crate::app) fn busy_tab(&self, link: u64, tab: u64) -> bool {
        self.busy.iter().any(|(aim, act)| aim.link == link && act.scope() == Scope::Tab(tab))
    }

    pub(in crate::app) fn closing(&self, link: u64, tab: u64) -> bool {
        self.busy.iter().any(|(aim, act)| aim.link == link && *act == Act::CloseTab(tab))
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(in crate::app) fn waiting(&self) -> usize {
        self.busy.len()
    }

    pub(in crate::app) fn removing(&self, link: u64, workspace: u64) -> bool {
        let gone = |act: &Act| matches!(act, Act::Close | Act::Remove { .. });
        self.busy.iter().any(|(aim, act)| aim.link == link && aim.workspace == workspace && gone(act))
    }
}

#[derive(Debug)]
pub(crate) struct Acted {
    aim: Aim,
    act: Act,
    label: String,
    machine: String,
    picks: u64,
    result: Result<Value, Refusal>,
}

pub(super) struct Target {
    machine: String,
    destination: String,
    boot: String,
    label: String,
}

impl App {
    pub(in crate::app) fn send_act(&mut self, aim: Aim, act: Act) {
        match self.aimed(aim, act.scope()) {
            Ok(target) => self.spawn_act(aim, act, target),
            Err(why) => self.not_sent(&why),
        }
    }

    pub(super) fn not_sent(&mut self, why: &str) {
        self.messages.toast = Some(Toast::new(format!("{why}; {NOTHING_SENT}"), ui::ToastIcon::Bug));
    }

    pub(in crate::app) fn unlinked(&self, held: &Held) -> String {
        let why = match held.status.down {
            Some(Down::Unreachable(_)) | None => format!("{} unreachable", held.name),
            Some(_) => self.machines.reason(held).unwrap_or_else(|| OLDER.into()),
        };
        machines::hide_destination(&why, &held.name, &held.destination)
    }

    pub(in crate::app) fn held_by(&self, link: u64) -> Option<&Held> {
        self.machines.held.iter().find(|held| held.link.id() == link)
    }

    pub(super) fn linked(&self, link: u64) -> bool {
        self.held_by(link).is_some_and(|held| held.status.state == State::Linked)
    }

    pub(in crate::app) fn aim_at(&self, link: u64, workspace: u64) -> Option<Aim> {
        self.held_by(link).map(|held| Aim { link, era: held.era, workspace })
    }

    pub(in crate::app) fn remote_menu(&mut self, aim: Aim, at: Position) {
        self.noted_menu(aim.link, &[MenuAction::RemoteRename(aim), MenuAction::RemotePorts(aim)], at);
    }

    pub(super) fn noted_menu(&mut self, link: u64, actions: &[MenuAction], at: Position) {
        let note = (!self.linked(link)).then_some(MenuAction::MachineNote(link));
        let actions = note.into_iter().chain(actions.iter().copied()).collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(in crate::app) fn remote_disabled(&self, action: MenuAction) -> bool {
        match action {
            MenuAction::MachineNote(_) => true,
            MenuAction::RemoteRename(aim) => {
                !self.linked(aim.link) || self.machines.acting.busy(aim.link, aim.workspace)
            }
            MenuAction::RemoteAgentFold(aim) => !self.linked(aim.link),
            MenuAction::RemotePorts(aim) => !self.linked(aim.link),
            other => self.remote_tab_disabled(other),
        }
    }

    pub(in crate::app) fn remote_menu_label(&self, action: MenuAction) -> Option<String> {
        match action {
            MenuAction::MachineNote(link) => {
                Some(self.held_by(link).map_or_else(|| UNLISTED.into(), |held| self.unlinked(held)))
            }
            MenuAction::RemoteRename(_) => Some(RENAME_WORKSPACE_LABEL.into()),
            MenuAction::RemotePorts(_) => Some("ports".into()),
            MenuAction::RemoteAgentFold(aim) => Some(folding(aim.collapse).into()),
            other => Self::remote_tab_label(other),
        }
    }

    pub(in crate::app) fn remote_action(&mut self, action: MenuAction) -> Option<MenuAction> {
        match action {
            MenuAction::RemoteRename(aim) if !self.remote_disabled(action) => self.remote_rename(aim),
            MenuAction::RemoteAgentFold(aim) if !self.remote_disabled(action) => self.send_agent_fold(aim),
            MenuAction::RemotePorts(aim) if !self.remote_disabled(action) => self.open_remote_ports(aim),
            MenuAction::RemoteRename(_)
            | MenuAction::RemotePorts(_)
            | MenuAction::RemoteAgentFold(_)
            | MenuAction::MachineNote(_) => {}
            other => return self.remote_tab_action(other),
        }
        None
    }

    pub(super) fn linked_held(&self, link: u64) -> Result<&Held, String> {
        let held = self.held_by(link).ok_or(UNLISTED)?;
        if held.status.state != State::Linked {
            return Err(self.unlinked(held));
        }
        Ok(held)
    }

    pub(super) fn aimed(&self, aim: Aim, scope: Scope) -> Result<Target, String> {
        let held = self.linked_held(aim.link)?;
        let (seen, workspace) = sighted(held, aim)?;
        let label = labelled(held, workspace, scope)?;
        let waiting = |label: &str| format!("{} {WAITING}", named(label, Some(&held.name)));
        let acting = &self.machines.acting;
        if acting.removing(aim.link, aim.workspace)
            || (scope == Scope::Workspace && acting.busy(aim.link, aim.workspace))
        {
            return Err(waiting(&workspace.name));
        }
        if let Scope::Tab(tab) = scope
            && acting.busy_tab(aim.link, tab)
        {
            return Err(waiting(&label));
        }
        Ok(Target { machine: held.name.clone(), destination: held.destination.clone(), boot: seen.boot.clone(), label })
    }

    fn spawn_act(&mut self, aim: Aim, act: Act, target: Target) {
        let Target { machine, destination, boot, label } = target;
        let asking = Asking { machine, destination, boot: Some(boot), timeout: act.timeout() };
        let job = Job::new(Level::Info, "machines", "act")
            .with("name", &asking.machine)
            .with("workspace", aim.workspace)
            .with("act", act.name());
        let picks = self.machines.picks;
        self.machines.acting.busy.push((aim, act.clone()));
        self.spawn_ask(job, asking, act.command(aim.workspace), move |result, asking, job| {
            let result = asking.heard(result, job).map(|value| warned(value, |why| asking.hide(why)));
            let machine = asking.machine.clone();
            AppEvent::Machine(MachineEvent::Acted(Acted { aim, act, label, machine, picks, result }))
        });
    }

    pub(super) fn follow_remote_acts(&mut self) {
        let busy = std::mem::take(&mut self.machines.acting.busy);
        self.machines.acting.busy = busy.into_iter().filter(|(aim, act)| self.sights(*aim, act.scope())).collect();
        let Some(Overlay::MachineWorkspace(dialog)) = &self.overlay else { return };
        if !self.sights(dialog.aim, dialog.kind.scope()) {
            log::info!("machines", "remote dialog dropped", link = dialog.aim.link, workspace = dialog.aim.workspace);
            self.overlay = None;
        }
    }

    pub(super) fn sights(&self, aim: Aim, scope: Scope) -> bool {
        self.held_by(aim.link)
            .is_some_and(|held| sighted(held, aim).and_then(|(_, workspace)| labelled(held, workspace, scope)).is_ok())
    }

    pub(super) fn acted(&mut self, acted: Acted) {
        let Acted { aim, act, label, machine, picks, result } = acted;
        let waits = result.is_ok() && matches!(act, Act::Close | Act::Remove { .. } | Act::CloseTab(_));
        if !waits {
            let busy = &mut self.machines.acting.busy;
            if let Some(at) = busy.iter().position(|(busy, done)| *busy == aim && *done == act) {
                busy.remove(at);
            }
        }
        if act == Act::Check {
            return self.remote_checked(aim, result, &label, &machine);
        }
        if let Ok(value) = &result {
            self.made(aim, picks, &act, value);
        }
        if let Some(toast) = told(&act, &label, &machine, &result) {
            self.messages.toast = Some(toast);
        }
    }

    fn made(&mut self, aim: Aim, picks: u64, act: &Act, value: &Value) {
        match act {
            Act::NewTab => {
                let held = self.held_by(aim.link).filter(|held| held.era == aim.era);
                let boot = held.and_then(|held| held.seen.as_deref()).map(|seen| seen.boot.clone());
                if let Some((tab, boot)) = value["tab"].as_u64().zip(boot).filter(|_| self.machines.picks == picks) {
                    self.await_remote(aim.link, boot, RemoteItem::Tab(tab));
                }
            }
            Act::Split { pane: split, .. } => {
                let shown = self.shown_remote().is_some_and(|s| (s.link, s.workspace) == (aim.link, aim.workspace));
                let framed = self.watched().and_then(Watched::live).map(|(_, screen)| &screen.panes);
                let there = framed.is_some_and(|panes| panes.iter().any(|framed| framed.pane == *split));
                let watched = self.machines.watched.as_mut().filter(|_| shown && there);
                if let Some((watched, pane)) = watched.zip(value["pane"].as_u64()) {
                    watched.order(Order::Focus(pane));
                }
            }
            _ => {}
        }
    }
}

fn seen_in(held: &Held, era: u64) -> Result<&control::Report, String> {
    held.seen.as_deref().filter(|_| held.era == era).ok_or_else(|| format!("{} restarted", held.name))
}

fn labelled(held: &Held, workspace: &WorkspaceInfo, scope: Scope) -> Result<String, String> {
    match scope {
        Scope::Workspace => Ok(workspace.name.clone()),
        Scope::Tab(id) => {
            let tab = workspace.tabs.iter().find(|tab| tab.id == id).ok_or_else(|| gone("tab", held))?;
            Ok(tab.name.clone())
        }
        Scope::Pane(id) => {
            let mut panes = workspace.tabs.iter().flat_map(|tab| &tab.panes);
            if panes.any(|pane| pane.id == id) { Ok(PANE.into()) } else { Err(gone("pane", held)) }
        }
    }
}

fn gone(what: &str, held: &Held) -> String {
    format!("that {what} is no longer on {}", held.name)
}

fn sighted(held: &Held, aim: Aim) -> Result<(&control::Report, &WorkspaceInfo), String> {
    let seen = seen_in(held, aim.era)?;
    let mut workspaces = seen.projects.iter().flat_map(|project| &project.workspaces);
    let workspace =
        workspaces.find(|workspace| workspace.id == aim.workspace).ok_or_else(|| gone("workspace", held))?;
    Ok((seen, workspace))
}

pub(super) fn sighted_pane(held: &Held, era: u64, pane: u64) -> Result<(), String> {
    let workspaces = seen_in(held, era)?.projects.iter().flat_map(|project| &project.workspaces);
    let mut panes = workspaces.flat_map(|workspace| &workspace.tabs).flat_map(|tab| &tab.panes);
    if panes.any(|there| there.id == pane) { Ok(()) } else { Err(gone("agent", held)) }
}

fn warned(mut value: Value, hidden: impl Fn(&str) -> String) -> Value {
    if let Some(Value::String(warning)) = value.get_mut(WARNING) {
        *warning = hidden(warning);
    }
    value
}

fn told(act: &Act, label: &str, machine: &str, result: &Result<Value, Refusal>) -> Option<Toast> {
    let named = named(label, Some(machine));
    match (act, result) {
        (Act::Check, Err(refusal)) if refusal.code != Code::StaleBoot => Some(Toast::new(
            format!("could not check {named}: {}; {NOTHING_REMOVED}", refusal.message),
            ui::ToastIcon::Bug,
        )),
        (_, Err(refusal)) => Some(refused(act.verb(), &named, machine, refusal)),
        (Act::Remove { .. }, Ok(value)) => Some(worktrees::removed(label, Some(machine), value[WARNING].as_str())),
        _ => None,
    }
}

pub(super) fn refused(verb: &str, named: &str, machine: &str, refusal: &Refusal) -> Toast {
    let message = if refusal.code == Code::StaleBoot {
        format!("{machine} restarted; nothing was changed there")
    } else {
        format!("could not {verb} {named}: {}", refusal.message)
    };
    Toast::new(message, ui::ToastIcon::Bug)
}
