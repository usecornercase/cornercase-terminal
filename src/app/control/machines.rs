use super::{Handled, conflict, json, malformed};
use crate::app::App;
use crate::app::machines::Update;
use crate::control::contract::{Code, Refusal};
use crate::control::{MachineList, MachineSet, Machines, machines::ThisComputer};
use crate::log;
use crate::machines::{self, DEFAULT_FOLDER, HERE_ICON, ICONS, Machine};
use crate::version;

fn unknown(name: &str) -> Refusal {
    Refusal::new(Code::NotFound, format!("no machine is called `{name}`; `cornercase machines` lists them"))
}

fn icon(text: &str) -> Result<String, Refusal> {
    let known: Vec<String> = ICONS.iter().map(|(glyph, name)| format!("{glyph} {name}")).collect();
    machines::icon_named(text)
        .map(String::from)
        .ok_or_else(|| malformed(format!("`{text}` is not an icon; use one of {}", known.join(", "))))
}

fn edited(old: &Machine, set: &MachineSet) -> Result<Machine, Refusal> {
    let given =
        |value: &Option<String>, old: &str| value.as_deref().map_or_else(|| old.to_string(), |v| v.trim().into());
    Ok(Machine {
        name: given(&set.new_name, &old.name),
        icon: set.icon.as_deref().map(icon).transpose()?.unwrap_or_else(|| old.icon.clone()),
        destination: given(&set.destination, &old.destination),
        folder: given(&set.folder, &old.folder),
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
    })
}

fn checked(machine: &Machine, others: &[Machine]) -> Result<(), Refusal> {
    machines::valid_name(&machine.name).map_err(malformed)?;
    machines::valid_destination(&machine.destination).map_err(malformed)?;
    machines::valid_folder(&machine.folder).map_err(malformed)?;
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

    pub(in crate::app) fn machine_list(&self) -> MachineList {
        let here = ThisComputer { icon: HERE_ICON.to_string(), version: version::CURRENT.into() };
        MachineList { here, machines: self.machine_infos() }
    }

    fn machine_at(&self, name: &str) -> Option<usize> {
        self.config.machines.iter().position(|machine| machine.name.eq_ignore_ascii_case(name.trim()))
    }

    fn set_machine(&mut self, set: &MachineSet) -> Result<(), Refusal> {
        self.readable_config()?;
        let at = self.machine_at(&set.name);
        let machine = match (at, set.create) {
            (Some(at), Some(true)) => {
                return Err(conflict(format!("a machine is already called `{}`", self.config.machines[at].name)));
            }
            (None, Some(false)) => return Err(unknown(&set.name)),
            (Some(at), _) => edited(&self.config.machines[at], set)?,
            (None, _) => added(set)?,
        };
        let mut others = self.config.machines.clone();
        if let Some(at) = at {
            others.remove(at);
        }
        checked(&machine, &others)?;
        log::info!("machines", "machine set", name = machine.name, new = at.is_none());
        self.change_config_for(|config| match at.filter(|at| *at < config.machines.len()) {
            Some(at) => config.machines[at] = machine,
            None => config.machines.push(machine),
        })
    }

    fn remove_machine(&mut self, name: &str) -> Result<(), Refusal> {
        self.readable_config()?;
        let at = self.machine_at(name).ok_or_else(|| unknown(name))?;
        let gone = self.config.machines[at].clone();
        log::info!("machines", "machine removed", name = gone.name);
        self.change_config_for(|config| {
            if let Some(found) = config.machines.iter().position(|machine| *machine == gone) {
                config.machines.remove(found);
            }
        })
    }

    fn update_machine_request(&mut self, client: u64, name: &str) -> Handled {
        match self.update_machine(name) {
            Update::Started(link) => {
                let key = self.clients.requests.key();
                self.machines.waiting.push((link, key));
                self.pend_reading(client, key);
                Ok(None)
            }
            Update::Running => Err(conflict(format!("`{name}` is already updating"))),
            Update::Unknown if self.machine_at(name).is_none() => Err(unknown(name)),
            Update::NotOlder | Update::Unknown => {
                Err(conflict(format!("`{name}` is not older than this cornercase: nothing to install")))
            }
        }
    }

    fn retry_machines(&self, name: Option<&str>) -> Result<(), Refusal> {
        if let Some(name) = name.filter(|name| self.machine_at(name).is_none()) {
            return Err(unknown(name));
        }
        let picked = |held: &&crate::app::machines::Held| name.is_none_or(|name| held.name.eq_ignore_ascii_case(name));
        for held in self.machines.held.iter().filter(picked) {
            log::info!("machines", "retrying", name = held.name);
            held.link.poke();
        }
        Ok(())
    }
}
