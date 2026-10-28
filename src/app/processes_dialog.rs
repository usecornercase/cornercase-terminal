use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::App;
use super::event::AppEvent;
use super::input::{ListPointer, list_pointer, wheel_span};
use super::overlay::Overlay;
use super::refresh::Refresh;
use super::toast::Toast;
use super::worktrees_dialog::wrap;
use crate::control::contract::Refusal;
use crate::control::{Item, ProcessStopped};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::process::command_line;
use crate::process::escalation::Outcome;
use crate::processes::dialog::{Action, AgentTag, Dialog, Hit, Line as At, PaneRow, ProcRow, Sort, Step, Target};
use crate::processes::stop::{self, Stopped};
use crate::processes::{self, PaneTree, Proc, Sample, Times, sum_cpu, sum_memory};
use crate::project::Hierarchy;
use crate::text::{CRUMB_SEPARATOR, memory_size};
use crate::ui;

const REFRESH_EVERY: Duration = Duration::from_secs(2);

const BASELINE: Duration = Duration::from_millis(500);

const COMMAND_WIDTH: usize = 40;

#[derive(Debug, Default)]
pub(super) struct ProcessesState {
    refresh: Refresh,
    previous: Option<Times>,
    sort: Sort,
    stopping: usize,
}

#[derive(Debug)]
pub(crate) enum ProcessesEvent {
    Answered { request: u64, interval: u64, result: Result<Sample> },
    Sampled { generation: u64, result: Result<Sample> },
    Stopped(Vec<Stopped>),
    StopAnswered { request: u64, result: std::result::Result<ProcessStopped, Refusal> },
}

impl App {
    pub(super) fn open_processes(&mut self) {
        let generation = self.panels.processes.refresh.open();
        self.panels.processes.previous = None;
        self.overlay = Some(Overlay::Processes(Box::new(Dialog::new(generation, self.panels.processes.sort))));
        self.watch_processes_dialog(Instant::now());
    }

    pub(super) fn watch_processes_dialog(&mut self, now: Instant) {
        let Some(Overlay::Processes(dialog)) = &self.overlay else { return };
        if !self.panels.processes.refresh.due(now, REFRESH_EVERY) {
            return;
        }
        let generation = dialog.generation;
        self.panels.processes.refresh.begin(now);
        let places = self.process_places();
        let previous = self.panels.processes.previous.take();
        let job = Job::new(Level::Debug, "processes", "sample");
        self.spawn_job(
            job,
            move || Ok(processes::sample(&places, previous, BASELINE)),
            move |result, job| {
                job.note("panes", result.as_ref().map_or(0, |sample| sample.panes.len()));
                Some(AppEvent::Processes(ProcessesEvent::Sampled { generation, result }))
            },
        );
    }

    pub(super) fn processes_answered(&mut self, event: ProcessesEvent, area: Rect) {
        match event {
            ProcessesEvent::Answered { request, interval, result } => self.processes_found(request, interval, result),
            ProcessesEvent::Sampled { generation, result } => self.processes_sampled(generation, result, area),
            ProcessesEvent::Stopped(stopped) => self.processes_stopped(&stopped),
            ProcessesEvent::StopAnswered { request, result } => self.process_stop_answered(request, result),
        }
    }

    fn processes_sampled(&mut self, generation: u64, result: Result<Sample>, area: Rect) {
        if !self.panels.processes.refresh.answered(generation) {
            return;
        }
        let keep_order = self.pointer_over_processes(area);
        let found = match result {
            Ok(sample) => {
                let rows = self.process_rows(&sample);
                self.panels.processes.previous = Some(sample.times);
                Ok(rows)
            }
            Err(error) => Err(error.to_string()),
        };
        if let Some(Overlay::Processes(dialog)) = &mut self.overlay
            && dialog.generation == generation
        {
            dialog.listed(found, keep_order);
        }
    }

    fn pointer_over_processes(&self, area: Rect) -> bool {
        let (_, list) = ui::processes_room(area, area.width < ui::COMPACT_WIDTH);
        self.screen.hover.is_some_and(|at| list.contains(at))
    }

    fn processes_stopped(&mut self, stopped: &[Stopped]) {
        self.panels.processes.stopping = self.panels.processes.stopping.saturating_sub(1);
        self.panels.processes.refresh.again();
        let failed = stopped.iter().any(|(_, outcome)| !matches!(outcome, Ok(Outcome::Stopped | Outcome::Killed)));
        let icon = if failed { ui::ToastIcon::Bug } else { ui::ToastIcon::Check };
        self.messages.toast = Some(Toast::new(stop::summary(stopped), icon));
    }

    pub(super) fn processes_key(&mut self, key: KeyEvent, area: Rect) {
        let (rows, _) = ui::processes_room(area, area.width < ui::COMPACT_WIDTH);
        let Some(Overlay::Processes(dialog)) = &mut self.overlay else { return };
        let action = dialog.key(key, rows);
        self.processes_act(action);
    }

    pub(super) fn processes_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(pointer) = list_pointer(ev.kind) else { return };
        let Some(Overlay::Processes(dialog)) = &self.overlay else { return };
        let view = self.processes_view(dialog, area);
        if let ListPointer::Wheel(delta) = pointer {
            let lines = view.confirm.as_ref().map(|confirm| confirm.lines.len());
            let (rows, lines) = wheel_span(ui::processes_room(area, view.compact), lines);
            if let Some(Overlay::Processes(dialog)) = &mut self.overlay {
                dialog.scroll_by(delta, rows, lines);
            }
            return;
        }
        let Some(hit) = ui::processes_hit(area, &view, pos) else { return };
        let Some(Overlay::Processes(dialog)) = &mut self.overlay else { return };
        let action = dialog.click(hit_of(hit));
        self.processes_act(action);
    }

    pub(super) fn processes_act(&mut self, action: Action) {
        if let Some(Overlay::Processes(dialog)) = &self.overlay {
            self.panels.processes.sort = dialog.sort;
        }
        match action {
            Action::None => {}
            Action::Close => self.overlay = None,
            Action::Go(pane) => {
                self.overlay = None;
                let _ = self.show(Item::Pane(pane));
            }
            Action::ClosePane(pane) => self.close_from_processes(pane),
            Action::Stop(group) => self.stop_processes(group),
        }
    }

    fn close_from_processes(&mut self, pane: u64) {
        self.overlay = None;
        if self.agent_close_plan(pane).is_ok() {
            self.ask_close_agent(pane);
        } else {
            self.ask_close_pane_of(pane);
        }
    }

    fn stop_processes(&mut self, group: stop::Group) {
        self.panels.processes.stopping += 1;
        let job = Job::new(Level::Info, "processes", "stop").with("programs", group.size());
        self.spawn_job(
            job,
            move || Ok(stop::stop(group)),
            move |result, _| Some(AppEvent::Processes(ProcessesEvent::Stopped(result.unwrap_or_default()))),
        );
    }

    fn process_place(&self, pane: u64) -> String {
        let Some(at) = self.projects.tab_of_pane(pane) else { return format!("pane {pane}") };
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at.workspace()));
        let tab = self.projects.tab(at);
        let names = [project.label(self.home.as_deref()), workspace.label(), tab.label(&self.config)];
        let place = names.join(CRUMB_SEPARATOR);
        if tab.panes.len() > 1 { format!("{place} · pane {pane}") } else { place }
    }

    fn process_rows(&self, sample: &Sample) -> (Vec<PaneRow>, Option<PaneRow>) {
        let known = |tree: &&PaneTree| self.projects.tab_of_pane(tree.pane.id).is_some();
        let rows = sample.panes.iter().filter(known).enumerate().map(|(order, tree)| self.pane_row(order, tree));
        let own = (!sample.cornercase.is_empty()).then(|| own_row(&sample.cornercase));
        (rows.collect(), own)
    }

    fn pane_row(&self, order: usize, tree: &PaneTree) -> PaneRow {
        let program = self.projects.pane_by(tree.pane.id).and_then(|term| term.program(&self.config));
        let tags = agent_tags(&tree.procs, tree.pane.agent.as_ref());
        PaneRow {
            pane: Some(tree.pane.id),
            order,
            place: self.process_place(tree.pane.id),
            program: program.unwrap_or_default(),
            cpu: sum_cpu(&tree.procs),
            memory: sum_memory(&tree.procs),
            procs: tree.procs.iter().zip(tags).map(|(proc, tag)| proc_row(proc, tag, true)).collect(),
        }
    }

    pub(super) fn processes_view(&self, dialog: &Dialog, area: Rect) -> Box<ui::Processes> {
        let compact = area.width < ui::COMPACT_WIDTH;
        let (_, body) = ui::processes_room(area, compact);
        let visible = dialog.visible();
        let confirm = match &dialog.step {
            Step::Confirm(target) => {
                Some(ui::ConfirmBody { lines: wrap(stop_lines(target), body.width), scroll: dialog.body })
            }
            Step::List => None,
        };
        let note = self.processes_note(dialog, confirm.as_ref(), body);
        Box::new(ui::Processes {
            filter: dialog.filter.clone(),
            sort: Sort::ALL.iter().position(|sort| *sort == dialog.sort).unwrap_or(0),
            totals: totals_text(dialog),
            lines: visible.iter().filter_map(|line| process_line(dialog, *line)).collect(),
            selected: dialog.selected_line(&visible),
            scroll: dialog.scroll,
            empty: empty_text(dialog, visible.is_empty()),
            note,
            confirm,
            compact,
        })
    }

    fn processes_note(&self, dialog: &Dialog, confirm: Option<&ui::ConfirmBody>, body: Rect) -> Option<ui::Note> {
        if self.panels.processes.stopping > 0 {
            return Some(ui::Note::Busy("stopping…"));
        }
        if let Some(error) = &dialog.failed {
            return Some(ui::Note::Error(format!("could not read the processes: {error}")));
        }
        if let Some(confirm) = confirm {
            return (confirm.scroll + usize::from(body.height) < confirm.lines.len())
                .then(|| ui::Note::Info("↓ scroll for more".into()));
        }
        (!dialog.loaded).then_some(ui::Note::Busy("measuring…"))
    }
}

#[cfg(test)]
impl App {
    pub(super) fn processes_sampling(&self) -> bool {
        self.panels.processes.refresh.busy()
    }
}

fn own_row(procs: &[Proc]) -> PaneRow {
    PaneRow {
        pane: None,
        order: usize::MAX,
        place: "cornercase".into(),
        program: "server".into(),
        cpu: sum_cpu(procs),
        memory: sum_memory(procs),
        procs: procs.iter().map(|proc| proc_row(proc, None, false)).collect(),
    }
}

fn proc_row(proc: &Proc, agent: Option<AgentTag>, pane: bool) -> ProcRow {
    ProcRow {
        pid: proc.pid,
        started: proc.started,
        depth: proc.depth,
        name: proc.name.clone(),
        command: command_line(Some(&proc.name), &proc.args, COMMAND_WIDTH),
        cpu: proc.cpu.unwrap_or(0.0),
        memory: proc.memory.unwrap_or(0),
        stoppable: pane && proc.depth > 0,
        agent,
    }
}

fn agent_tags(procs: &[Proc], agent: Option<&(String, i32)>) -> Vec<Option<AgentTag>> {
    let Some((name, root)) = agent else { return vec![None; procs.len()] };
    let mut under: Vec<i32> = Vec::new();
    procs
        .iter()
        .map(|proc| {
            let inside = proc.pid == *root || proc.parent.is_some_and(|parent| under.contains(&parent));
            if inside {
                under.push(proc.pid);
            }
            inside.then(|| AgentTag { name: name.clone(), itself: proc.pid == *root })
        })
        .collect()
}

fn totals_text(dialog: &Dialog) -> String {
    let totals = dialog.totals();
    format!(
        "{} · {} · cpu {:.1}% · {}",
        crate::text::plural(totals.panes, "pane", "panes"),
        crate::text::plural(totals.processes, "process", "processes"),
        totals.cpu,
        memory_size(totals.memory)
    )
}

fn empty_text(dialog: &Dialog, nothing: bool) -> Option<&'static str> {
    match (dialog.loaded && nothing, dialog.filter.trim().is_empty()) {
        (false, _) => None,
        (true, true) => Some("No pane is open."),
        (true, false) => Some("no matches"),
    }
}

fn process_line(dialog: &Dialog, line: At) -> Option<ui::ProcessLine> {
    match line {
        At::Pane(at) => {
            let row = dialog.row(at)?;
            Some(ui::ProcessLine {
                depth: None,
                open: dialog.is_open(row),
                text: row.place.clone(),
                program: row.program.clone(),
                cpu: row.cpu,
                memory: row.memory,
                pane: row.pane.is_some(),
                stop: false,
            })
        }
        At::Proc(at, index) => {
            let proc = dialog.row(at)?.procs.get(index)?;
            Some(ui::ProcessLine {
                depth: Some(proc.depth),
                open: false,
                text: proc.command.clone(),
                program: format!("pid {}", proc.pid),
                cpu: proc.cpu,
                memory: proc.memory,
                pane: false,
                stop: proc.stoppable,
            })
        }
    }
}

fn stop_lines(target: &Target) -> Vec<Line<'static>> {
    let warn = Style::default().fg(Color::Yellow);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut lines = vec![Line::styled("Stop this program and the programs it started?", bold)];
    lines.push(Line::raw(format!("• {} · pid {} · {}", target.command, target.group.root.pid, target.place)));
    if !target.also.is_empty() {
        lines.push(Line::raw(format!("  also stops: {}", target.also.join(", "))));
    }
    match &target.agent {
        Some(tag) if tag.itself => lines.push(Line::from(Span::styled(
            format!("  ⚠ it is the agent {}: its conversation ends; close agent closes its tab instead", tag.name),
            warn,
        ))),
        Some(tag) => lines.push(Line::from(Span::styled(
            format!("  ⚠ the agent {} started it: the agent may start it again, or fail without it", tag.name),
            warn,
        ))),
        None => {}
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw(
        "Each gets SIGTERM, then SIGKILL if it is still running after 3 s. The pane's shell keeps running.",
    ));
    lines
}

fn hit_of(hit: ui::ProcessesHit) -> Hit {
    match hit {
        ui::ProcessesHit::Filter => Hit::Filter,
        ui::ProcessesHit::Sort(at) => Hit::Sort(Sort::ALL[at.min(Sort::ALL.len() - 1)]),
        ui::ProcessesHit::Row(row) => Hit::Row(row),
        ui::ProcessesHit::Go(row) => Hit::Go(row),
        ui::ProcessesHit::Close(row) => Hit::Close(row),
        ui::ProcessesHit::StopRow(row) => Hit::StopRow(row),
        ui::ProcessesHit::Stop => Hit::Stop,
        ui::ProcessesHit::Done => Hit::Done,
        ui::ProcessesHit::Back => Hit::Back,
    }
}
