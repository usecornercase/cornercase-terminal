use ratatui::layout::Position;
use serde_json::Value;

use super::{Held, MachineEvent, OLDER};
use crate::app::App;
use crate::app::dialogs::named;
use crate::app::event::AppEvent;
use crate::app::menus::{MenuAction, RENAME_WORKSPACE_LABEL};
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::control::contract::{Code, Refusal};
use crate::control::{self, WorkspaceInfo};
use crate::error;
use crate::log::{self, Job, Level};
use crate::machines::act::{self, Act};
use crate::machines::link::Down;
use crate::machines::{self, State, ask};
use crate::ui;

pub(in crate::app) mod dialog;

const NOTHING_SENT: &str = "nothing was sent";
const UNLISTED: &str = "that machine is no longer listed";
const NOTHING_REMOVED: &str = "nothing was removed";
const WAITING: &str = "is still waiting for its last action";

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
        self.busy.iter().any(|(aim, _)| aim.link == link && aim.workspace == workspace)
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
    result: Result<Value, Refusal>,
}

struct Target {
    machine: String,
    destination: String,
    boot: String,
    label: String,
}

impl App {
    pub(in crate::app) fn send_act(&mut self, aim: Aim, act: Act) {
        match self.aimed(aim) {
            Ok(target) => self.spawn_act(aim, act, target),
            Err(why) => self.not_sent(&why),
        }
    }

    fn not_sent(&mut self, why: &str) {
        self.messages.toast = Some(Toast::new(format!("{why}; {NOTHING_SENT}"), ui::ToastIcon::Bug));
    }

    fn unlinked(&self, held: &Held) -> String {
        let why = match held.status.down {
            Some(Down::Unreachable(_)) | None => format!("{} unreachable", held.name),
            Some(_) => self.machines.reason(held).unwrap_or_else(|| OLDER.into()),
        };
        machines::hide_destination(&why, &held.name, &held.destination)
    }

    fn held_by(&self, link: u64) -> Option<&Held> {
        self.machines.held.iter().find(|held| held.link.id() == link)
    }

    fn linked(&self, link: u64) -> bool {
        self.held_by(link).is_some_and(|held| held.status.state == State::Linked)
    }

    pub(in crate::app) fn aim_at(&self, link: u64, workspace: u64) -> Option<Aim> {
        self.held_by(link).map(|held| Aim { link, era: held.era, workspace })
    }

    pub(in crate::app) fn remote_menu(&mut self, aim: Aim, at: Position) {
        let note = (!self.linked(aim.link)).then_some(MenuAction::MachineNote(aim.link));
        let actions = note.into_iter().chain([MenuAction::RemoteRename(aim)]).collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(in crate::app) fn remote_disabled(&self, action: MenuAction) -> bool {
        match action {
            MenuAction::MachineNote(_) => true,
            MenuAction::RemoteRename(aim) => {
                !self.linked(aim.link) || self.machines.acting.busy(aim.link, aim.workspace)
            }
            _ => false,
        }
    }

    pub(in crate::app) fn remote_menu_label(&self, action: MenuAction) -> Option<String> {
        match action {
            MenuAction::MachineNote(link) => {
                Some(self.held_by(link).map_or_else(|| UNLISTED.into(), |held| self.unlinked(held)))
            }
            MenuAction::RemoteRename(_) => Some(RENAME_WORKSPACE_LABEL.into()),
            _ => None,
        }
    }

    pub(in crate::app) fn remote_action(&mut self, action: MenuAction) -> Option<MenuAction> {
        match action {
            MenuAction::RemoteRename(aim) if !self.remote_disabled(action) => self.remote_rename(aim),
            MenuAction::RemoteRename(_) | MenuAction::MachineNote(_) => {}
            other => return Some(other),
        }
        None
    }

    fn aimed(&self, aim: Aim) -> Result<Target, String> {
        let held = self.held_by(aim.link).ok_or(UNLISTED)?;
        if held.status.state != State::Linked {
            return Err(self.unlinked(held));
        }
        let (seen, workspace) = sighted(held, aim)?;
        if self.machines.acting.busy(aim.link, aim.workspace) {
            return Err(format!("{} {WAITING}", named(&workspace.name, Some(&held.name))));
        }
        Ok(Target {
            machine: held.name.clone(),
            destination: held.destination.clone(),
            boot: seen.boot.clone(),
            label: workspace.name.clone(),
        })
    }

    fn spawn_act(&mut self, aim: Aim, act: Act, target: Target) {
        let Target { machine, destination, boot, label } = target;
        let (command, timeout) = (act.command(aim.workspace), act.timeout());
        let connect = self.machines.connector(&destination);
        let job = Job::new(Level::Info, "machines", "act")
            .with("name", &machine)
            .with("workspace", aim.workspace)
            .with("act", act.verb())
            .slow_after(timeout);
        self.machines.acting.busy.push((aim, act.clone()));
        let there = machine.clone();
        let work = move || Ok(ask::ask(connect.as_ref(), &there, command, Some(boot), timeout));
        self.spawn_job(job, work, move |result, job| {
            let hidden = |why: &str| machines::hide_destination(why, &machine, &destination);
            let result = answered(result).map_err(|refusal| Refusal::new(refusal.code, hidden(&refusal.message)));
            if let Err(refusal) = &result {
                job.fail(&refusal.message);
            }
            Some(AppEvent::Machine(MachineEvent::Acted(Acted { aim, act, label, machine, result })))
        });
    }

    pub(super) fn follow_remote_acts(&mut self) {
        let busy = std::mem::take(&mut self.machines.acting.busy);
        self.machines.acting.busy = busy.into_iter().filter(|(aim, _)| self.sights(*aim)).collect();
        let Some(Overlay::MachineWorkspace(dialog)) = &self.overlay else { return };
        if !self.sights(dialog.aim) {
            log::info!("machines", "remote dialog dropped", link = dialog.aim.link, workspace = dialog.aim.workspace);
            self.overlay = None;
        }
    }

    fn sights(&self, aim: Aim) -> bool {
        self.held_by(aim.link).is_some_and(|held| sighted(held, aim).is_ok())
    }

    pub(super) fn acted(&mut self, acted: Acted) {
        let Acted { aim, act, label, machine, result } = acted;
        let waits = result.is_ok() && matches!(act, Act::Close | Act::Remove { .. });
        if !waits {
            self.machines.acting.busy.retain(|(busy, _)| *busy != aim);
        }
        if act == Act::Check {
            return self.remote_checked(aim, result, &label, &machine);
        }
        if let Some(toast) = told(&act, &label, &machine, &result) {
            self.messages.toast = Some(toast);
        }
    }
}

fn sighted(held: &Held, aim: Aim) -> Result<(&control::Report, &WorkspaceInfo), String> {
    let restarted = || format!("{} restarted", held.name);
    let seen = held.seen.as_deref().filter(|_| held.era == aim.era).ok_or_else(restarted)?;
    let mut workspaces = seen.projects.iter().flat_map(|project| &project.workspaces);
    let workspace = workspaces
        .find(|workspace| workspace.id == aim.workspace)
        .ok_or_else(|| format!("that workspace is no longer on {}", held.name))?;
    Ok((seen, workspace))
}

fn answered(result: error::Result<Result<String, String>>) -> Result<Value, Refusal> {
    match result {
        Ok(Ok(text)) => act::heard(&text),
        Ok(Err(why)) => Err(Refusal::new(Code::Failed, why)),
        Err(error) => Err(Refusal::new(Code::Failed, error.to_string())),
    }
}

fn told(act: &Act, label: &str, machine: &str, result: &Result<Value, Refusal>) -> Option<Toast> {
    let named = named(label, Some(machine));
    let (message, icon) = match (act, result) {
        (_, Err(refusal)) if refusal.code == Code::StaleBoot => {
            (format!("{machine} restarted; nothing was changed there"), ui::ToastIcon::Bug)
        }
        (Act::Check, Err(refusal)) => {
            (format!("could not check {named}: {}; {NOTHING_REMOVED}", refusal.message), ui::ToastIcon::Bug)
        }
        (_, Err(refusal)) => (format!("could not {} {named}: {}", act.verb(), refusal.message), ui::ToastIcon::Bug),
        (Act::Remove { .. }, Ok(_)) => (format!("removed {named}"), ui::ToastIcon::Check),
        _ => return None,
    };
    Some(Toast::new(message, icon))
}
