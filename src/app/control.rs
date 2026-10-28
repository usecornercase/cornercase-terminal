mod account_restart;
pub(in crate::app) mod accounts;
mod arrange;
mod changes;
mod close;
mod closed_agents;
mod config;
mod contract;
mod create;
mod files;
mod flow;
mod held;
mod idle_restart;
mod issues;
mod merge_check;
mod plugins;
mod ports;
mod processes;
mod queue;
mod search;
mod status;
mod talk;
mod terminal;
mod tour;
mod updates;
mod usage;
mod wait;
mod worktrees;

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;
use std::time::{Duration, Instant, SystemTime};

use ratatui::layout::Rect;
use serde::Serialize;
use serde_json::Value;

use crate::agents;
use crate::agents::launch::Launch;
use crate::app::App;
use crate::app::flow_runs::Starting;
use crate::app::messenger::CLIENTS as MESSENGER_CLIENTS;
use crate::app::{changes_panel, events, files_panel, pasted_files, surfaces, views};
use crate::control::contract::{Code, Refusal};
use crate::control::start_server::Started;
use crate::control::{self, Command, Done, Ids, Item, Request, StartServer};
use crate::error;
use crate::git;
use crate::log::{self, Level};
use crate::project::search::Goto;
use crate::project::{Hierarchy, Phase, Tab, TabAt, Workspace, WorkspaceAt};
use crate::term::Term;
pub(in crate::app) use changes::{BaseChange, Replied};
pub(in crate::app) use config::needs_a_repository;
use create::Then;
use held::Held;
use issues::Asking;
use wait::{Condition, Confirm, Watch};

const STARTS_WITHIN: Duration = Duration::from_secs(10);

use idle_restart::INPUT_HOLDS;

const CONFIRM_WITHIN: Duration = Duration::from_secs(10);

const NO_SIZE: &str = "this cornercase server has not started its session yet, so a new terminal would have no size; \
    run `cornercase start-server` (or open a window with `cornercase`) first";

const NO_PROJECT: &str = "no project is open; open one with `cornercase open PATH`";

const NO_PANE: &str = "no pane is open here; pass --pane or --tab (`cornercase status` lists them)";

type Reply = Result<Value, String>;

pub(in crate::app) type Handled = Result<Option<Value>, Refusal>;

pub(super) struct Clients {
    pub(super) requests: Requests,
    pub(super) events: events::Events,
    pub(super) change_streams: changes_panel::Streams,
    pub(super) file_streams: files_panel::Streams,
    pub(super) surfaces: surfaces::Surfaces,
    pub(super) views: views::Views,
    pub(super) pasted: pasted_files::PastedFiles,
    confirm_within: Duration,
    typing_holds: Duration,
}

impl Default for Clients {
    fn default() -> Self {
        Self {
            requests: Requests::default(),
            events: events::Events::default(),
            change_streams: changes_panel::Streams::default(),
            file_streams: files_panel::Streams::default(),
            surfaces: surfaces::Surfaces::default(),
            views: views::Views::default(),
            pasted: pasted_files::PastedFiles::default(),
            confirm_within: CONFIRM_WITHIN,
            typing_holds: INPUT_HOLDS,
        }
    }
}

#[cfg(test)]
impl Clients {
    pub(super) fn set_confirm_within(&mut self, within: Duration) {
        self.confirm_within = within;
    }

    pub(super) fn set_typing_holds(&mut self, holds: Duration) {
        self.typing_holds = holds;
    }
}

#[derive(Default)]
pub(super) struct Requests {
    pending: Vec<Pending>,
    open: Vec<u64>,
    asked: HashMap<u64, (Instant, &'static str)>,
    callers: HashMap<u64, u64>,
    structured: HashSet<u64>,
    answers: Vec<(u64, String)>,
    launched: Vec<(u64, bool)>,
    todo_sends: HashMap<u64, Sending>,
    queued: HashMap<u64, u64>,
    restarting: HashMap<u64, account_restart::Batch>,
    next: u64,
}

struct Sending {
    todo: u64,
    text: String,
    confirmed: bool,
}

impl Requests {
    pub(super) fn launched(&mut self, key: u64, started: bool) {
        self.launched.push((key, started));
    }

    fn key(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn push(&mut self, client: u64, key: u64, waits: Waits) {
        let Waits { done, stage, timeout } = waits;
        self.pending.push(Pending { client: Some(client), key, timeout, done, stage });
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
    Run(u64, Box<Starting>),
    Removing { force: bool, unlock: bool },
    Check,
    Held(Box<Held>),
    Launch { confirm: Option<SystemTime>, compacts: bool, wait: Option<Condition> },
    Confirm(Confirm),
    Watch(Watch),
    Several { all: bool, parts: Vec<Pending> },
    Reading,
    Setup(u64),
    Restart { caller: Option<u64> },
    Issue(Box<Asking>),
    Usage(Vec<(agents::usage::Key, u64)>),
    Ports,
    Processes,
}

impl Stage {
    fn listing(&self) -> &'static str {
        match self {
            Self::Processes => "the processes are still being measured",
            _ => "the ports are still being listed",
        }
    }

    fn launch(wait: Option<Condition>) -> Self {
        Self::Launch { confirm: None, compacts: false, wait }
    }

    fn outlives_its_client(&self) -> bool {
        !matches!(
            self,
            Self::Held(_)
                | Self::Confirm(_)
                | Self::Watch(_)
                | Self::Several { .. }
                | Self::Reading
                | Self::Restart { .. }
                | Self::Check
                | Self::Issue(_)
                | Self::Usage(_)
                | Self::Ports
                | Self::Processes
        )
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::app) struct Asker {
    pub(in crate::app) caller: Option<u64>,
    pub(in crate::app) view: Option<u64>,
}

impl From<Option<u64>> for Asker {
    fn from(caller: Option<u64>) -> Self {
        Self { caller, view: None }
    }
}

#[derive(Clone, Copy)]
pub(in crate::app) struct Asked {
    pub(in crate::app) client: u64,
    pub(in crate::app) asker: Asker,
    pub(in crate::app) area: Option<Rect>,
    pub(in crate::app) now: Instant,
}

struct Waits {
    done: Done,
    stage: Stage,
    timeout: Option<(Instant, f64)>,
}

#[derive(Default)]
struct Here {
    project: Option<usize>,
    workspace: Option<WorkspaceAt>,
    tab: Option<TabAt>,
    pane: Option<u64>,
}

pub(in crate::app) fn json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn sized(area: Option<Rect>) -> Result<Rect, String> {
    area.ok_or_else(|| NO_SIZE.to_string())
}

pub(in crate::app) fn malformed(message: impl Into<String>) -> Refusal {
    Refusal::new(Code::Malformed, message)
}

pub(in crate::app) fn conflict(message: impl Into<String>) -> Refusal {
    Refusal::new(Code::Conflict, message)
}

pub(in crate::app) fn no_popup() -> Refusal {
    Refusal::new(Code::NotFound, "no plugin popup is open")
}

pub(in crate::app) fn none(kind: &str, id: u64) -> Refusal {
    Refusal::new(Code::NotFound, format!("there is no {kind} {id}; `cornercase status` lists them"))
}

fn stale_boot(boot: &str) -> Refusal {
    let current = control::server_token();
    Refusal::new(
        Code::StaleBoot,
        format!(
            "this request is for the cornercase server started as {boot}, but this one started as {current}; \
            its ids may name other items now, so read `cornercase status` again"
        ),
    )
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

fn endings(value: &Value) -> String {
    let ended = |value: &Value| value.get("ended").and_then(Value::as_str).unwrap_or("-").to_string();
    let Some(panes) = value.get("panes").and_then(Value::as_array) else { return ended(value) };
    let id = |pane: &Value| pane.get("tab").or_else(|| pane.get("pane")).and_then(Value::as_u64).unwrap_or_default();
    panes.iter().map(|pane| format!("{}:{}", id(pane), ended(pane))).collect::<Vec<_>>().join(",")
}

fn log_request(client: u64, request: &Request) {
    let command = request.command.name();
    let level = if Command::chatty(command) { Level::Debug } else { Level::Info };
    let mut fields = vec![("client", client.to_string())];
    fields.extend(request.caller.map(|caller| ("caller", caller.to_string())));
    fields.extend(request.view.map(|view| ("view", view.to_string())));
    fields.extend(request.command.fields());
    log::write_fields(level, "control", command, &fields);
}

fn log_answered(client: u64, at: Instant, command: &'static str, reply: &Reply) {
    let ms = at.elapsed().as_millis();
    match reply {
        Ok(value) => {
            let ended = endings(value);
            let level = if Command::chatty(command) { Level::Debug } else { Level::Info };
            log::event!(level, "control", "answered", client = client, command = command, ms = ms, ended = ended);
        }
        Err(message) => {
            log::info!("control", "refused", client = client, command = command, ms = ms, error = message);
        }
    }
}

fn pane_ids(pane: u64) -> Done {
    Done { ids: Ids { pane: Some(pane), ..Ids::default() }, ..Done::default() }
}

impl App {
    pub(crate) fn request(&mut self, client: u64, text: &str, area: Option<Rect>, now: Instant) {
        let parsed = contract::parse(text, self.unavailable());
        if parsed.structured {
            self.clients.requests.structured.insert(client);
        }
        if parsed.request.is_err() {
            log::warning!("control", "unreadable request", client = client, bytes = text.len());
        }
        self.run_request(client, parsed.request, area, now);
    }

    fn unavailable(&self) -> &'static [(&'static str, &'static str)] {
        if self.update.looks_for_releases() { &[] } else { contract::NOT_LOOKING }
    }

    pub(in crate::app) fn run_request(
        &mut self,
        client: u64,
        request: Result<Request, Refusal>,
        area: Option<Rect>,
        now: Instant,
    ) {
        self.clients.requests.open.push(client);
        if let Ok(request) = &request {
            self.clients.requests.asked.insert(client, (now, request.command.name()));
            log_request(client, request);
        }
        let handled = request.and_then(|request| self.handle_request(client, request, area, now));
        self.answer_handled(client, handled);
    }

    pub(in crate::app) fn answer_handled(&mut self, client: u64, handled: Handled) {
        match handled {
            Ok(Some(value)) => self.refuse_or_answer(Some(client), Ok(value)),
            Ok(None) => {}
            Err(refusal) => self.refuse_or_answer(Some(client), Err(refusal)),
        }
    }

    pub(crate) fn take_answers(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.clients.requests.answers)
    }

    pub(crate) fn answer_lost_requests(&mut self) {
        let Requests { pending, open, .. } = &self.clients.requests;
        let lost: Vec<u64> =
            open.iter().copied().filter(|client| pending.iter().all(|p| p.client != Some(*client))).collect();
        for client in lost {
            self.answer(Some(client), Err(error::Error::Bug.to_string()));
        }
    }

    pub(crate) fn forget(&mut self, client: u64) {
        self.clients.requests.open.retain(|open| *open != client);
        self.clients.requests.structured.remove(&client);
        self.clients.requests.callers.remove(&client);
        self.orphan_todo_send(client);
        if let Some((at, command)) = self.clients.requests.asked.remove(&client) {
            log::info!(
                "control",
                "client left before the answer",
                client = client,
                command = command,
                ms = at.elapsed().as_millis()
            );
        }
        self.clients.events.forget(client);
        self.forget_change_streams(client);
        self.forget_file_streams(client);
        self.forget_surfaces(client);
        self.forget_views(client);
        self.messages.notices.forget(client);
        self.forget_flow_reads(Some(client));
        for pending in self.clients.requests.pending.iter_mut().filter(|p| p.client == Some(client)) {
            pending.client = None;
        }
        self.clients.requests.pending.retain(|p| p.client.is_some() || p.stage.outlives_its_client());
    }

    pub(in crate::app) fn answer_read(&mut self, key: u64, reply: impl FnOnce(&mut Self) -> Result<Value, Refusal>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let reply = reply(self);
        self.refuse_or_answer(pending.client, reply);
    }

    fn orphan_todo_send(&mut self, client: u64) {
        let Some(sending) = self.clients.requests.todo_sends.remove(&client) else { return };
        let orphan = self.todo_client();
        let mut kept = false;
        for pending in self.clients.requests.pending.iter_mut().filter(|p| p.client == Some(client)) {
            pending.client = Some(orphan);
            kept = true;
        }
        if kept {
            self.clients.requests.todo_sends.insert(orphan, sending);
            self.panels.todo.send(orphan, String::new());
        }
    }

    fn start_server_request(&mut self, client: u64, start: &StartServer) -> Handled {
        let size = start.size().map_err(|message| Refusal::new(Code::Malformed, message))?;
        if let Some(theme) = &start.theme {
            control::theme::check(theme).map_err(|message| Refusal::new(Code::Malformed, message))?;
            self.give_theme(Some((**theme).clone()));
        }
        self.outbox.start(client, size);
        Ok(None)
    }

    pub(crate) fn answer_start(&mut self, client: u64, started: Result<&Started, String>) {
        self.refuse_or_answer(Some(client), started.map(json).map_err(|message| Refusal::new(Code::Failed, message)));
    }

    fn answer(&mut self, client: Option<u64>, reply: Reply) {
        self.refuse_or_answer(client, reply.map_err(Refusal::from));
    }

    fn refuse_or_answer(&mut self, client: Option<u64>, reply: Result<Value, Refusal>) {
        let Some(client) = client else { return };
        let structured = self.clients.requests.structured.remove(&client);
        let reply = match (self.clients.requests.todo_sends.remove(&client), reply) {
            (Some(sending), Ok(value)) => Ok(self.todo_sent(&sending, value)),
            (_, reply) => reply,
        };
        let (code, reply) = match reply {
            Ok(value) => (Code::Failed, Ok(value)),
            Err(refusal) => (refusal.code, Err(refusal.message)),
        };
        self.flow_answered(client, &reply);
        self.clients.requests.open.retain(|open| *open != client);
        self.clients.requests.callers.remove(&client);
        if let Some((at, command)) = self.clients.requests.asked.remove(&client) {
            log_answered(client, at, command, &reply);
        }
        if self.queue_answered(client, &reply)
            || self.todo_answered(client, &reply)
            || self.tour_answered(client, &reply)
            || self.wake_answered(client, &reply)
        {
            return;
        }
        if client >= MESSENGER_CLIENTS {
            self.chat_answered(client, reply);
            return;
        }
        let reply = reply.map_err(|message| Refusal::new(code, message));
        if let Some(text) = control::contract::render(reply, structured) {
            self.clients.requests.answers.push((client, text));
        }
    }

    fn todo_sent(&mut self, sending: &Sending, mut value: Value) -> Value {
        let editing = self.panels.todo.field().filter(|field| field.item == Some(sending.todo));
        let same = self.panels.todos.item(sending.todo).is_some_and(|item| !item.done && item.text == sending.text)
            && editing.is_none_or(|field| field.editor.text() == sending.text);
        let checked = sending.confirmed && same;
        if checked {
            self.panels.todos.toggle(sending.todo);
        }
        if let Some(fields) = value.as_object_mut() {
            fields.insert("todo".into(), sending.todo.into());
            fields.insert("checked".into(), checked.into());
            if !sending.confirmed {
                fields.insert("unconfirmed".into(), true.into());
            }
        }
        value
    }

    fn handle_request(&mut self, client: u64, request: Request, area: Option<Rect>, now: Instant) -> Handled {
        if let Some(boot) = request.boot.as_deref().filter(|boot| *boot != control::server_token()) {
            return Err(stale_boot(boot));
        }
        let caller = request.caller.filter(|_| request.server.as_deref() == Some(control::server_token()));
        if let Some(caller) = caller {
            self.clients.requests.callers.insert(client, caller);
        }
        let asker = Asker { caller, view: request.view };
        self.check_view(&request)?;
        self.settle_views();
        let noted = self.flow_steps(caller, &request.command, now);
        let handled = match self.route(Asked { client, asker, area, now }, request.command) {
            ControlFlow::Break(handled) => handled,
            ControlFlow::Continue(command) => Err(malformed(format!("no area answers `{}`", command.name()))),
        };
        self.flow_requested(client, noted, handled.is_ok());
        handled
    }

    fn route(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let command = self.pane_requests(asked, command)?;
        let command = self.row_requests(asked, command)?;
        let command = self.stream_requests(asked, command)?;
        let command = self.surface_requests(asked, command)?;
        let command = self.changes_requests(asked, command)?;
        let command = self.files_requests(asked, command)?;
        let command = self.issue_requests(asked, command)?;
        let command = self.flow_requests(asked, command)?;
        let command = self.account_requests(asked, command)?;
        self.server_requests(asked, command)
    }

    fn pane_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::Open(open) => self.open_request(asker, &open, area),
            Command::NewWorkspace(new) => self.new_workspace_request(client, asker, new, area),
            Command::NewTab(new) => self.new_tab_request(asked, new),
            Command::Split(split) => self.split_request(asked, split),
            Command::Start(start) | Command::StartWith(start) => self.start_request(asked, start),
            Command::Send(send) => self.send_request(client, asker, send, now),
            Command::Keys(keys) => self.keys_request(asker, &keys, now),
            Command::Read(read) => self.read_request(asker, &read),
            Command::LastMessage(last) => self.last_message_request(client, asker, &last),
            Command::Wait(wait) => self.wait_request(client, asker, wait, now),
            Command::WaitSeveral(wait) => self.wait_several_request(client, asker, wait, now),
            Command::Mark(mark) => self.mark_request(asker, &mark),
            Command::Notify(notify) => self.notify_request(&notify.text),
            Command::Focus(focus) => self.focus_request(asker, &focus),
            other => return ControlFlow::Continue(other),
        })
    }

    fn row_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::Close(close) => self.close_request(client, &close),
            Command::Rename(rename) => self.rename_request(asker, rename),
            Command::RemovalCheck(check) => self.removal_check_request(client, &check),
            Command::CloseAgent(close) => self.close_agent_request(&close),
            Command::AgentsClose(close) => self.agents_close_request(asker, &close),
            Command::AgentsReopen(reopen) => self.agents_reopen_request(asker, &reopen, area),
            Command::AgentsForget(forget) => self.agents_forget_request(asker, &forget),
            Command::Move(moving) => self.move_request(&moving),
            Command::MovePane(moving) => self.move_pane_request(&moving, area),
            Command::Collapse(fold) => self.fold_request("collapse", fold.item, true),
            Command::Expand(fold) => self.fold_request("expand", fold.item, false),
            Command::GroupNew(new) => self.group_new_request(&new),
            Command::GroupStyle(style) => self.group_style_request(&style),
            Command::GroupDelete(delete) => self.group_delete_request(&delete),
            Command::Todo(todo) => self.todo_request(todo),
            Command::TodoSend(send) => self.todo_send_request(client, asker, &send, now),
            other => return ControlFlow::Continue(other),
        })
    }

    fn stream_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::Status(_) => Ok(Some(self.status_value(asker))),
            Command::Stats(_) => Ok(Some(json(&self.stats_view(SystemTime::now())))),
            Command::Contract(_) => Ok(Some(json(&contract::descriptor(self.unavailable())))),
            Command::Watch(_) => Ok(Some(json(&self.watch_snapshots(client, now)))),
            Command::App(_) => Ok(Some(json(&self.app_snapshots(client, now)))),
            Command::Events(events) => self.events_request(client, events),
            Command::FlowRuns(_) => Ok(Some(json(&self.flow_runs_snapshots(client, now)))),
            Command::Surface(surface) => self.surface_request(client, asker.view, &surface, now),
            Command::View(open) => self.view_request(client, &open, area),
            Command::ViewSet(set) => self.view_set_asked(asker, &set, area),
            Command::Search(search) => self.search_request(client, &search.query),
            other => return ControlFlow::Continue(other),
        })
    }

    fn surface_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, area, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::Input(input) => self.input_request(&input),
            Command::PasteFile(paste) => self.paste_file_request(client, paste),
            Command::Scroll(scroll) => self.scroll_request(&scroll),
            Command::Selection(selection) => self.selection_request(&selection),
            Command::Resize(resize) => self.resize_surface_request(&resize, now),
            Command::Redraw(redraw) => self.redraw_surface_request(&redraw, now),
            Command::Ratio(ratio) => self.ratio_request(&ratio, area),
            Command::Link(at) => self.link_request(&at),
            other => return ControlFlow::Continue(other),
        })
    }

    fn changes_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::Changes(changes) => self.changes_request(client, asker, &changes),
            Command::ChangesMode(mode) => self.changes_mode_request(client, &mode),
            Command::ChangesFile(file) => self.changes_file_request(client, asker, &file, now),
            Command::ChangesGap(gap) => self.changes_gap_request(client, asker, &gap, now),
            Command::ChangesBase(base) => self.changes_base_request(client, asker, &base),
            Command::ChangesViewed(viewed) => self.changes_viewed_request(asker, &viewed),
            Command::ChangesDiscard(discard) => self.changes_discard_request(client, asker, &discard),
            Command::ChangesStage(stage) => self.changes_stage_request(client, asker, &stage),
            Command::ChangesMergeCheck(check) => self.changes_merge_check_request(client, asker, &check),
            other => return ControlFlow::Continue(other),
        })
    }

    fn issue_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, .. } = asked;
        ControlFlow::Break(match command {
            Command::IssueSources(sources) => self.issue_sources_request(asker, &sources),
            Command::IssueList(list) => self.issue_list_request(client, asker, &list),
            Command::IssueRead(read) => self.issue_read_request(client, asker, &read),
            Command::IssuePeople(people) => self.issue_people_request(client, asker, &people),
            Command::Queue(queue) => self.queue_request(asked, queue),
            other => return ControlFlow::Continue(other),
        })
    }

    fn flow_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, .. } = asked;
        let caller = asker.caller;
        ControlFlow::Break(match command {
            Command::FlowList(list) => self.flow_list_request(caller, &list),
            Command::FlowRun(run) => self.flow_run_request(client, caller, run, area),
            Command::FlowExport(export) => self.flow_export_request(caller, &export),
            Command::FlowImport(import) => self.flow_import_request(caller, &import),
            Command::FlowSave(save) => self.flow_save_request(caller, &save),
            Command::FlowDelete(delete) => self.flow_delete_request(caller, &delete),
            Command::FlowHide(hide) => self.flow_hide_request(&hide),
            Command::FlowRestore(restore) => self.flow_restore_request(&restore),
            Command::FlowDefault(req) => self.flow_default_request(caller, &req),
            Command::FlowOrder(req) => self.flow_order_request(caller, &req),
            Command::FlowMessages(messages) => self.flow_messages_request(client, caller, &messages),
            Command::FlowFiles(files) => self.flow_files_request(client, caller, &files),
            other => return ControlFlow::Continue(other),
        })
    }

    fn account_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, area, now } = asked;
        ControlFlow::Break(match command {
            Command::AccountList(list) => Ok(self.account_list_request(client, &list)),
            Command::AccountAdd(add) => self.account_add_request(asker, &add, area, now),
            Command::AccountRemove(remove) => self.account_remove_request(&remove),
            Command::AccountRename(rename) => self.account_rename_request(&rename),
            Command::AccountUse(chosen) => self.account_use_request(&chosen),
            Command::AccountRestart(restart) => self.account_restart_request(client, &restart),
            Command::ProfileList(_) => Ok(Some(json(&self.accounts_info(None)))),
            Command::ProfileNew(new) => self.profile_new_request(&new),
            Command::ProfileSet(set) => self.profile_set_request(&set),
            Command::ProfileUse(wanted) => self.profile_use_request(&wanted),
            Command::ProfileDelete(delete) => self.profile_delete_request(&delete),
            other => return ControlFlow::Continue(other),
        })
    }

    fn server_requests(&mut self, asked: Asked, command: Command) -> ControlFlow<Handled, Command> {
        let Asked { client, asker, now, .. } = asked;
        ControlFlow::Break(match command {
            Command::Config(config) => self.config_request(client, config),
            Command::ConfigExport(_) => self.config_export_request(),
            Command::ConfigImport(import) => self.config_import_request(&import),
            Command::Theme(theme) => self.theme_request(&theme),
            Command::Go(go) => self.go_request(asker, &go),
            Command::UpdateCheck(_) => {
                self.update_check_request(client, now);
                Ok(None)
            }
            Command::UpdateInstall(_) => self.update_install_request(client),
            Command::RestartWhenIdle(restart) => self.restart_request(client, asker.caller, &restart, now),
            Command::StartServer(start) => self.start_server_request(client, &start),
            Command::Away(away) => self.away_request(away.off).map_err(Refusal::from),
            Command::Usage(usage) => Ok(self.usage_request(client, &usage, now)),
            Command::Ports(ports) => self.ports_request(client, asker, &ports),
            Command::StopPort(stop) => {
                self.stop_port_request(client, &stop);
                Ok(None)
            }
            Command::Processes(processes) => self.processes_request(client, &processes),
            Command::StopProcess(stop) => {
                self.stop_process_request(client, &stop);
                Ok(None)
            }
            Command::WorktreeSize(size) => self.worktree_size_request(client, asker, &size),
            Command::FreeBuilds(free) => self.free_builds_request(client, asker, &free),
            Command::Tour(tour) => self.tour_request(&tour),
            Command::PluginList(_) => Ok(Some(self.plugin_list_request(now))),
            Command::PluginActions(actions) => Ok(Some(self.plugin_actions_request(&actions))),
            Command::PluginRun(run) => self.plugin_run_request(client, &run),
            Command::PluginPopupClose(_) => self.plugin_popup_close_request(),
            other => return ControlFlow::Continue(other),
        })
    }

    fn status_value(&self, asker: Asker) -> Value {
        match asker.view.and_then(|view| self.view_report(view)) {
            Some(report) => json(&report),
            None => json(&self.report(asker.caller)),
        }
    }

    fn view_set_asked(&mut self, asker: Asker, set: &control::SetView, area: Option<Rect>) -> Handled {
        match asker.view {
            Some(view) => self.view_set_request(view, set, area),
            None => Err(malformed("`view-set` needs the view to change: pass --view")),
        }
    }

    fn launch_within(&mut self, client: u64, launch: Option<Launch>, waits: Waits) -> Option<Value> {
        let Some(mut launch) = launch else { return Some(json(&waits.done)) };
        let key = self.clients.requests.key();
        launch.key = Some(key);
        launch.trust = self.panels.queued.trust(client).then_some(agents::launch::Trust::Accepted);
        self.agents.launches.add(launch);
        self.clients.requests.push(client, key, waits);
        None
    }

    fn show_if(&mut self, view: Option<u64>, focus: bool, item: Item) -> Result<Option<u64>, Refusal> {
        if focus { self.show_to(view, item) } else { Ok(None) }
    }

    fn runs_agent(&self, term: &Term) -> bool {
        term.agent.status().is_some() || agents::detect(&self.config, &term.foreground_args()).is_some()
    }

    fn watched_agent(&self, term: &Term) -> Result<(), Option<String>> {
        if term.agent.status().is_some() {
            return Ok(());
        }
        match agents::detect(&self.config, &term.foreground_args()) {
            Some(agent) if agents::Known::of(&agent).is_some() => Ok(()),
            other => Err(other),
        }
    }

    pub(in crate::app) fn pend_reading(&mut self, client: u64, key: u64) {
        let done = Done::default();
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done,
            stage: Stage::Reading,
        });
    }

    fn here(&self, asker: impl Into<Asker>) -> Here {
        let asker = asker.into();
        if let Some(view) = asker.view.and_then(|id| self.clients.views.get(id)) {
            let shown = &view.shown;
            let project = shown.project.and_then(|id| self.projects.project_index(id));
            let workspace = shown.project.zip(shown.workspace).and_then(|(p, w)| self.projects.workspace_in(p, w));
            let tab =
                shown.project.zip(shown.workspace).zip(shown.tab).and_then(|((p, w), t)| self.projects.tab_in(p, w, t));
            let pane = tab.and_then(|at| self.projects.tab(at).pane());
            return Here { project, workspace, tab, pane: pane.map(|term| term.id) };
        }
        if let Some(pane) = asker.caller
            && let Some(at) = self.projects.tab_of_pane(pane)
        {
            return Here {
                project: Some(at.project()),
                workspace: Some(at.workspace()),
                tab: Some(at),
                pane: Some(pane),
            };
        }
        if self.project().is_none() {
            return Here::default();
        }
        let workspace = self.projects.active_workspace(self.active);
        let tab = workspace.and_then(|at| self.projects.active_tab(at));
        let pane = tab.and_then(|at| self.projects.tab(at).pane()).map(|t| t.id);
        Here { project: Some(self.active), workspace, tab, pane }
    }

    fn ids(&self, p: usize, workspace: Option<WorkspaceAt>, tab: Option<TabAt>) -> Ids {
        let tab = tab.map(|at| self.projects.tab(at));
        Ids {
            project: Some(self.projects[p].id),
            workspace: workspace.map(|at| self.projects.workspace(at).id),
            tab: tab.map(|t| t.id),
            pane: tab.and_then(Tab::pane).map(|t| t.id),
        }
    }

    pub(in crate::app) fn ids_of_tab(&self, at: TabAt) -> Ids {
        self.ids(at.project(), Some(at.workspace()), Some(at))
    }

    pub(in crate::app) fn shown_ids(&self, p: usize) -> Ids {
        let workspace = self.projects.active_workspace(p);
        self.ids(p, workspace, workspace.and_then(|at| self.projects.active_tab(at)))
    }

    fn here_workspace(&mut self, asker: impl Into<Asker>) -> Result<WorkspaceAt, String> {
        let here = self.here(asker);
        let p = here.project.ok_or(NO_PROJECT)?;
        Ok(here.workspace.unwrap_or_else(|| self.ensure_workspace(p)))
    }

    pub(in crate::app) fn target(
        &self,
        asker: impl Into<Asker>,
        pane: Option<u64>,
        tab: Option<u64>,
    ) -> Result<u64, Refusal> {
        if let Some(id) = pane {
            return self.projects.tab_of_pane(id).map(|_| id).ok_or_else(|| none("pane", id));
        }
        if let Some(id) = tab {
            let tab = self.projects.tab(self.projects.tab_index(id).ok_or_else(|| none("tab", id))?);
            let agent = tab.pane().into_iter().chain(&tab.panes).find(|term| self.runs_agent(term));
            return agent.or_else(|| tab.pane()).map(|t| t.id).ok_or_else(|| format!("tab {id} has no pane").into());
        }
        self.here(asker).pane.ok_or_else(|| NO_PANE.to_string().into())
    }

    pub(in crate::app) fn show(&mut self, item: Item) -> Result<(), Refusal> {
        let place = |app: &Self, p: usize, w: Option<WorkspaceAt>, t: Option<TabAt>| {
            let Ids { project, workspace, tab, .. } = app.ids(p, w, t);
            Goto::Place { project: project.unwrap_or_default(), workspace, tab }
        };
        let goto = match item {
            Item::Group(id) => {
                self.group_index(id).ok_or_else(|| none("group", id))?;
                Goto::Group(id)
            }
            Item::Project(id) => {
                let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                place(self, p, None, None)
            }
            Item::Workspace(id) => {
                let at = self.usable_workspace(id)?;
                place(self, at.project(), Some(at), None)
            }
            Item::Tab(id) => {
                let at = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
                place(self, at.project(), Some(at.workspace()), Some(at))
            }
            Item::Pane(id) => {
                let at = self.projects.tab_of_pane(id).ok_or_else(|| none("pane", id))?;
                self.projects.tab_mut(at).focus(id);
                place(self, at.project(), Some(at.workspace()), Some(at))
            }
        };
        self.goto(goto);
        Ok(())
    }

    fn check_view(&self, request: &Request) -> Result<(), Refusal> {
        let Some(view) = request.view else { return Ok(()) };
        if !control::view::takes_view(&request.command) {
            let message = format!("`{}` takes no view", request.command.name());
            return Err(Refusal::new(Code::Malformed, message));
        }
        if self.clients.views.get(view).is_none() {
            let message = format!(
                "there is no view {view}: it closed with the connection that opened it, or the server restarted"
            );
            return Err(Refusal::new(Code::NotFound, message));
        }
        Ok(())
    }

    fn focus_request(&mut self, asker: Asker, focus: &control::Focus) -> Handled {
        let reveal = focus.reveal.as_ref().map(|reveal| search::revealing(focus.item, reveal)).transpose()?;
        let projection = self.show_to(asker.view, focus.item)?;
        let revealed = reveal.map(|(pane, needle, near)| self.reveal_in(pane, &needle, near));
        let ids = asker.view.map(|view| self.view_ids(view)).unwrap_or_default();
        Ok(Some(json(&Done { ids, revealed, projection, ..Done::default() })))
    }

    pub(in crate::app) fn usable_workspace(&self, id: u64) -> Result<WorkspaceAt, Refusal> {
        let at = self.projects.workspace_index(id).ok_or_else(|| none("workspace", id))?;
        match self.projects.workspace(at).phase {
            Phase::Open => Ok(at),
            Phase::Removing(_) => Err(format!("workspace {id} is being removed").into()),
            Phase::Closing => Err(format!("workspace {id} is closing").into()),
        }
    }

    fn workspace_on(&self, p: usize, branch: &str) -> Option<u64> {
        let on = |w: &Workspace| w.open() && git::branch(&w.path).as_deref() == Some(branch);
        self.projects.find_workspace(p, on).map(|at| self.projects.workspace(at).id)
    }
}

#[cfg(test)]
mod tests;
