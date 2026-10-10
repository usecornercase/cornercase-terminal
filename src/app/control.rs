use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use super::{App, AppEvent, Target, Toast};
use crate::activity::{self, Status};
use crate::agents;
use crate::context;
use crate::control::{
    self, Command, Done, GroupInfo, Ids, Item, PaneInfo, ProjectInfo, Report, Request, Response, TabInfo, TodoItem,
    TodoList, Until, WorkspaceInfo,
};
use crate::error;
use crate::git;
use crate::keys;
use crate::launch::{self, Launch};
use crate::log::{self, Job, Level};
use crate::notify::{self, Notification};
use crate::panics;
use crate::project::{Phase, Project, Tab, Workspace};
use crate::search::Goto;
use crate::split::Dir;
use crate::term::{self, Term};
use crate::ui;
use crate::update;
use crate::worktree;

const STARTS_WITHIN: Duration = Duration::from_secs(10);
pub(super) const CONFIRM_WITHIN: Duration = Duration::from_secs(10);
const SHELL_SETTLES: Duration = Duration::from_millis(300);
const SHELL_UNSEEN: Duration = Duration::from_secs(1);
const INPUT_HOLDS: Duration = Duration::from_secs(2);
const TEXT_EVERY: Duration = Duration::from_millis(100);
const SOONEST: Duration = Duration::from_millis(10);
const WATCHED: [&str; 4] = [agents::CLAUDE, agents::CODEX, agents::GEMINI, agents::OPENCODE];
const NO_SIZE: &str = "this cornercase server has not opened a window yet, so a new terminal would have no size; \
    run `cornercase` once first";
const NO_PROJECT: &str = "no project is open; open one with `cornercase open PATH`";
const NO_PANE: &str = "no pane is open here; pass --pane or --tab (`cornercase status` lists them)";
const NO_AGENT: &str = "say which agent to start, such as `cornercase start claude`: settings → agents is on auto \
    and no agent runs here";
const RESTART_CANCELLED: &str = "the restart was cancelled in the window; the server keeps running";

type Reply = Result<Value, String>;
type Handled = Result<Option<Value>, String>;

#[derive(Default)]
pub(super) struct Requests {
    pending: Vec<Pending>,
    open: Vec<u64>,
    asked: HashMap<u64, (Instant, &'static str)>,
    answers: Vec<(u64, String)>,
    launched: Vec<(u64, bool)>,
    next: u64,
}

impl Requests {
    pub(super) fn launched(&mut self, key: u64, started: bool) {
        self.launched.push((key, started));
    }

    fn key(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn take(&mut self, key: u64) -> Option<Pending> {
        let found = self.pending.iter().position(|p| p.key == key)?;
        Some(self.pending.remove(found))
    }
}

struct Pending {
    client: Option<u64>,
    key: u64,
    timeout: Option<(Instant, f64)>,
    done: Done,
    stage: Stage,
}

enum Stage {
    Worktree(Then),
    Removing { force: bool },
    Launch { confirm: Option<SystemTime>, wait: Option<Condition> },
    Confirm(Confirm),
    Watch(Watch),
    Several { all: bool, parts: Vec<Pending> },
    Reading,
    Restart { caller: Option<u64> },
}

impl Stage {
    fn launch(wait: Option<Condition>) -> Self {
        Self::Launch { confirm: None, wait }
    }
}

struct Confirm {
    pane: u64,
    since: SystemTime,
    by: Instant,
    wait: Option<Condition>,
}

struct Then {
    start: Option<(launch::Spec, Option<String>)>,
    wait: Option<Condition>,
    focus: bool,
}

struct Watch {
    pane: u64,
    until: Condition,
    since: Option<Instant>,
    checked: Option<Instant>,
    left: Option<Instant>,
}

impl Watch {
    fn settles(&self, term: &Term) -> Duration {
        let ran = self.left.is_some_and(|left| term.input_at.is_none_or(|input| left > input));
        if ran { SHELL_SETTLES } else { SHELL_UNSEEN }
    }

    fn next_look(&self, term: &Term) -> Option<Instant> {
        let at = self.checked.filter(|checked| term.output_at >= *checked)?;
        at.checked_add(TEXT_EVERY)
    }
}

enum Condition {
    Stops,
    Idle,
    Working,
    Waiting,
    Shell,
    TurnOver,
    Text(Regex),
    Quiet(Duration),
}

impl Condition {
    fn of(until: Until) -> Result<Self, String> {
        Ok(match until {
            Until::Stops => Self::Stops,
            Until::Idle => Self::Idle,
            Until::Working => Self::Working,
            Until::Waiting => Self::Waiting,
            Until::Shell => Self::Shell,
            Until::TurnOver => Self::TurnOver,
            Until::Text(pattern) => Self::Text(
                Regex::new(&pattern).map_err(|e| format!("`{pattern}` is not a valid regular expression: {e}"))?,
            ),
            Until::Quiet(seconds) => Self::Quiet(duration(seconds, "--quiet")?),
        })
    }

    fn needs_agent(&self) -> bool {
        matches!(self, Self::Stops | Self::Idle | Self::Working | Self::Waiting | Self::TurnOver)
    }

    fn ends_on_idle(&self) -> bool {
        matches!(self, Self::Stops | Self::Idle | Self::TurnOver)
    }

    fn ended(&self, agent: &activity::Pane) -> Option<&'static str> {
        let status = agent.status()?;
        let holds = match self {
            Self::TurnOver if agent.background_shell() => return Some(activity::SHELL),
            Self::Stops | Self::TurnOver => status != Status::Working,
            Self::Idle => matches!(status, Status::Idle | Status::Done),
            Self::Working => status == Status::Working,
            Self::Waiting => status == Status::Waiting,
            Self::Shell | Self::Text(_) | Self::Quiet(_) => false,
        };
        holds.then(|| status.name())
    }
}

enum Verdict {
    Pending,
    Ended(&'static str, Option<String>),
    Failed(String),
}

#[derive(Default)]
struct Here {
    project: Option<usize>,
    workspace: Option<usize>,
    tab: Option<usize>,
    pane: Option<u64>,
}

fn json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn sized(area: Option<Rect>) -> Result<Rect, String> {
    area.ok_or_else(|| NO_SIZE.to_string())
}

fn none(kind: &str, id: u64) -> String {
    format!("there is no {kind} {id}; `cornercase status` lists them")
}

fn shell_quote(path: &str) -> String {
    if !path.is_empty() && path.chars().all(|c| c.is_ascii_alphanumeric() || "/._~+-".contains(c)) {
        return path.to_string();
    }
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn duration(seconds: f64, what: &str) -> Result<Duration, String> {
    Duration::try_from_secs_f64(seconds)
        .ok()
        .filter(|d| !d.is_zero())
        .ok_or_else(|| format!("{what} takes a positive number of seconds"))
}

fn deadline(timeout: Option<f64>, now: Instant) -> Result<Option<(Instant, f64)>, String> {
    let at = |seconds| {
        let at = now.checked_add(duration(seconds, "--timeout")?);
        at.map(|at| (at, seconds)).ok_or_else(|| "--timeout is too long".to_string())
    };
    timeout.map(at).transpose()
}

fn name_of(name: Option<String>) -> Option<String> {
    let name: String = name?.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn not_confirmed(pane: u64) -> String {
    format!(
        "not confirmed: the prompt may not have been submitted: the agent in pane {pane} recorded no new prompt \
         after the Enter; `cornercase read --pane {pane}` shows where it is, read it before sending again"
    )
}

fn holds_prompts(agent: Option<&str>, status: Option<Status>) -> bool {
    matches!(agent, Some(agents::CODEX | agents::GEMINI)) && matches!(status, Some(Status::Working | Status::Waiting))
}

fn not_reading(pane: u64) -> String {
    format!(
        "the program in pane {pane} is not reading its input: {} MiB are waiting for it, so nothing more was sent",
        term::MAX_QUEUED / (1024 * 1024)
    )
}

fn endings(value: &Value) -> String {
    let ended = |value: &Value| value.get("ended").and_then(Value::as_str).unwrap_or("-").to_string();
    let Some(panes) = value.get("panes").and_then(Value::as_array) else { return ended(value) };
    let id = |pane: &Value| pane.get("tab").or_else(|| pane.get("pane")).and_then(Value::as_u64).unwrap_or_default();
    panes.iter().map(|pane| format!("{}:{}", id(pane), ended(pane))).collect::<Vec<_>>().join(",")
}

fn unfound(pane: u64, agent: &str) -> String {
    let why = match agent {
        agents::CLAUDE => "Claude Code has not said yet which conversation it is in",
        agents::CODEX => "Codex writes its rollout from its first turn on, and two Codex in one folder hide each other",
        agents::GEMINI => "Gemini has no conversation in its folder yet, or two Gemini share that home and folder",
        _ => "opencode has no conversation in its folder yet, or two opencode share that folder",
    };
    format!(
        "cornercase has not found where the {agent} agent in pane {pane} keeps its conversation: {why}; \
         `cornercase read --pane {pane}` prints its screen"
    )
}

fn turn_over(agent: &activity::Pane) -> Option<bool> {
    agent.status().map(|_| Condition::TurnOver.ended(agent).is_some())
}

fn held(panes: &[u64]) -> String {
    let ids: Vec<String> = panes.iter().map(u64::to_string).collect();
    match ids.as_slice() {
        [] => "the server keeps running".into(),
        [one] => format!("the agent in pane {one} has not ended its turn, so the server keeps running"),
        [first @ .., last] => format!(
            "the agents in panes {} and {last} have not ended their turn, so the server keeps running",
            first.join(", ")
        ),
    }
}

fn pending_restart(agents: usize) -> String {
    match agents {
        0 => "restart pending".into(),
        1 => "restart pending until 1 agent ends its turn".into(),
        n => format!("restart pending until {n} agents end their turn"),
    }
}

fn pane_ids(pane: u64) -> Done {
    Done { ids: Ids { pane: Some(pane), ..Ids::default() }, ..Done::default() }
}

fn parse(text: &str) -> Result<Request, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("the request is not JSON: {e}"))?;
    let name = control::command_name(&value).unwrap_or_default().to_string();
    let server = update::CURRENT;
    if !Command::NAMES.contains(&name.as_str()) {
        return Err(control::unknown_command(&name));
    }
    serde_json::from_value(value)
        .map_err(|e| format!("the running cornercase server ({server}) cannot read this `{name}` request: {e}"))
}

impl App {
    pub fn request(&mut self, client: u64, text: &str, area: Option<Rect>, now: Instant) {
        self.requests.open.push(client);
        let request = parse(text);
        match &request {
            Ok(request) => {
                let command = request.command.name();
                self.requests.asked.insert(client, (now, command));
                let mut fields = vec![("client", client.to_string())];
                fields.extend(request.caller.map(|caller| ("caller", caller.to_string())));
                fields.extend(request.command.fields());
                log::write_fields(Level::Info, "control", command, &fields);
            }
            Err(_) => log::warning!("control", "unreadable request", client = client, bytes = text.len()),
        }
        match request.and_then(|request| self.handle_request(client, request, area, now)) {
            Ok(Some(value)) => self.answer(Some(client), Ok(value)),
            Ok(None) => {}
            Err(message) => self.answer(Some(client), Err(message)),
        }
    }

    pub fn take_answers(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.requests.answers)
    }

    pub fn answer_lost_requests(&mut self) {
        let Requests { pending, open, .. } = &self.requests;
        let lost: Vec<u64> =
            open.iter().copied().filter(|client| pending.iter().all(|p| p.client != Some(*client))).collect();
        for client in lost {
            self.answer(Some(client), Err(error::Error::Bug.to_string()));
        }
    }

    pub fn forget(&mut self, client: u64) {
        self.requests.open.retain(|open| *open != client);
        if let Some((at, command)) = self.requests.asked.remove(&client) {
            log::info!(
                "control",
                "client left before the answer",
                client = client,
                command = command,
                ms = at.elapsed().as_millis()
            );
        }
        self.events.forget(client);
        for pending in self.requests.pending.iter_mut().filter(|p| p.client == Some(client)) {
            pending.client = None;
        }
        self.requests.pending.retain(|p| {
            p.client.is_some()
                || !matches!(
                    p.stage,
                    Stage::Confirm(_)
                        | Stage::Watch(_)
                        | Stage::Several { .. }
                        | Stage::Reading
                        | Stage::Restart { .. }
                )
        });
    }

    fn answer(&mut self, client: Option<u64>, reply: Reply) {
        let Some(client) = client else { return };
        self.requests.open.retain(|open| *open != client);
        if let Some((at, command)) = self.requests.asked.remove(&client) {
            let ms = at.elapsed().as_millis();
            match &reply {
                Ok(value) => {
                    let ended = endings(value);
                    log::info!("control", "answered", client = client, command = command, ms = ms, ended = ended);
                }
                Err(message) => {
                    log::info!("control", "refused", client = client, command = command, ms = ms, error = message);
                }
            }
        }
        let response = match reply {
            Ok(value) => Response::Ok(value),
            Err(message) => Response::Error(message),
        };
        if let Ok(text) = serde_json::to_string(&response) {
            self.requests.answers.push((client, text));
        }
    }

    fn handle_request(&mut self, client: u64, request: Request, area: Option<Rect>, now: Instant) -> Handled {
        let caller = request.caller.filter(|_| request.server.as_deref() == Some(control::server_token()));
        match request.command {
            Command::Status(_) => Ok(Some(json(&self.report(caller)))),
            Command::Open(open) => self.open_request(&open, area),
            Command::NewWorkspace(new) => self.new_workspace_request(client, caller, new, area),
            Command::NewTab(new) => self.new_tab_request(client, caller, new, area, now),
            Command::Split(split) => self.split_request(client, caller, split, area, now),
            Command::Start(start) => self.start_request(client, caller, start, area, now),
            Command::Send(send) => self.send_request(client, caller, send, now),
            Command::Keys(keys) => self.keys_request(caller, &keys, now),
            Command::Read(read) => self.read_request(caller, &read),
            Command::Wait(wait) => self.wait_request(client, caller, wait, now),
            Command::WaitSeveral(wait) => self.wait_several_request(client, caller, wait, now),
            Command::Close(close) => self.close_request(client, &close),
            Command::Rename(rename) => self.rename_request(caller, rename),
            Command::Focus(focus) => self.show(focus.item).map(|()| Some(json(&Done::default()))),
            Command::Notify(notify) => self.notify_request(&notify.text),
            Command::Todo(todo) => self.todo_request(todo),
            Command::Events(events) => self.events_request(client, events),
            Command::LastMessage(last) => self.last_message_request(client, caller, &last),
            Command::RestartWhenIdle(restart) => self.restart_request(client, caller, &restart, now),
        }
    }

    fn launch_within(
        &mut self,
        client: u64,
        done: Done,
        launch: Option<Launch>,
        stage: Stage,
        timeout: Option<(Instant, f64)>,
    ) -> Option<Value> {
        let Some(mut launch) = launch else { return Some(json(&done)) };
        let key = self.requests.key();
        launch.key = Some(key);
        self.launches.push(launch);
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done, stage });
        None
    }

    pub(super) fn check_requests(&mut self, now: Instant) {
        let launched = std::mem::take(&mut self.requests.launched);
        let mut kept = Vec::new();
        for mut pending in std::mem::take(&mut self.requests.pending) {
            match self.advance(&mut pending, &launched, now) {
                None => kept.push(pending),
                Some(reply) => {
                    if reply.is_ok() && matches!(pending.stage, Stage::Restart { .. }) {
                        self.restart = true;
                    }
                    self.answer(pending.client, reply);
                    if matches!(pending.stage, Stage::Worktree(_)) {
                        kept.push(Pending { client: None, timeout: None, ..pending });
                    }
                }
            }
        }
        kept.append(&mut self.requests.pending);
        self.requests.pending = kept;
        self.show_pending_restart(now);
    }

    fn advance(&self, pending: &mut Pending, launched: &[(u64, bool)], now: Instant) -> Option<Reply> {
        let pane = pending.done.ids.pane.unwrap_or_default();
        if let Stage::Launch { confirm, wait } = &mut pending.stage
            && let Some(&(_, started)) = launched.iter().find(|(key, _)| *key == pending.key)
        {
            if !started {
                return Some(Err(self.not_started(pane)));
            }
            let (confirm, wait) = (confirm.take(), wait.take());
            let next = match confirm {
                Some(since) => Some(Stage::Confirm(Confirm { pane, since, by: now + self.confirm_within, wait })),
                None => wait.map(|until| Stage::Watch(self.watch(pane, until, now))),
            };
            match next {
                Some(next) if pending.client.is_some() => pending.stage = next,
                _ => return Some(Ok(json(&pending.done))),
            }
        }
        if let Stage::Confirm(confirm) = &mut pending.stage {
            match self.confirmation(confirm, now) {
                None => {}
                Some(Err(message)) => return Some(Err(message)),
                Some(Ok(())) => {
                    let Some(until) = confirm.wait.take() else { return Some(Ok(json(&pending.done))) };
                    pending.stage = Stage::Watch(self.watch(pane, until, now));
                }
            }
        }
        if let Stage::Watch(watch) = &mut pending.stage {
            match self.verdict(watch, now) {
                Verdict::Pending => {}
                Verdict::Ended(ended, line) => {
                    pending.done.ended = Some(ended.to_string());
                    pending.done.line = line;
                    return Some(Ok(json(&pending.done)));
                }
                Verdict::Failed(message) => return Some(Err(message)),
            }
        }
        if let Stage::Several { all, parts } = &mut pending.stage {
            for part in parts.iter_mut().filter(|part| part.done.ended.is_none()) {
                if self.pane_by(part.done.ids.pane.unwrap_or_default()).is_none() {
                    part.done.ended = Some("closed".into());
                } else if let Some(Err(message)) = self.advance(part, launched, now) {
                    return Some(Err(message));
                }
            }
            let mut ended = parts.iter().filter(|part| part.done.ended.is_some()).map(|part| part.done.clone());
            let panes: Vec<Done> = if *all {
                if parts.iter().all(|part| part.done.ended.is_some()) { ended.collect() } else { Vec::new() }
            } else {
                ended.next().into_iter().collect()
            };
            if !panes.is_empty() {
                return Some(Ok(json(&Done { panes, ..Done::default() })));
            }
        }
        if let Stage::Restart { caller } = pending.stage
            && self.watched == Some(now)
            && self.holding(caller, now).is_empty()
        {
            return Some(Ok(json(&self.report(caller))));
        }
        let (at, seconds) = pending.timeout.filter(|_| pending.client.is_some())?;
        (now >= at).then(|| Err(format!("timed out after {seconds}s: {}", self.waiting_on(&pending.stage, pane))))
    }

    fn not_started(&self, pane: u64) -> String {
        if self.pane_by(pane).is_none() {
            format!("pane {pane} closed before it was ready")
        } else {
            format!(
                "pane {pane} did not take what was typed: the agent did not start, or its program is not reading; \
                 `cornercase read --pane {pane}` shows where it is"
            )
        }
    }

    fn waiting_on(&self, stage: &Stage, pane: u64) -> String {
        let watch = match stage {
            Stage::Worktree(_) => return "git is still creating the worktree".into(),
            Stage::Removing { .. } => return "git is still removing the worktree".into(),
            Stage::Launch { .. } => return format!("pane {pane} is still starting"),
            Stage::Confirm(confirm) => return self.unrecorded(confirm.pane),
            Stage::Reading => return format!("the record of pane {pane} is still being read"),
            Stage::Restart { caller } => return held(&self.holding(*caller, Instant::now())),
            Stage::Several { parts, .. } => {
                let pending = parts.iter().filter(|part| part.done.ended.is_none());
                let waits = pending.map(|part| self.waiting_on(&part.stage, part.done.ids.pane.unwrap_or_default()));
                return waits.collect::<Vec<_>>().join("; ");
            }
            Stage::Watch(watch) => watch,
        };
        let pane = watch.pane;
        let Some(term) = self.pane_by(pane) else { return format!("pane {pane} closed") };
        match &watch.until {
            Condition::Text(re) => format!("no line on the screen of pane {pane} matches `{re}`"),
            Condition::Quiet(_) => format!("pane {pane} kept writing"),
            Condition::Shell => {
                format!("pane {pane} still runs {}", term.program(&self.config).unwrap_or_else(|| "a program".into()))
            }
            Condition::Stops if term.agent.background_shell() => format!(
                "the agent in pane {pane} ended its turn, but a background shell it started still runs; \
                 --until turn-over ends there"
            ),
            _ => format!("the agent in pane {pane} is {}", term.agent.status().map_or("starting", Status::name)),
        }
    }

    fn holding(&self, caller: Option<u64>, now: Instant) -> Vec<u64> {
        let launching: Vec<u64> = self.launches.iter().filter(|l| !l.waits_for_you()).map(|l| l.term).collect();
        let terms = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms);
        let holds = |term: &&Term| Some(term.id) != caller && (launching.contains(&term.id) || self.at_work(term, now));
        terms.filter(holds).map(|term| term.id).collect()
    }

    fn at_work(&self, term: &Term, now: Instant) -> bool {
        if term.agent.status().is_some() && Condition::TurnOver.ended(&term.agent).is_none() {
            return true;
        }
        let within = |at: Instant, limit: Duration| now.saturating_duration_since(at) < limit;
        let typed = term.input_at.is_some_and(|at| within(at, INPUT_HOLDS));
        let unanswered = term.submitted.is_some_and(|at| within(at, STARTS_WITHIN) && !term.agent.reacted(at));
        (typed || unanswered) && self.runs_agent(term)
    }

    pub(super) fn show_pending_restart(&mut self, now: Instant) {
        let shown = self.toast.as_ref().is_some_and(|t| t.icon == ui::ToastIcon::Restart);
        let pending = self.requests.pending.iter().find_map(|p| match p.stage {
            Stage::Restart { caller } => Some(caller),
            _ => None,
        });
        match pending {
            None if shown => self.toast = None,
            Some(caller) if shown || self.toast.is_none() => {
                let message = pending_restart(self.holding(caller, now).len());
                self.toast = Some(Toast::new(message, ui::ToastIcon::Restart));
            }
            _ => {}
        }
    }

    pub(super) fn cancel_restarts(&mut self) {
        let (cancelled, kept): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.requests.pending)
            .into_iter()
            .partition(|p| matches!(p.stage, Stage::Restart { .. }));
        self.requests.pending = kept;
        log::info!("control", "restart cancelled in the window", waits = cancelled.len());
        for pending in cancelled {
            self.answer(pending.client, Err(RESTART_CANCELLED.into()));
        }
    }

    fn unrecorded(&self, pane: u64) -> String {
        let holding = self.pane_by(pane).is_some_and(|term| holds_prompts(term.agent.agent(), term.agent.status()));
        let why = if holding { ": this agent queues a prompt sent while it works until its next step" } else { "" };
        format!("the agent in pane {pane} has not recorded the prompt yet{why}")
    }

    fn confirmation(&self, confirm: &mut Confirm, now: Instant) -> Option<Result<(), String>> {
        let pane = confirm.pane;
        let Some(term) = self.pane_by(pane) else { return Some(Err(format!("pane {pane} closed"))) };
        if term.context.prompted().is_some_and(|at| at >= confirm.since) {
            return Some(Ok(()));
        }
        if holds_prompts(term.agent.agent(), term.agent.status()) {
            confirm.by = confirm.by.max(now + self.confirm_within);
        }
        (now >= confirm.by).then(|| Err(not_confirmed(pane)))
    }

    fn verdict(&self, watch: &mut Watch, now: Instant) -> Verdict {
        let pane = watch.pane;
        let Some(term) = self.pane_by(pane) else { return Verdict::Failed(format!("pane {pane} closed")) };
        let quiet = now.saturating_duration_since(term.output_at);
        if !term.shell_in_foreground() {
            watch.left = Some(now);
        }
        let look = watch.checked.is_none() || watch.next_look(term).is_some_and(|due| now >= due);
        match &watch.until {
            Condition::Shell
                if term.shell_in_foreground()
                    && term.input_at.is_none_or(|at| term.output_at > at)
                    && quiet >= watch.settles(term) =>
            {
                return Verdict::Ended("shell", None);
            }
            Condition::Quiet(enough) if quiet >= *enough => return Verdict::Ended("quiet", None),
            Condition::Text(re) if look => {
                watch.checked = Some(now);
                let screen = term.emulator.screen_text().unwrap_or_default();
                if let Some(line) = screen.lines().find(|line| re.is_match(line)) {
                    return Verdict::Ended("text", Some(line.to_string()));
                }
                return Verdict::Pending;
            }
            Condition::Shell | Condition::Quiet(_) | Condition::Text(_) => return Verdict::Pending,
            Condition::Stops | Condition::Idle | Condition::Working | Condition::Waiting | Condition::TurnOver => {}
        }
        if term.agent.status().is_none() {
            if self.watched_agent(term).is_ok() {
                return Verdict::Pending;
            }
            return Verdict::Failed(format!("the agent in pane {pane} exited"));
        }
        if let Some(since) = watch.since {
            if term.agent.reacted(since) {
                watch.since = None;
            } else if now >= since + STARTS_WITHIN {
                return Verdict::Failed(format!(
                    "the agent in pane {pane} did not start working within {}s of the Enter: it may have finished \
                     at once, or not got the text; `cornercase read --pane {pane}` shows where it is",
                    STARTS_WITHIN.as_secs()
                ));
            } else {
                return Verdict::Pending;
            }
        }
        watch.until.ended(&term.agent).map_or(Verdict::Pending, |ended| Verdict::Ended(ended, None))
    }

    pub(super) fn next_request(&self, now: Instant) -> Option<Duration> {
        let due = self.requests.pending.iter().flat_map(|pending| {
            let deadline = pending.timeout.filter(|_| pending.client.is_some()).map(|(at, _)| at);
            let watches: Vec<&Watch> = match &pending.stage {
                Stage::Watch(watch) => vec![watch],
                Stage::Several { parts, .. } => parts
                    .iter()
                    .filter(|part| part.done.ended.is_none())
                    .filter_map(|part| match &part.stage {
                        Stage::Watch(watch) => Some(watch),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let confirm = match &pending.stage {
                Stage::Confirm(confirm) => Some(confirm.by),
                _ => None,
            };
            deadline.into_iter().chain(confirm).chain(watches.into_iter().flat_map(|watch| self.due(watch)))
        });
        due.filter(|at| *at > now).min().map(|at| at.saturating_duration_since(now).max(SOONEST))
    }

    fn due(&self, watch: &Watch) -> impl Iterator<Item = Instant> {
        let quiet = self.pane_by(watch.pane).and_then(|term| match watch.until {
            Condition::Quiet(quiet) => term.output_at.checked_add(quiet),
            Condition::Shell => term.output_at.checked_add(watch.settles(term)),
            Condition::Text(_) => watch.next_look(term),
            _ => None,
        });
        [quiet, watch.since.map(|since| since + STARTS_WITHIN)].into_iter().flatten()
    }

    fn pane_by(&self, id: u64) -> Option<&Term> {
        self.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms).find(|t| t.id == id)
    }

    fn pane_by_mut(&mut self, id: u64) -> Option<&mut Term> {
        self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id)
    }

    fn watch(&self, pane: u64, until: Condition, now: Instant) -> Watch {
        let submitted = self.pane_by(pane).and_then(|term| term.submitted);
        let recent = submitted.filter(|at| now.saturating_duration_since(*at) < STARTS_WITHIN);
        Watch { pane, since: recent.filter(|_| until.ends_on_idle()), until, checked: None, left: None }
    }

    fn runs_agent(&self, term: &Term) -> bool {
        term.agent.status().is_some() || agents::detect(&self.config, &term.foreground_args()).is_some()
    }

    fn watched_agent(&self, term: &Term) -> Result<(), Option<String>> {
        if term.agent.status().is_some() {
            return Ok(());
        }
        match agents::detect(&self.config, &term.foreground_args()) {
            Some(agent) if WATCHED.contains(&agent.as_str()) => Ok(()),
            other => Err(other),
        }
    }

    fn locate(&self, pane: u64) -> Option<(usize, usize, usize)> {
        self.find_tab(|tab| tab.panes.iter().any(|t| t.id == pane))
    }

    fn locate_tab(&self, id: u64) -> Option<(usize, usize, usize)> {
        self.find_tab(|tab| tab.id == id)
    }

    fn find_tab(&self, found: impl Fn(&Tab) -> bool) -> Option<(usize, usize, usize)> {
        self.projects.iter().enumerate().find_map(|(p, project)| {
            project
                .workspaces
                .iter()
                .enumerate()
                .find_map(|(w, workspace)| workspace.tabs.iter().position(&found).map(|t| (p, w, t)))
        })
    }

    fn here(&self, caller: Option<u64>) -> Here {
        if let Some(pane) = caller
            && let Some((p, w, t)) = self.locate(pane)
        {
            return Here { project: Some(p), workspace: Some(w), tab: Some(t), pane: Some(pane) };
        }
        let Some(project) = self.project() else { return Here::default() };
        let workspace = project.workspace();
        let tab = workspace.and_then(Workspace::tab);
        Here {
            project: Some(self.active),
            workspace: workspace.map(|_| project.active),
            tab: workspace.filter(|_| tab.is_some()).map(|w| w.active),
            pane: tab.and_then(Tab::pane).map(|t| t.id),
        }
    }

    fn ids(&self, p: usize, w: Option<usize>, t: Option<usize>) -> Ids {
        let project = &self.projects[p];
        let workspace = w.and_then(|w| project.workspaces.get(w));
        let tab = workspace.zip(t).and_then(|(workspace, t)| workspace.tabs.get(t));
        Ids {
            project: Some(project.id),
            workspace: workspace.map(|w| w.id),
            tab: tab.map(|t| t.id),
            pane: tab.and_then(Tab::pane).map(|t| t.id),
        }
    }

    fn shown_ids(&self, p: usize) -> Ids {
        let project = &self.projects[p];
        let w = project.workspace().map(|_| project.active);
        self.ids(p, w, project.workspace().map(|w| w.active))
    }

    fn here_workspace(&mut self, caller: Option<u64>) -> Result<(usize, usize), String> {
        let here = self.here(caller);
        let p = here.project.ok_or(NO_PROJECT)?;
        Ok((p, here.workspace.unwrap_or_else(|| self.ensure_workspace(p))))
    }

    fn target(&self, caller: Option<u64>, pane: Option<u64>, tab: Option<u64>) -> Result<u64, String> {
        if let Some(id) = pane {
            return self.locate(id).map(|_| id).ok_or_else(|| none("pane", id));
        }
        if let Some(id) = tab {
            let (p, w, t) = self.locate_tab(id).ok_or_else(|| none("tab", id))?;
            let tab = &self.projects[p].workspaces[w].tabs[t];
            let agent = std::iter::once(tab.active)
                .chain(0..tab.panes.len())
                .filter_map(|i| tab.panes.get(i))
                .find(|term| self.runs_agent(term));
            return agent.or_else(|| tab.pane()).map(|t| t.id).ok_or_else(|| format!("tab {id} has no pane"));
        }
        self.here(caller).pane.ok_or_else(|| NO_PANE.to_string())
    }

    fn show(&mut self, item: Item) -> Result<(), String> {
        let place = |app: &Self, p: usize, w: Option<usize>, t: Option<usize>| {
            let Ids { project, workspace, tab, .. } = app.ids(p, w, t);
            Goto::Place { project: project.unwrap_or_default(), workspace, tab }
        };
        let goto = match item {
            Item::Group(id) => {
                self.group_index(id).ok_or_else(|| none("group", id))?;
                Goto::Group(id)
            }
            Item::Project(id) => {
                let p = self.project_index(id).ok_or_else(|| none("project", id))?;
                place(self, p, None, None)
            }
            Item::Workspace(id) => {
                let (p, w) = self.usable_workspace(id)?;
                place(self, p, Some(w), None)
            }
            Item::Tab(id) => {
                let (p, w, t) = self.locate_tab(id).ok_or_else(|| none("tab", id))?;
                place(self, p, Some(w), Some(t))
            }
            Item::Pane(id) => {
                let (p, w, t) = self.locate(id).ok_or_else(|| none("pane", id))?;
                self.projects[p].workspaces[w].tabs[t].focus(id);
                place(self, p, Some(w), Some(t))
            }
        };
        self.goto(goto);
        Ok(())
    }

    pub(super) fn report(&self, caller: Option<u64>) -> Report {
        let caller = caller.filter(|id| self.locate(*id).is_some());
        let tab_info = |t: usize, tab: &Tab, active: usize| TabInfo {
            id: tab.id,
            name: tab.label(&self.config),
            status: tab.status().map(|s| s.name().to_string()),
            active: t == active,
            panes: tab
                .panes
                .iter()
                .enumerate()
                .map(|(i, term)| self.pane_info(term, i == tab.active, caller))
                .collect(),
        };
        let workspace_info = |w: usize, ws: &Workspace, active: usize| WorkspaceInfo {
            id: ws.id,
            name: ws.label(),
            path: ws.path.clone(),
            branch: git::branch(&ws.path),
            worktree: ws.worktree,
            behind: ws.behind,
            active: w == active,
            tabs: ws.tabs.iter().enumerate().map(|(t, tab)| tab_info(t, tab, ws.active)).collect(),
        };
        let projects = self.projects.iter().enumerate().filter(|(_, p)| !p.closing).map(|(p, project)| ProjectInfo {
            id: project.id,
            name: self.project_label(project),
            path: project.path.clone(),
            group: project.group,
            active: p == self.active,
            workspaces: project
                .workspaces
                .iter()
                .enumerate()
                .filter(|(_, w)| !w.closing())
                .map(|(w, ws)| workspace_info(w, ws, project.active))
                .collect(),
        });
        let groups = self.groups.iter().map(|g| GroupInfo {
            id: g.id,
            name: g.entry.name.clone(),
            collapsed: g.entry.collapsed,
        });
        Report {
            version: update::CURRENT.to_string(),
            caller,
            shown: self.project().map(|_| self.shown_ids(self.active)).unwrap_or_default(),
            groups: groups.collect(),
            projects: projects.collect(),
        }
    }

    fn pane_info(&self, term: &Term, active: bool, caller: Option<u64>) -> PaneInfo {
        let context = term.context.context();
        PaneInfo {
            id: term.id,
            path: term.cwd(),
            program: term.program(&self.config),
            agent: term.agent.agent().map(str::to_string),
            status: term.agent.status().map(|s| s.name().to_string()),
            background_shell: term.agent.background_shell(),
            dialog: term.dialog(),
            survey: term.survey(),
            at_prompt: Some(term.shell_in_foreground()),
            model: context.map(|c| c.model.clone()),
            context: context.and_then(|c| c.percent),
            active,
            caller: caller == Some(term.id),
            resumes: self.config.resume_agents && term.resume.is_some(),
        }
    }

    fn open_request(&mut self, open: &control::Open, area: Option<Rect>) -> Handled {
        let path = open.path.canonicalize().map_err(|e| format!("cannot open `{}`: {e}", open.path.display()))?;
        if !path.is_dir() {
            return Err(format!("`{}` is not a folder", path.display()));
        }
        let p = match self.projects.iter().position(|p| p.path == path && !p.closing) {
            Some(p) => p,
            None => self.add_project(path, sized(area)?).map_err(|e| e.to_string())?,
        };
        let ids = self.shown_ids(p);
        if open.focus {
            self.show(Item::Project(self.projects[p].id))?;
        }
        Ok(Some(json(&Done { ids, ..Done::default() })))
    }

    fn new_workspace_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        new: control::NewWorkspace,
        area: Option<Rect>,
    ) -> Handled {
        let area = sized(area)?;
        let name = name_of(Some(new.name)).ok_or("the workspace needs a name")?;
        let p = match new.project {
            Some(id) => self.project_index(id).ok_or_else(|| none("project", id))?,
            None => self.here(caller).project.ok_or(NO_PROJECT)?,
        };
        if new.worktree {
            if let Some(open) = self.workspace_on(p, &name) {
                return Err(format!("branch {name} is already open in workspace {open}"));
            }
            let then = Then { start: None, wait: None, focus: new.focus };
            return self.create_worktree(client, p, &name, then, None).map(|()| None);
        }
        let path = self.projects[p].path.clone();
        let workspace = self.new_workspace(area, path, Some(name), false).map_err(|e| e.to_string())?;
        self.projects[p].workspaces.push(workspace);
        let w = self.projects[p].workspaces.len() - 1;
        let ids = self.ids(p, Some(w), Some(0));
        if new.focus {
            self.show(Item::Tab(ids.tab.unwrap_or_default()))?;
        }
        Ok(Some(json(&Done { ids, ..Done::default() })))
    }

    fn usable_workspace(&self, id: u64) -> Result<(usize, usize), String> {
        let (p, w) = self.workspace_position(id).ok_or_else(|| none("workspace", id))?;
        match self.projects[p].workspaces[w].phase {
            Phase::Open => Ok((p, w)),
            Phase::Removing(_) => Err(format!("workspace {id} is being removed")),
            Phase::Closing => Err(format!("workspace {id} is closing")),
        }
    }

    fn workspace_on(&self, p: usize, branch: &str) -> Option<u64> {
        let workspaces = self.projects[p].workspaces.iter().filter(|w| w.open());
        workspaces.into_iter().find(|w| git::branch(&w.path).as_deref() == Some(branch)).map(|w| w.id)
    }

    fn create_worktree(
        &mut self,
        client: u64,
        p: usize,
        branch: &str,
        then: Then,
        timeout: Option<(Instant, f64)>,
    ) -> Result<(), String> {
        let project = &self.projects[p];
        if !git::is_repo_root(&project.path) {
            return Err(format!("project {} is not the root of a git repository, so it has no worktrees", project.id));
        }
        let key = self.requests.key();
        self.spawn_worktree(p, branch.to_string(), None, Some(key));
        let stage = Stage::Worktree(then);
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        Ok(())
    }

    pub(super) fn worktree_ready(&mut self, key: u64, project: u64, result: error::Result<PathBuf>, area: Rect) {
        let found = self.requests.pending.iter().position(|p| p.key == key && matches!(p.stage, Stage::Worktree(_)));
        let Some(i) = found else { return };
        let mut pending = self.requests.pending.remove(i);
        let Stage::Worktree(then) = std::mem::replace(&mut pending.stage, Stage::launch(None)) else { return };
        match self.open_worktree(project, result, then, area, &mut pending) {
            Ok(true) => self.requests.pending.push(pending),
            Ok(false) => self.answer(pending.client, Ok(json(&pending.done))),
            Err(message) => self.answer(pending.client, Err(message)),
        }
    }

    fn open_worktree(
        &mut self,
        project: u64,
        result: error::Result<PathBuf>,
        then: Then,
        area: Rect,
        pending: &mut Pending,
    ) -> Result<bool, String> {
        let path = result.map_err(|e| e.to_string())?;
        let path = path.canonicalize().unwrap_or(path);
        let p = self.project_index(project).ok_or_else(|| format!("project {project} closed"))?;
        let w = self.worktree_workspace(p, path);
        let launch = match then.start {
            Some((spec, name)) => {
                let t = self.push_tab(p, w, area, name).map_err(|e| e.to_string())?;
                Some((spec, t))
            }
            None if self.projects[p].workspaces[w].tabs.is_empty() => {
                self.push_tab(p, w, area, None).map_err(|e| e.to_string())?;
                None
            }
            None => None,
        };
        let t = launch.as_ref().map_or(self.projects[p].workspaces[w].active, |(_, t)| *t);
        pending.done.ids = self.ids(p, Some(w), Some(t));
        let pane = pending.done.ids.pane.unwrap_or_default();
        if then.focus {
            self.show(Item::Pane(pane))?;
        }
        let Some((spec, _)) = launch else { return Ok(false) };
        let mut launch = Launch::new(pane, spec, Instant::now());
        launch.key = Some(pending.key);
        self.launches.push(launch);
        pending.stage = Stage::launch(then.wait);
        Ok(true)
    }

    fn new_tab_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        new: control::NewTab,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let (p, w) = match new.workspace {
            Some(id) => self.usable_workspace(id)?,
            None => self.here_workspace(caller)?,
        };
        let t = self.push_tab(p, w, area, name_of(new.name)).map_err(|e| e.to_string())?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        if new.focus {
            self.show(Item::Pane(pane))?;
        }
        let launch = new.command.map(|command| Launch::command(pane, command, now));
        Ok(self.launch_within(client, Done { ids, ..Done::default() }, launch, Stage::launch(None), None))
    }

    fn split_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        split: control::Split,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let pane = self.target(caller, split.pane, None)?;
        let (p, w, t) = self.locate(pane).ok_or_else(|| none("pane", pane))?;
        let (dir, way) = if split.down { (Dir::Down, "down") } else { (Dir::Right, "right") };
        if !self.projects[p].workspaces[w].tabs[t].can_split(self.layout(area).pane, pane, dir) {
            return Err(format!("pane {pane} is too small to split {way}"));
        }
        let new =
            self.split_pane(pane, dir, area, false).map_err(|e| e.to_string())?.ok_or_else(|| none("pane", pane))?;
        let ids = Ids { pane: Some(new), ..self.ids(p, Some(w), Some(t)) };
        if split.focus {
            self.show(Item::Pane(new))?;
        }
        let launch = split.command.map(|command| Launch::command(new, command, now));
        Ok(self.launch_within(client, Done { ids, ..Done::default() }, launch, Stage::launch(None), None))
    }

    fn start_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        start: control::Start,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let timeout = deadline(start.timeout, now)?;
        let wait = start.wait.then(|| Condition::of(start.until)).transpose()?;
        let kind = self.agent_kind(caller, start.agent)?;
        let prompt = start.prompt.filter(|p| !p.trim().is_empty());
        let spec = launch::Spec { command: agents::command_line(&self.config, &kind), prompt, submit: true };
        let name = name_of(start.name);
        let (p, w) = if let Some(id) = start.workspace {
            self.usable_workspace(id)?
        } else if let Some(branch) = start.worktree.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            let p = self.here(caller).project.ok_or(NO_PROJECT)?;
            let open = self.workspace_on(p, branch).and_then(|id| self.workspace_position(id));
            let Some(found) = open else {
                let then = Then { start: Some((spec, name)), wait, focus: start.focus };
                return self.create_worktree(client, p, branch, then, timeout).map(|()| None);
            };
            found
        } else {
            self.here_workspace(caller)?
        };
        let t = self.push_tab(p, w, area, name).map_err(|e| e.to_string())?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        if start.focus {
            self.show(Item::Pane(pane))?;
        }
        let launch = Some(Launch::new(pane, spec, now));
        Ok(self.launch_within(client, Done { ids, ..Done::default() }, launch, Stage::launch(wait), timeout))
    }

    fn agent_kind(&self, caller: Option<u64>, agent: Option<String>) -> Result<String, String> {
        let kinds = agents::kinds(&self.config);
        if let Some(agent) = agent {
            return if kinds.contains(&agent) {
                Ok(agent)
            } else {
                Err(format!(
                    "unknown agent `{agent}`; cornercase knows {}, and more can be added with agent_commands in \
                     config.json",
                    kinds.join(", ")
                ))
            };
        }
        let running = self.here(caller).pane.and_then(|id| self.pane_by(id));
        let running = running.and_then(|term| agents::detect(&self.config, &term.foreground_args()));
        agents::resolve(&self.config, None, running.as_deref()).ok_or_else(|| NO_AGENT.to_string())
    }

    fn send_request(&mut self, client: u64, caller: Option<u64>, send: control::SendText, now: Instant) -> Handled {
        let pane = self.target(caller, send.pane, send.tab)?;
        let timeout = deadline(send.timeout, now)?;
        let term = self.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        if term.agent.status() == Some(Status::Waiting) {
            return Err(format!(
                "the agent in pane {pane} is waiting for an answer to a question or a permission prompt, and what \
                 you send would answer it; ask the user, then answer with `cornercase keys`"
            ));
        }
        if !send.force && term.dialog() {
            return Err(format!(
                "the agent in pane {pane} shows a dialog, a panel or its shell mode instead of its input box, and \
                 what you send would go there; `cornercase read --pane {pane}` shows it: send again once it is \
                 closed, or with --force"
            ));
        }
        if !send.force && term.survey() {
            return Err(format!(
                "the agent in pane {pane} shows Claude Code's feedback survey above its input box (`1: Bad  2: Fine  \
                 3: Good  0: Dismiss`), and a digit sent alone would answer it; dismiss it with `cornercase keys \
                 --pane {pane} 0` and send again, or send with --force"
            ));
        }
        let local = agents::detect(&self.config, &term.foreground_args()).as_deref() == Some(agents::GEMINI)
            && send.text.as_deref().is_some_and(|text| text.trim_start().starts_with(['/', '?', '!']));
        let confirm = (send.enter && !local && self.watched_agent(term).is_ok()).then(SystemTime::now);
        let wait = send.wait.then(|| Condition::of(send.until)).transpose()?;
        if let Some(until) = &wait {
            self.can_wait(caller, pane, until)?;
        }
        let text = send.text.filter(|t| !t.is_empty());
        if text.is_none() && !send.enter {
            return Err("nothing to send: give a text, - to read it from stdin, or --enter".into());
        }
        if let Some(text) = &text
            && let Some(term) = self.pane_by_mut(pane)
            && !term.paste(text)
        {
            return Err(not_reading(pane));
        }
        if !send.enter {
            return Ok(Some(json(&pane_ids(pane))));
        }
        let stage = Stage::Launch { confirm, wait };
        Ok(self.launch_within(client, pane_ids(pane), Some(Launch::enter(pane, now)), stage, timeout))
    }

    fn can_wait(&self, caller: Option<u64>, pane: u64, until: &Condition) -> Result<(), String> {
        let term = self.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        if until.needs_agent()
            && let Err(agent) = self.watched_agent(term)
        {
            let runs = agent.map_or_else(
                || "runs no agent".to_string(),
                |agent| {
                    format!(
                        "runs {agent}, and cornercase only knows what Claude Code, Codex, Gemini CLI and opencode \
                         are doing"
                    )
                },
            );
            return Err(format!(
                "pane {pane} {runs}, so there is nothing to wait for; wait with --until shell, --text or --quiet instead"
            ));
        }
        if caller == Some(pane) && (until.needs_agent() || matches!(until, Condition::Shell)) {
            return Err(format!(
                "pane {pane} is the one running this command, so it would wait for itself; pass --pane"
            ));
        }
        Ok(())
    }

    fn keys_request(&mut self, caller: Option<u64>, keys: &control::Keys, now: Instant) -> Handled {
        let pane = self.target(caller, keys.pane, keys.tab)?;
        let events = keys
            .keys
            .iter()
            .map(|name| keys::named(name).ok_or_else(|| format!("unknown key `{name}`")))
            .collect::<Result<Vec<_>, _>>()?;
        if events.is_empty() {
            return Err("no keys given".into());
        }
        let term = self.pane_by_mut(pane).ok_or_else(|| none("pane", pane))?;
        let enter = events.iter().any(|key| key.code == KeyCode::Enter);
        let bytes: Vec<u8> = events.into_iter().flat_map(|key| term.emulator.encode_key(key)).collect();
        if !term.write(&bytes) {
            return Err(not_reading(pane));
        }
        if enter {
            term.submitted = Some(now);
        }
        Ok(Some(json(&pane_ids(pane))))
    }

    fn read_request(&self, caller: Option<u64>, read: &control::Read) -> Handled {
        let pane = self.target(caller, read.pane, read.tab)?;
        let term = self.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let text = match read.lines {
            Some(lines) => term.emulator.last_lines(lines),
            None => term.emulator.screen_text(),
        };
        let text = text.map_err(|e| format!("cannot read pane {pane}: {e}"))?;
        Ok(Some(json(&Done { text: Some(text), ..pane_ids(pane) })))
    }

    fn last_message_request(&mut self, client: u64, caller: Option<u64>, last: &control::LastMessage) -> Handled {
        let pane = self.target(caller, last.pane, last.tab)?;
        let term = self.pane_by(pane).ok_or_else(|| none("pane", pane))?;
        let running = term.agent.agent().map(str::to_string);
        let agent = match running.or_else(|| agents::detect(&self.config, &term.foreground_args())) {
            Some(agent) if WATCHED.contains(&agent.as_str()) => agent,
            other => {
                let runs = other.map_or_else(|| "runs no agent".to_string(), |agent| format!("runs {agent}"));
                return Err(format!(
                    "pane {pane} {runs}, and cornercase only reads the messages of Claude Code, Codex, Gemini CLI \
                     and opencode; \
                     `cornercase read --pane {pane}` prints its screen"
                ));
            }
        };
        let record = term.context.record_for(&agent).ok_or_else(|| unfound(pane, &agent))?;
        let key = self.requests.key();
        let tx = self.tx.clone();
        let job = Job::new(Level::Debug, "control", "last message").with("pane", pane).with("agent", &agent).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| record.last_message());
            job.with("found", result.as_ref().is_ok_and(Option::is_some)).finish(&result);
            let _ = tx.send(AppEvent::LastMessage { request: key, result });
        });
        let done = Done { agent: Some(agent), ..pane_ids(pane) };
        self.requests.pending.push(Pending { client: Some(client), key, timeout: None, done, stage: Stage::Reading });
        Ok(None)
    }

    pub(super) fn message_read(&mut self, key: u64, result: error::Result<Option<context::Said>>) {
        let Some(mut pending) = self.requests.take(key) else { return };
        let pane = pending.done.ids.pane.unwrap_or_default();
        let agent = pending.done.agent.clone().unwrap_or_default();
        let reply = match result {
            Ok(Some(message)) => {
                pending.done.text = Some(message.text);
                pending.done.written = message.written;
                pending.done.turn_over = self.pane_by(pane).and_then(|term| turn_over(&term.agent));
                Ok(json(&pending.done))
            }
            Ok(None) => Err(format!("the {agent} agent in pane {pane} has not written a message yet")),
            Err(e) => Err(format!("cannot read what the {agent} agent in pane {pane} wrote: {e}")),
        };
        self.answer(pending.client, reply);
    }

    fn wait_request(&mut self, client: u64, caller: Option<u64>, wait: control::Wait, now: Instant) -> Handled {
        let pane = self.target(caller, wait.pane, wait.tab)?;
        let timeout = deadline(wait.timeout, now)?;
        let until = Condition::of(wait.until)?;
        self.can_wait(caller, pane, &until)?;
        let key = self.requests.key();
        let watch = self.watch(pane, until, now);
        let pending = Pending { client: Some(client), key, timeout, done: pane_ids(pane), stage: Stage::Watch(watch) };
        self.requests.pending.push(pending);
        Ok(None)
    }

    fn wait_several_request(&mut self, client: u64, caller: Option<u64>, wait: control::Wait, now: Instant) -> Handled {
        let timeout = deadline(wait.timeout, now)?;
        let given = wait.pane.into_iter().chain(wait.panes).map(|id| (None, Some(id)));
        let given = given.chain(wait.tab.into_iter().chain(wait.tabs).map(|id| (Some(id), None)));
        let mut targets: Vec<(Option<u64>, Option<u64>)> = Vec::new();
        for target in given {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        if targets.is_empty() {
            targets.push((None, None));
        }
        let mut parts = Vec::new();
        for (tab, pane) in targets {
            let id = self.target(caller, pane, tab)?;
            let until = Condition::of(wait.until.clone())?;
            self.can_wait(caller, id, &until)?;
            let done = Done { ids: Ids { tab, pane: Some(id), ..Ids::default() }, ..Done::default() };
            let stage = Stage::Watch(self.watch(id, until, now));
            parts.push(Pending { client: None, key: 0, timeout: None, done, stage });
        }
        let key = self.requests.key();
        let stage = Stage::Several { all: wait.all, parts };
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        Ok(None)
    }

    fn close_request(&mut self, client: u64, close: &control::Close) -> Handled {
        match close.item {
            Item::Pane(id) => self.pane_by_mut(id).ok_or_else(|| none("pane", id))?.kill(),
            Item::Tab(id) => {
                let (p, w, t) = self.locate_tab(id).ok_or_else(|| none("tab", id))?;
                self.projects[p].workspaces[w].tabs[t].panes.iter_mut().for_each(Term::kill);
            }
            Item::Workspace(id) => {
                let (p, w) = self.usable_workspace(id)?;
                if close.remove_worktree {
                    if !self.projects[p].workspaces[w].worktree {
                        return Err(format!("workspace {id} is not a git worktree"));
                    }
                    let key = self.requests.key();
                    self.spawn_check(p, w, Some(key));
                    let pending = Pending {
                        client: Some(client),
                        key,
                        timeout: None,
                        done: Done::default(),
                        stage: Stage::Removing { force: close.force },
                    };
                    self.requests.pending.push(pending);
                    return Ok(None);
                }
                let workspace = &mut self.projects[p].workspaces[w];
                if workspace.worktree {
                    workspace.kill();
                } else {
                    self.drop_workspace(self.projects[p].id, id);
                }
            }
            Item::Project(id) => {
                self.project_index(id).ok_or_else(|| none("project", id))?;
                self.close_project(id);
            }
            Item::Group(id) => return Err(format!("group {id} cannot be closed; close its projects")),
        }
        Ok(Some(json(&Done::default())))
    }

    pub(super) fn removal_checked(&mut self, key: u64, project: u64, workspace: u64, status: &worktree::Status) {
        let force =
            self.requests.pending.iter().any(|p| p.key == key && matches!(p.stage, Stage::Removing { force: true }));
        let clear = status.lock.is_none() && (force || !status.changed);
        if clear && self.start_removal(project, workspace, force, false, Some(key)) {
            return;
        }
        let Some(pending) = self.requests.take(key) else { return };
        let message = if let Some(lock) = &status.lock {
            self.locked(workspace, lock)
        } else if status.changed && !force {
            format!("workspace {workspace} has changes that are not committed; add --force to remove it anyway")
        } else {
            self.usable_workspace(workspace).err().unwrap_or_else(|| none("workspace", workspace))
        };
        self.answer(pending.client, Err(message));
    }

    fn locked(&self, workspace: u64, lock: &worktree::Lock) -> String {
        let path = self
            .workspace_position(workspace)
            .map(|(p, w)| self.projects[p].workspaces[w].path.display().to_string())
            .unwrap_or_default();
        let reason = if lock.reason.is_empty() { String::new() } else { format!(" ({})", lock.reason) };
        format!(
            "workspace {workspace} is locked{reason}; unlock it with `git worktree unlock {}` to remove it",
            shell_quote(&path)
        )
    }

    pub(super) fn worktree_gone(&mut self, key: u64, project: u64, workspace: u64, result: error::Result<()>) {
        let pending = self.requests.take(key);
        self.settle_removal(project, workspace, result.is_ok());
        let Some(pending) = pending else { return };
        self.answer(pending.client, result.map(|()| json(&Done::default())).map_err(|e| e.to_string()));
    }

    fn rename_request(&mut self, caller: Option<u64>, rename: control::Rename) -> Handled {
        let target = match rename.item {
            Some(Item::Group(id)) => {
                self.group_index(id).ok_or_else(|| none("group", id))?;
                Target::Group(id)
            }
            Some(Item::Project(id)) => {
                self.project_index(id).ok_or_else(|| none("project", id))?;
                Target::Project(id)
            }
            Some(Item::Workspace(id)) => {
                let (p, _) = self.usable_workspace(id)?;
                Target::Workspace(self.projects[p].id, id)
            }
            Some(Item::Tab(id)) => self.tab_target(self.locate_tab(id)).ok_or_else(|| none("tab", id))?,
            Some(Item::Pane(id)) => self.tab_target(self.locate(id)).ok_or_else(|| none("pane", id))?,
            None => {
                let here = self.here(caller);
                let found = here.project.zip(here.workspace).zip(here.tab).map(|((p, w), t)| (p, w, t));
                self.tab_target(found).ok_or("no tab is open here; pass --tab")?
            }
        };
        let name = name_of(Some(rename.name));
        if matches!(target, Target::Group(_)) && name.is_none() {
            return Err("a group needs a name".into());
        }
        self.rename(target, name);
        Ok(Some(json(&Done::default())))
    }

    pub(super) fn tab_target(&self, found: Option<(usize, usize, usize)>) -> Option<Target> {
        let (p, w, t) = found?;
        let project = &self.projects[p];
        let workspace = &project.workspaces[w];
        Some(Target::Tab(project.id, workspace.id, workspace.tabs[t].id))
    }

    fn notify_request(&mut self, text: &str) -> Handled {
        let message = notify::clean(text).trim().to_string();
        if message.is_empty() {
            return Err("the notification needs a text".into());
        }
        self.notifications.extend(Notification::new(&message, &self.config.desktop_notifications));
        self.toast = Some(Toast::new(message, ui::ToastIcon::Check));
        Ok(Some(json(&Done::default())))
    }

    fn restart_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        restart: &control::RestartWhenIdle,
        now: Instant,
    ) -> Handled {
        let timeout = deadline(restart.timeout, now)?;
        let key = self.requests.key();
        let stage = Stage::Restart { caller };
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        self.show_pending_restart(now);
        Ok(None)
    }

    fn events_request(&mut self, client: u64, events: control::Events) -> Handled {
        if let Some(pane) = events.panes.iter().find(|pane| self.locate(**pane).is_none()) {
            return Err(none("pane", *pane));
        }
        self.publish(log::enabled(Level::Info));
        self.events.subscribe(client, events.panes);
        Ok(Some(json(&Done::default())))
    }

    fn todo_request(&mut self, todo: control::Todo) -> Handled {
        let done = match todo {
            control::Todo::Add(text) => {
                Done { todo: Some(self.todos.add(&text).ok_or("the todo needs a text")?), ..Done::default() }
            }
            control::Todo::List => {
                let todos =
                    self.todos.items().iter().map(|i| TodoItem { id: i.id, text: i.text.clone(), done: i.done });
                return Ok(Some(json(&TodoList { todos: todos.collect() })));
            }
            control::Todo::Done(id) => {
                self.todos.item(id).ok_or_else(|| none("todo", id))?;
                self.todos.toggle(id);
                Done::default()
            }
            control::Todo::Rm(id) => {
                self.todos.remove(id).ok_or_else(|| none("todo", id))?;
                Done::default()
            }
        };
        Ok(Some(json(&done)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::host_theme::HostTheme;
    use crate::test_util::TempDir;

    mod shell_quote {
        use rstest::rstest;

        use super::super::shell_quote;

        #[rstest]
        #[case::plain("/home/me/wt-1.x", "/home/me/wt-1.x")]
        #[case::a_space("/home/me/my wt", "'/home/me/my wt'")]
        #[case::a_single_quote("/home/it's", "'/home/it'\\''s'")]
        fn paths_are_quoted_for_a_shell(#[case] path: &str, #[case] expected: &str) {
            assert_eq!(shell_quote(path), expected);
        }
    }

    mod holds_prompts {
        use rstest::rstest;

        use super::super::holds_prompts;
        use crate::activity::Status;
        use crate::agents::{CLAUDE, CODEX, GEMINI, OPENCODE};

        #[rstest]
        #[case::codex_at_work(Some(CODEX), Some(Status::Working), true)]
        #[case::codex_asking(Some(CODEX), Some(Status::Waiting), true)]
        #[case::gemini_at_work(Some(GEMINI), Some(Status::Working), true)]
        #[case::gemini_asking(Some(GEMINI), Some(Status::Waiting), true)]
        #[case::gemini_idle(Some(GEMINI), Some(Status::Idle), false)]
        #[case::codex_idle(Some(CODEX), Some(Status::Idle), false)]
        #[case::claude_at_work(Some(CLAUDE), Some(Status::Working), false)]
        #[case::opencode_at_work(Some(OPENCODE), Some(Status::Working), false)]
        fn agents_that_queue_prompts_record_them_late(
            #[case] agent: Option<&str>,
            #[case] status: Option<Status>,
            #[case] holds: bool,
        ) {
            assert_eq!(holds_prompts(agent, status), holds);
        }
    }

    mod answer_lost_requests {
        use super::*;

        #[test]
        fn answers_the_bug_to_each_request_with_nothing_left_pending() {
            let dir = TempDir::new();
            let (tx, _rx) = mpsc::channel();
            let mut app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), tx);
            let removing = Pending {
                client: Some(1),
                key: 1,
                timeout: None,
                done: Done::default(),
                stage: Stage::Removing { force: false },
            };
            app.requests.pending.push(removing);
            app.requests.open.extend([1, 2]);

            app.answer_lost_requests();

            let bug = serde_json::to_string(&Response::Error(error::Error::Bug.to_string())).expect("json");
            assert_eq!(app.take_answers(), [(2, bug)]);
        }
    }

    mod forget {
        use super::*;

        #[test]
        fn a_last_message_read_goes_with_its_client() {
            let dir = TempDir::new();
            let (tx, _rx) = mpsc::channel();
            let mut app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), tx);
            let reading = Pending { client: Some(1), key: 1, timeout: None, done: pane_ids(4), stage: Stage::Reading };
            app.requests.pending.push(reading);
            app.requests.open.push(1);

            app.forget(1);
            let left = app.requests.pending.len();
            app.message_read(1, Ok(Some(context::Said { text: "Done.".into(), written: None })));

            assert_eq!((left, app.take_answers()), (0, Vec::new()));
        }
    }
}
