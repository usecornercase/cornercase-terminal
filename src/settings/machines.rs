use crate::machines::{self, HERE_ICON, Info, Machine, State};
use crate::text::{cells, pad_cells, truncate_cells_left};
use crate::version;

use super::{Action, Row, Settings};

const FOLDER_WIDTH: usize = 24;
const HINT: &str = "enter edits a machine · × removes it, and its server keeps running there";
const HERE_HINT: &str = "this computer runs the server you are using; it links to each machine below";
const UPDATE_NOTE: &str = "installs this version there with cornercase update; nothing restarts";
const ADD_NOTE: &str = "another computer you reach with ssh";

impl Settings {
    pub(super) fn machine_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Here];
        for at in 0..self.config.machines.len() {
            rows.push(Row::Machine(at));
            if self.machine_info(at).is_some_and(|info| info.problem.is_none() && info.state == State::Older) {
                rows.push(Row::UpdateMachine(at));
            }
        }
        rows.push(Row::AddMachine);
        rows
    }

    pub(super) fn selected_machine(&self) -> Option<(Machine, bool)> {
        match self.row()? {
            Row::Machine(at) => Some((self.config.machines.get(at)?.clone(), false)),
            Row::UpdateMachine(at) => Some((self.config.machines.get(at)?.clone(), true)),
            _ => None,
        }
    }

    pub(super) fn select_machine(&mut self, (machine, update): (Machine, bool)) {
        let Some(at) = self.config.machines.iter().position(|listed| *listed == machine) else { return };
        let rows = self.rows();
        let wanted = if update { Row::UpdateMachine(at) } else { Row::Machine(at) };
        let found = rows.iter().position(|row| *row == wanted);
        if let Some(i) = found.or_else(|| rows.iter().position(|row| *row == Row::Machine(at))) {
            self.cursor = i;
        }
    }

    pub(crate) fn take_machines(&mut self, infos: Vec<Info>) {
        let was = self.row();
        self.machines = infos;
        let rows = self.rows();
        let kept = was.and_then(|was| {
            let same = rows.iter().position(|row| *row == was);
            same.or_else(|| match was {
                Row::UpdateMachine(at) => rows.iter().position(|row| *row == Row::Machine(at)),
                _ => None,
            })
        });
        if let Some(at) = kept {
            self.cursor = at;
        }
    }

    pub(crate) fn machine_info(&self, at: usize) -> Option<Info> {
        let machine = self.config.machines.get(at)?;
        let listed =
            self.machines.get(at).filter(|info| info.name == machine.name && info.destination == machine.destination);
        Some(listed.cloned().unwrap_or_else(|| {
            let problem = machine.problem(&self.config.machines[..at]);
            Info::of(machine, None, problem, None)
        }))
    }

    pub(super) fn machine_row(&self, row: &Row) -> (String, String, String, bool) {
        match row {
            Row::Here => (format!("  {HERE_ICON} this computer"), String::new(), version::CURRENT.into(), false),
            Row::Machine(at) => self.machine_text(*at),
            Row::UpdateMachine(at) => {
                let updating = self.machine_info(*at).is_some_and(|info| info.updating);
                let note = if updating { "updating…" } else { UPDATE_NOTE };
                ("    update…".into(), String::new(), note.into(), false)
            }
            _ => ("+ add machine".into(), String::new(), ADD_NOTE.into(), false),
        }
    }

    fn machine_text(&self, at: usize) -> (String, String, String, bool) {
        let (Some(machine), Some(info)) = (self.config.machines.get(at), self.machine_info(at)) else {
            return (String::new(), String::new(), String::new(), false);
        };
        let label = format!("{} {}", machines::known_icon(&machine.icon), machine.name);
        (label, self.place(machine), info.note(self.now), info.problem.is_some())
    }

    fn longest(&self, text: fn(&Machine) -> &str, most: usize) -> usize {
        self.config.machines.iter().map(|machine| cells(text(machine))).max().unwrap_or_default().min(most)
    }

    fn place(&self, machine: &Machine) -> String {
        let width = self.longest(|machine| &machine.destination, usize::MAX);
        pad_cells(&machine.destination, width)
    }

    pub(super) fn machine_tail(&self, row: &Row) -> String {
        let Some(machine) = (match row {
            Row::Machine(at) => self.config.machines.get(*at),
            _ => None,
        }) else {
            return String::new();
        };
        let width = self.longest(|machine| &machine.folder, FOLDER_WIDTH);
        pad_cells(&truncate_cells_left(&machine.folder, width), width)
    }

    pub(super) fn machine_mark(&self, row: &Row) -> Option<State> {
        let Row::Machine(at) = row else { return None };
        self.machine_info(*at).map(|info| if info.problem.is_some() { State::Unreachable } else { info.state })
    }

    pub(super) fn activate_machine(row: &Row) -> Action {
        match row {
            Row::Machine(at) => Action::EditMachine(*at),
            Row::UpdateMachine(at) => Action::UpdateMachine(*at),
            Row::AddMachine => Action::AddMachine,
            _ => Action::None,
        }
    }

    pub(super) fn machine_removal(row: &Row) -> Option<Action> {
        match row {
            Row::Machine(at) => Some(Action::RemoveMachine(*at)),
            _ => None,
        }
    }

    pub(super) fn machine_hint(&self) -> String {
        match self.row() {
            Some(Row::Machine(at) | Row::UpdateMachine(at)) => {
                self.machine_info(at).and_then(|info| info.error).unwrap_or_else(|| HINT.into())
            }
            Some(Row::Here) => HERE_HINT.into(),
            _ => HINT.into(),
        }
    }
}
