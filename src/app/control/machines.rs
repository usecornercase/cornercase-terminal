use super::{Handled, conflict, json, malformed};
use crate::app::App;
use crate::app::machine_links::{RemoteItem, Update};
use crate::control::contract::{Code, Refusal};
use crate::control::{FocusMachine, Item, MachineList, MachineSet, Machines, machines::ThisComputer};
use crate::log;
use crate::machines::{self, DEFAULT_FOLDER, HERE_ICON, ICONS, Machine};
use crate::version;

pub(in crate::app) fn unknown(name: &str) -> Refusal {
    Refusal::new(Code::NotFound, format!("no machine is called `{name}`; `cornercase machines` lists them"))
}

fn not_shown(name: &str) -> Refusal {
    let why = format!("`{name}` is not shown here, so it has no link: tick show its work here in Settings › Machines");
    Refusal::new(Code::NotFound, why)
}

fn remote_item(item: Item) -> Result<RemoteItem, Refusal> {
    match item {
        Item::Workspace(id) => Ok(RemoteItem::Workspace(id)),
        Item::Tab(id) => Ok(RemoteItem::Tab(id)),
        other => Err(malformed(format!("`{other}` is not a machine's workspace or tab: say one of them"))),
    }
}

fn missing(name: &str, item: RemoteItem) -> Refusal {
    let what = match item {
        RemoteItem::Workspace(id) => format!("workspace {id}"),
        RemoteItem::Tab(id) => format!("tab {id}"),
    };
    Refusal::new(Code::NotFound, format!("`{name}` has no {what}: `cornercase status` lists its workspaces and tabs"))
}

fn icon(text: &str) -> Result<String, Refusal> {
    let known: Vec<String> = ICONS.iter().map(|(glyph, name)| format!("{glyph} {name}")).collect();
    machines::icon_named(text)
        .map(String::from)
        .ok_or_else(|| malformed(format!("`{text}` is not an icon; use one of {}", known.join(", "))))
}

fn command_of(set: &MachineSet) -> Option<String> {
    set.command.as_deref().map(str::trim).filter(|command| !command.is_empty()).map(String::from)
}

fn edited(old: &Machine, set: &MachineSet) -> Result<Machine, Refusal> {
    let given =
        |value: &Option<String>, old: &str| value.as_deref().map_or_else(|| old.to_string(), |v| v.trim().into());
    let destination = given(&set.destination, &old.destination);
    Ok(Machine {
        name: given(&set.new_name, &old.name),
        icon: set.icon.as_deref().map(icon).transpose()?.unwrap_or_else(|| old.icon.clone()),
        folder: given(&set.folder, &old.folder),
        command: if set.command.is_some() { command_of(set) } else { old.command.clone() },
        shown: set.shown.unwrap_or(old.shown),
        unreached: old.unreached || destination != old.destination,
        destination,
    })
}

fn added(set: &MachineSet) -> Result<Machine, Refusal> {
    if set.new_name.is_some() {
        return Err(unknown(set.name.trim()));
    }
    Ok(Machine {
        name: set.name.trim().into(),
        icon: set.icon.as_deref().map_or_else(|| Ok(ICONS[0].0.to_string()), icon)?,
        destination: set.destination.as_deref().unwrap_or_default().trim().into(),
        folder: set.folder.as_deref().map_or(DEFAULT_FOLDER, str::trim).into(),
        command: command_of(set),
        shown: set.shown.unwrap_or(true),
        unreached: true,
    })
}

fn position(machines: &[Machine], name: &str) -> Option<usize> {
    machines.iter().position(|machine| machine.name.eq_ignore_ascii_case(name.trim()))
}

fn resolved(machines: &[Machine], set: &MachineSet) -> Result<(Option<usize>, Machine), Refusal> {
    let at = position(machines, &set.name);
    let machine = match (at, set.create) {
        (Some(at), Some(true)) => {
            return Err(conflict(format!("a machine is already called `{}`", machines[at].name)));
        }
        (None, Some(false)) => return Err(unknown(&set.name)),
        (Some(at), _) => edited(&machines[at], set)?,
        (None, _) => added(set)?,
    };
    let mut others = machines.to_vec();
    if let Some(at) = at {
        others.remove(at);
    }
    checked(&machine, &others)?;
    Ok((at, machine))
}

fn checked(machine: &Machine, others: &[Machine]) -> Result<(), Refusal> {
    machines::valid_name(&machine.name).map_err(malformed)?;
    machines::valid_destination(&machine.destination).map_err(malformed)?;
    machines::valid_folder(&machine.folder).map_err(malformed)?;
    machines::valid_command(machine.command.as_deref().unwrap_or_default()).map_err(malformed)?;
    let taken = machines::name_taken(others, &machine.name);
    match taken.or_else(|| machines::destination_taken(others, &machine.destination)) {
        Some(taken) => Err(conflict(taken)),
        None => Ok(()),
    }
}

impl App {
    pub(super) fn machines_request(&mut self, client: u64, request: Machines) -> Handled {
        match request {
            Machines::List => {}
            Machines::Set(set) => self.set_machine(&set)?,
            Machines::Remove(name) => self.remove_machine(&name)?,
            Machines::Update(name) => return self.update_machine_request(client, &name),
            Machines::Retry(name) => self.retry_machines(name.as_deref())?,
        }
        Ok(Some(json(&self.machine_list())))
    }

    pub(super) fn focus_machine_request(&mut self, focus: &FocusMachine) -> Handled {
        let item = remote_item(focus.item)?;
        let name = focus.machine.trim();
        let at = self.linkable(name)?;
        let name = self.config.machines[at].name.clone();
        if !self.sent_workspaces(&name) {
            let message = format!("`{name}` has not sent its workspaces yet: `cornercase machines` says how it is");
            return Err(Refusal::new(Code::Unavailable, message));
        }
        self.reported_run(&name, focus.machine_boot.as_deref())?;
        if !self.select_remote(&name, item) {
            return Err(missing(&name, item));
        }
        self.screen.nav = None;
        Ok(Some(json(&self.shown_remote_info())))
    }

    pub(in crate::app) fn machine_list(&self) -> MachineList {
        let here = ThisComputer { icon: HERE_ICON.to_string(), version: version::CURRENT.into() };
        MachineList { here, machines: self.machine_infos() }
    }

    fn machine_at(&self, name: &str) -> Option<usize> {
        position(&self.config.machines, name)
    }

    fn linkable(&self, name: &str) -> Result<usize, Refusal> {
        let at = self.machine_at(name).ok_or_else(|| unknown(name))?;
        let machine = &self.config.machines[at];
        match machines::problems(&self.config.machines).swap_remove(at) {
            Some(problem) => {
                let name = &machine.name;
                Err(Refusal::new(Code::NotFound, format!("`{name}` has no link until it is fixed: {problem}")))
            }
            None if !machine.shown => Err(not_shown(&machine.name)),
            None => Ok(at),
        }
    }

    pub(in crate::app) fn no_link(&self, name: &str) -> Refusal {
        match self.machine_at(name) {
            Some(at) if !self.config.machines[at].shown => not_shown(&self.config.machines[at].name),
            _ => unknown(name),
        }
    }

    fn set_machine(&mut self, set: &MachineSet) -> Result<(), Refusal> {
        let mut refused = None;
        self.change_config_for(|config| match resolved(&config.machines, set) {
            Ok((at, machine)) => {
                log::info!("machines", "machine set", name = machine.name, new = at.is_none());
                match at {
                    Some(at) => config.machines[at] = machine,
                    None => config.machines.push(machine),
                }
            }
            Err(refusal) => refused = Some(refusal),
        })?;
        refused.map_or(Ok(()), Err)
    }

    fn remove_machine(&mut self, name: &str) -> Result<(), Refusal> {
        let mut found = false;
        self.change_config_for(|config| {
            if let Some(at) = position(&config.machines, name) {
                let gone = config.machines.remove(at);
                log::info!("machines", "machine removed", name = gone.name);
                found = true;
            }
        })?;
        if found { Ok(()) } else { Err(unknown(name)) }
    }

    fn update_machine_request(&mut self, client: u64, name: &str) -> Handled {
        let name = name.trim();
        self.follow_machines();
        match self.update_machine(name) {
            Update::Started(link) => {
                let key = self.clients.requests.key();
                self.machines.wait_for_update(link, key);
                self.pend_reading(client, key);
                Ok(None)
            }
            Update::Running => Err(conflict(format!("`{name}` is already updating"))),
            Update::NotOlder | Update::Unknown => Err(self.linkable(name).err().unwrap_or_else(|| {
                conflict(format!("`{name}` is not older than this cornercase: nothing to install"))
            })),
        }
    }

    fn retry_machines(&mut self, name: Option<&str>) -> Result<(), Refusal> {
        let name = name.map(str::trim);
        if let Some(name) = name {
            self.linkable(name)?;
        }
        self.follow_machines();
        self.machines.retry(name);
        Ok(())
    }
}
