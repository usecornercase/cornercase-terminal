use std::fmt::Write as _;
use std::time::SystemTime;

use super::args::Print;
use super::args::machines::{MachinesAction, MachinesArgs};
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{Command, MachineList, MachineSet, Machines};
use crate::error::Result;
use crate::text;

pub(super) fn run(args: &MachinesArgs) -> Result<()> {
    let (request, print) = request(args);
    let value = ask("machines", Command::Machines(request))?;
    let list: MachineList = answer(value.clone())?;
    if print.json { print_json(&value) } else { print_out(&list_text(&list, SystemTime::now())) }
}

pub(crate) fn request(args: &MachinesArgs) -> (Machines, &Print) {
    match &args.action {
        None => (Machines::List, &args.print),
        Some(MachinesAction::Add(add)) => {
            let set = MachineSet {
                name: add.name.clone(),
                icon: add.icon.clone(),
                destination: Some(add.destination.clone()),
                folder: add.folder.clone(),
                create: Some(true),
                ..MachineSet::default()
            };
            (Machines::Set(set), &add.print)
        }
        Some(MachinesAction::Edit(edit)) => {
            let set = MachineSet {
                name: edit.name.clone(),
                new_name: edit.new_name.clone(),
                icon: edit.icon.clone(),
                destination: edit.destination.clone(),
                folder: edit.folder.clone(),
                create: Some(false),
            };
            (Machines::Set(set), &edit.print)
        }
        Some(MachinesAction::Remove(remove)) => (Machines::Remove(remove.name.clone()), &remove.print),
        Some(MachinesAction::Update(update)) => (Machines::Update(update.name.clone()), &update.print),
        Some(MachinesAction::Retry(retry)) => (Machines::Retry(retry.name.clone()), &retry.print),
    }
}

pub(crate) fn list_text(list: &MachineList, now: SystemTime) -> String {
    let mut text = format!("{} this computer  {}\n", list.here.icon, list.here.version);
    let width = |cells: fn(&crate::machines::Info) -> &str| {
        list.machines.iter().map(|machine| text::cells(cells(machine))).max().unwrap_or_default()
    };
    let (names, destinations) = (width(|m| &m.name), width(|m| &m.destination));
    for machine in &list.machines {
        let name = format!("{:names$}", machine.name);
        let destination = format!("{:destinations$}", machine.destination);
        let (mark, icon, folder, note) = (machine.mark(), &machine.icon, &machine.folder, machine.note(now));
        let _ = writeln!(text, "{mark} {icon} {name}  {destination}  {folder}  {note}");
        if let Some(error) = &machine.error {
            let _ = writeln!(text, "    {error}");
        }
    }
    text
}

#[cfg(test)]
mod tests;
