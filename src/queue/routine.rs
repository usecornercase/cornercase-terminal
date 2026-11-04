use super::repeat::Repeat;
use super::{Entry, GRACE, Place, Queue, RUNS_KEPT, State, Task, missing};

fn not_a_routine(id: u64) -> String {
    format!("entry {id} is not a routine")
}

impl Queue {
    pub(crate) fn runs_of(&self, routine: u64) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |entry| entry.task.routine == Some(routine))
    }

    fn routine(&self, id: u64) -> Result<(Entry, Repeat), String> {
        let entry = self.entry(id).ok_or_else(|| missing(id))?;
        let repeat = entry.task.repeat.ok_or_else(|| not_a_routine(id))?;
        Ok((entry.clone(), repeat))
    }

    fn add_run(&mut self, routine: &Entry, at: i64, state: State) -> Result<u64, String> {
        let id = self.next_id.checked_add(1).ok_or("the queue has no ids left")?;
        self.next_id = id;
        let place = Place { project: routine.task.place.project.clone(), ..Place::default() };
        let task = Task { at, repeat: None, paused: false, routine: Some(routine.id), place, ..routine.task.clone() };
        self.entries.push(Entry { id, task, state });
        self.sort();
        self.trim(routine.id);
        Ok(id)
    }

    fn trim(&mut self, routine: u64) {
        let mut runs: Vec<u64> = self.runs_of(routine).map(|run| run.id).collect();
        runs.sort_unstable();
        let extra = runs.len().saturating_sub(RUNS_KEPT);
        let mut dropped = 0;
        for id in runs {
            if dropped == extra {
                break;
            }
            let settled = self.entry(id).is_some_and(|run| !matches!(run.state, State::Pending | State::Starting));
            if settled {
                self.entries.retain(|entry| entry.id != id);
                dropped += 1;
            }
        }
    }

    fn fire_one(&mut self, routine: &Entry, repeat: Repeat, now: i64, missed: &mut Vec<u64>) {
        let task = &routine.task;
        let walk = match repeat.walk(&task.zone, task.at, now) {
            Ok(walk) => walk,
            Err(reason) => {
                self.add_run(routine, task.at, State::Failed { reason }).ok();
                if let Some(entry) = self.entry_mut(routine.id) {
                    entry.task.paused = true;
                }
                return;
            }
        };
        if walk.skipped || now - walk.last > GRACE {
            missed.extend(self.add_run(routine, task.at, State::Missed));
        }
        if now - walk.last <= GRACE {
            self.add_run(routine, walk.last, State::Pending).ok();
        }
        if let Some(entry) = self.entry_mut(routine.id) {
            entry.task.at = walk.next;
        }
        self.sort();
    }

    pub(crate) fn fire(&mut self, now: i64) -> Vec<u64> {
        let due: Vec<Entry> = self
            .entries
            .iter()
            .filter(|entry| entry.is_routine() && !entry.task.paused && entry.task.at <= now)
            .cloned()
            .collect();
        let mut missed = Vec::new();
        for routine in due {
            if let Some(repeat) = routine.task.repeat {
                self.fire_one(&routine, repeat, now, &mut missed);
            }
        }
        missed
    }

    pub(crate) fn run_now(&mut self, routine: u64, now: i64) -> Result<u64, String> {
        let (entry, _) = self.routine(routine)?;
        self.add_run(&entry, now, State::Pending)
    }

    pub(crate) fn pause(&mut self, id: u64, paused: bool, now: i64) -> Result<(), String> {
        let (entry, repeat) = self.routine(id)?;
        let at = if paused || entry.task.at > now {
            entry.task.at
        } else {
            repeat.walk(&entry.task.zone, entry.task.at, now)?.next
        };
        let entry = self.entry_mut(id).ok_or_else(|| missing(id))?;
        entry.task.paused = paused;
        entry.task.at = at;
        self.sort();
        Ok(())
    }
}
