mod arrange;
mod close;
mod contract;
mod create;
mod flow;
mod idle_restart;
mod queue;
mod search;
mod status;
mod talk;
mod wait;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};

use ratatui::layout::Rect;
use serde::Serialize;
use serde_json::Value;

use crate::agents;
use crate::agents::launch::Launch;
use crate::app::App;
use crate::app::flow_runs::Starting;
use crate::app::messenger::CLIENTS as MESSENGER_CLIENTS;
use crate::control::contract::{Code, Refusal};
use crate::control::start_server::Started;
use crate::control::{self, Command, Done, Ids, Item, Request, StartServer};
use crate::error;
use crate::git;
use crate::log::{self, Level};
use crate::project::search::Goto;
use crate::project::{Hierarchy, Phase, Tab, Workspace};
use crate::term::Term;
use create::Then;
use wait::{Condition, Confirm, Watch};

const STARTS_WITHIN: Duration = Duration::from_secs(10);

pub(super) const CONFIRM_WITHIN: Duration = Duration::from_secs(10);

const NO_SIZE: &str = "this cornercase server has not started its session yet, so a new terminal would have no size; \
    run `cornercase start-server` (or open a window with `cornercase`) first";

const NO_PROJECT: &str = "no project is open; open one with `cornercase open PATH`";

const NO_PANE: &str = "no pane is open here; pass --pane or --tab (`cornercase status` lists them)";

type Reply = Result<Value, String>;

type Handled = Result<Option<Value>, Refusal>;

#[derive(Default)]
pub(super) struct Requests {
    pending: Vec<Pending>,
    open: Vec<u64>,
    asked: HashMap<u64, (Instant, &'static str)>,
    structured: HashSet<u64>,
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
    Run(u64, Box<Starting>),
    Removing { force: bool },
    Launch { confirm: Option<SystemTime>, wait: Option<Condition> },
    Confirm(Confirm),
    Watch(Watch),
    Several { all: bool, parts: Vec<Pending> },
    Reading,
    Setup(u64),
    Restart { caller: Option<u64> },
}

impl Stage {
    fn launch(wait: Option<Condition>) -> Self {
        Self::Launch { confirm: None, wait }
    }
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

fn none(kind: &str, id: u64) -> Refusal {
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

fn pane_ids(pane: u64) -> Done {
    Done { ids: Ids { pane: Some(pane), ..Ids::default() }, ..Done::default() }
}

impl App {
    pub(crate) fn request(&mut self, client: u64, text: &str, area: Option<Rect>, now: Instant) {
        let parsed = contract::parse(text);
        if parsed.structured {
            self.requests.structured.insert(client);
        }
        self.run_request(client, parsed.request, text.len(), area, now);
    }

    pub(in crate::app) fn run_request(
        &mut self,
        client: u64,
        request: Result<Request, Refusal>,
        bytes: usize,
        area: Option<Rect>,
        now: Instant,
    ) {
        self.requests.open.push(client);
        match &request {
            Ok(request) => {
                let command = request.command.name();
                self.requests.asked.insert(client, (now, command));
                let mut fields = vec![("client", client.to_string())];
                fields.extend(request.caller.map(|caller| ("caller", caller.to_string())));
                fields.extend(request.command.fields());
                log::write_fields(Level::Info, "control", command, &fields);
            }
            Err(_) => log::warning!("control", "unreadable request", client = client, bytes = bytes),
        }
        match request.and_then(|request| self.handle_request(client, request, area, now)) {
            Ok(Some(value)) => self.refuse_or_answer(Some(client), Ok(value)),
            Ok(None) => {}
            Err(refusal) => self.refuse_or_answer(Some(client), Err(refusal)),
        }
    }

    pub(crate) fn take_answers(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.requests.answers)
    }

    pub(crate) fn answer_lost_requests(&mut self) {
        let Requests { pending, open, .. } = &self.requests;
        let lost: Vec<u64> =
            open.iter().copied().filter(|client| pending.iter().all(|p| p.client != Some(*client))).collect();
        for client in lost {
            self.answer(Some(client), Err(error::Error::Bug.to_string()));
        }
    }

    pub(crate) fn forget(&mut self, client: u64) {
        self.requests.open.retain(|open| *open != client);
        self.requests.structured.remove(&client);
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
        self.notices.forget(client);
        self.forget_flow_reads(Some(client));
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

    fn start_server_request(&mut self, client: u64, start: &StartServer) -> Handled {
        let size = start.size().map_err(|message| Refusal::new(Code::Malformed, message))?;
        self.outbox.start = Some((client, size));
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
        let structured = self.requests.structured.remove(&client);
        let (code, reply) = match reply {
            Ok(value) => (Code::Failed, Ok(value)),
            Err(refusal) => (refusal.code, Err(refusal.message)),
        };
        self.flow_answered(client, &reply);
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
        if self.queue_answered(client, &reply) {
            return;
        }
        if client >= MESSENGER_CLIENTS {
            self.chat_answered(client, reply);
            return;
        }
        let reply = reply.map_err(|message| Refusal::new(code, message));
        if let Some(text) = control::contract::render(reply, structured) {
            self.requests.answers.push((client, text));
        }
    }

    fn handle_request(&mut self, client: u64, request: Request, area: Option<Rect>, now: Instant) -> Handled {
        if let Some(boot) = request.boot.as_deref().filter(|boot| *boot != control::server_token()) {
            return Err(stale_boot(boot));
        }
        let caller = request.caller.filter(|_| request.server.as_deref() == Some(control::server_token()));
        let noted = self.flow_steps(caller, &request.command, now);
        let handled = match request.command {
            Command::Status(_) => Ok(Some(json(&self.report(caller)))),
            Command::Stats(_) => Ok(Some(json(&self.stats_view(SystemTime::now())))),
            Command::Contract(_) => Ok(Some(json(&contract::descriptor()))),
            Command::StartServer(start) => self.start_server_request(client, &start),
            Command::Open(open) => self.open_request(&open, area),
            Command::NewWorkspace(new) => self.new_workspace_request(client, caller, new, area),
            Command::NewTab(new) => self.new_tab_request(client, caller, new, area, now),
            Command::Split(split) => self.split_request(client, caller, split, area, now),
            Command::Start(start) | Command::StartWith(start) => self.start_request(client, caller, start, area, now),
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
            Command::Queue(queue) => self.queue_request(client, caller, queue, area, now),
            Command::Events(events) => self.events_request(client, events),
            Command::Watch(_) => Ok(Some(json(&self.watch_snapshots(client, now)))),
            Command::LastMessage(last) => self.last_message_request(client, caller, &last),
            Command::RestartWhenIdle(restart) => self.restart_request(client, caller, &restart, now),
            Command::Away(away) => self.away_request(away.off).map_err(Refusal::from),
            Command::Mark(mark) => self.mark_request(caller, &mark),
            Command::FlowList(list) => self.flow_list_request(caller, &list),
            Command::FlowRun(run) => self.flow_run_request(client, caller, run, area),
            Command::FlowExport(export) => self.flow_export_request(caller, &export),
            Command::FlowImport(import) => self.flow_import_request(caller, &import),
            Command::Search(search) => self.search_request(client, &search.query),
        };
        self.flow_requested(client, noted, handled.is_ok());
        handled
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
        launch.trust = self.queued.running.contains_key(&client).then_some(agents::launch::Trust::Accepted);
        self.agents.launches.push(launch);
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done, stage });
        None
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

    fn here(&self, caller: Option<u64>) -> Here {
        if let Some(pane) = caller
            && let Some((p, w, t)) = self.projects.tab_of_pane(pane)
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

    pub(in crate::app) fn target(
        &self,
        caller: Option<u64>,
        pane: Option<u64>,
        tab: Option<u64>,
    ) -> Result<u64, Refusal> {
        if let Some(id) = pane {
            return self.projects.tab_of_pane(id).map(|_| id).ok_or_else(|| none("pane", id));
        }
        if let Some(id) = tab {
            let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
            let tab = &self.projects[p].workspaces[w].tabs[t];
            let agent = std::iter::once(tab.active)
                .chain(0..tab.panes.len())
                .filter_map(|i| tab.panes.get(i))
                .find(|term| self.runs_agent(term));
            return agent.or_else(|| tab.pane()).map(|t| t.id).ok_or_else(|| format!("tab {id} has no pane").into());
        }
        self.here(caller).pane.ok_or_else(|| NO_PANE.to_string().into())
    }

    fn show(&mut self, item: Item) -> Result<(), Refusal> {
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
                let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                place(self, p, None, None)
            }
            Item::Workspace(id) => {
                let (p, w) = self.usable_workspace(id)?;
                place(self, p, Some(w), None)
            }
            Item::Tab(id) => {
                let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
                place(self, p, Some(w), Some(t))
            }
            Item::Pane(id) => {
                let (p, w, t) = self.projects.tab_of_pane(id).ok_or_else(|| none("pane", id))?;
                self.projects[p].workspaces[w].tabs[t].focus(id);
                place(self, p, Some(w), Some(t))
            }
        };
        self.goto(goto);
        Ok(())
    }

    fn usable_workspace(&self, id: u64) -> Result<(usize, usize), Refusal> {
        let (p, w) = self.projects.workspace_index(id).ok_or_else(|| none("workspace", id))?;
        match self.projects[p].workspaces[w].phase {
            Phase::Open => Ok((p, w)),
            Phase::Removing(_) => Err(format!("workspace {id} is being removed").into()),
            Phase::Closing => Err(format!("workspace {id} is closing").into()),
        }
    }

    fn workspace_on(&self, p: usize, branch: &str) -> Option<u64> {
        let workspaces = self.projects[p].workspaces.iter().filter(|w| w.open());
        workspaces.into_iter().find(|w| git::branch(&w.path).as_deref() == Some(branch)).map(|w| w.id)
    }
}

#[cfg(test)]
mod tests;
