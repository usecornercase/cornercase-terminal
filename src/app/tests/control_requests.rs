use serde_json::{Value, json};

use super::*;
#[cfg(unix)]
use crate::agents::claude::fake::FakeClaude;
use crate::app::messenger::{self as wiring, MessengerEvent};
#[cfg(unix)]
use crate::control::Until;
use crate::control::{self as wire, Command, Done, Response};
#[cfg(unix)]
use crate::control::{Item, Report};
use crate::messenger::link::Event as LinkEvent;
use crate::messenger::{Chat, Message, Order, Update, Via};
#[cfg(unix)]
use crate::messenger::{Keyboard, Press};
use crate::project::Hierarchy;
#[cfg(unix)]
use crate::test_util::write_executable;

#[cfg(unix)]
fn wait_after_typing(app: &mut App, rx: &Receiver<AppEvent>) -> Instant {
    let id = done(answered(app, rx, "the command is typed")).ids.pane.expect("the pane");
    let typed = Instant::now();
    ask(app, None, wait_for(id, Until::Shell, None));
    typed
}

const CLIENT: u64 = 9;

fn request(app: &mut App, caller: Option<u64>, command: Command, area: Option<Rect>) {
    let server = caller.map(|_| wire::server_token().to_string());
    let text =
        serde_json::to_string(&wire::Request { caller, server, contract: None, boot: None, view: None, command })
            .expect("json");
    app.request(CLIENT, &text, area, Instant::now());
}

fn ask(app: &mut App, caller: Option<u64>, command: Command) {
    request(app, caller, command, Some(AREA));
}

#[derive(Debug)]
struct Refused {
    code: String,
    message: String,
}

fn call(app: &mut App, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    call_as(app, command, args, None)
}

fn call_as(app: &mut App, command: &str, args: &Value, boot: Option<&str>) -> std::result::Result<Value, Refused> {
    call_headed(app, &json!({"boot": boot}), command, args)
}

fn call_in_view(app: &mut App, view: u64, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    call_headed(app, &json!({"view": view}), command, args)
}

fn call_headed(app: &mut App, head: &Value, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    let mut envelope = json!({"contract": 1, "command": command, "args": args});
    envelope.as_object_mut().expect("an envelope").extend(head.as_object().expect("a head").clone());
    let text = envelope.to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    reply_of(&answers.remove(0).1)
}

const VIEWER: u64 = 31;

struct Apart {
    app: App,
    _rx: Receiver<AppEvent>,
    folders: [TempDir; 3],
}

fn apart() -> Apart {
    let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
    let folders = [TempDir::new(), TempDir::new(), TempDir::new()];
    let [config, viewed, windowed] = &folders;
    let (mut app, rx) = new_app_with_shell(shell, config.path().join("config.json"));
    app.open_project(viewed.path().to_path_buf(), AREA).expect("the project a view shows");
    app.open_project(windowed.path().to_path_buf(), AREA).expect("the project the window shows");
    Apart { app, _rx: rx, folders }
}

impl Apart {
    fn views_the_first_project(&mut self) -> u64 {
        let project = self.app.projects[0].id;
        open_view(&mut self.app, VIEWER, &json!({"cols": 100, "rows": 30, "project": project}))
    }
}

fn reply_of(text: &str) -> std::result::Result<Value, Refused> {
    let mut reply: Value = serde_json::from_str(text).expect("a reply");
    match reply["error"].take() {
        Value::Null => Ok(reply["ok"].take()),
        error => Err(Refused {
            code: error["code"].as_str().unwrap_or_default().to_string(),
            message: error["message"].as_str().unwrap_or_default().to_string(),
        }),
    }
}

#[cfg(unix)]
fn send_waiting(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    caller: Option<u64>,
    command: &str,
    args: &Value,
) -> Vec<(u64, String)> {
    let server = caller.map(|_| wire::server_token());
    send_headed(app, rx, &json!({"caller": caller, "server": server}), command, args)
}

#[cfg(unix)]
fn send_headed(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    head: &Value,
    command: &str,
    args: &Value,
) -> Vec<(u64, String)> {
    let mut text = head.clone();
    text["contract"] = json!(1);
    text["command"] = json!(command);
    text["args"] = args.clone();
    app.request(CLIENT, &text.to_string(), Some(AREA), Instant::now());
    let mut got = Vec::new();
    wait_until("the answer", || {
        drain(app, rx);
        app.refresh(Instant::now());
        got.extend(app.take_answers());
        !got.is_empty()
    });
    got
}

#[cfg(unix)]
fn asked(app: &mut App, rx: &Receiver<AppEvent>, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    asked_by(app, rx, None, command, args)
}

#[cfg(unix)]
fn asked_by(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    caller: Option<u64>,
    command: &str,
    args: &Value,
) -> std::result::Result<Value, Refused> {
    let mut answers = send_waiting(app, rx, caller, command, args);
    assert_eq!(answers.len(), 1);
    reply_of(&answers.remove(0).1)
}

fn ok(app: &mut App, command: &str, args: &Value) -> Value {
    call(app, command, args).unwrap_or_else(|e| panic!("{command} refused: {} {}", e.code, e.message))
}

fn refused(app: &mut App, command: &str, args: &Value) -> Refused {
    match call(app, command, args) {
        Ok(value) => panic!("{command} was taken: {value}"),
        Err(refused) => refused,
    }
}

fn call_later(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    command: &str,
    args: &Value,
) -> std::result::Result<Value, Refused> {
    request_json(app, command, args);
    reply_later(app, rx)
}

fn request_json(app: &mut App, command: &str, args: &Value) {
    let text = json!({"contract": 1, "command": command, "args": args}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
}

fn open_view(app: &mut App, client: u64, args: &Value) -> u64 {
    let text = json!({"command": "view", "args": args}).to_string();
    app.request(client, &text, Some(AREA), Instant::now());
    let answers = app.take_answers();
    let (_, reply) = answers.into_iter().find(|(who, _)| *who == client).expect("an answer");
    let opened: Value = serde_json::from_str(&reply).expect("a reply");
    opened["ok"]["view"]["id"].as_u64().expect("a view")
}

fn reply_later(app: &mut App, rx: &Receiver<AppEvent>) -> std::result::Result<Value, Refused> {
    let mut reply = None;
    wait_until("the request is answered", || {
        drain(app, rx);
        app.refresh(Instant::now());
        reply = app.take_answers().into_iter().next().map(|(_, text)| text);
        reply.is_some()
    });
    reply_of(&reply.expect("an answer"))
}

fn answers(app: &mut App) -> Vec<Response> {
    let answers = app.take_answers();
    assert!(answers.iter().all(|(client, _)| *client == CLIENT), "{answers:?}");
    answers.into_iter().map(|(_, text)| serde_json::from_str(&text).expect("a response")).collect()
}

#[cfg(unix)]
fn answered(app: &mut App, rx: &Receiver<AppEvent>, what: &str) -> Response {
    let mut got = Vec::new();
    wait_until(what, || {
        drain(app, rx);
        app.refresh(Instant::now());
        got = answers(app);
        !got.is_empty()
    });
    got.remove(0)
}

fn done(response: Response) -> Done {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("an answer"),
        Response::Error(message) => panic!("the request failed: {message}"),
    }
}

fn error(response: Response) -> String {
    match response {
        Response::Error(message) => message,
        Response::Ok(value) => panic!("the request did not fail: {value}"),
    }
}

fn now(app: &mut App, caller: Option<u64>, command: Command) -> Response {
    ask(app, caller, command);
    let mut answers = answers(app);
    assert_eq!(answers.len(), 1, "an answer at once");
    answers.remove(0)
}

fn value(app: &mut App, caller: Option<u64>, command: Command) -> Value {
    match now(app, caller, command) {
        Response::Ok(value) => value,
        Response::Error(message) => panic!("the request failed: {message}"),
    }
}

fn pane(app: &App, id: u64) -> &Term {
    app.projects.pane_by(id).expect("the pane")
}

#[cfg(unix)]
struct Agent {
    app: App,
    rx: Receiver<AppEvent>,
    fake: FakeClaude,
    config_dir: TempDir,
    _project: TempDir,
}

#[cfg(unix)]
impl Agent {
    fn new() -> Self {
        let (config, project) = (TempDir::new(), TempDir::new());
        let fake = FakeClaude::running(FakeClaude::CONTROL);
        let settings = Config {
            agent_commands: [("claude".to_string(), fake.script().display().to_string())].into(),
            agent_args: [("claude".to_string(), vec![fake.dir().display().to_string()])].into(),
            ..Config::default()
        };
        let config_path = write_config(config.path(), &settings);
        let (mut app, rx) = app_in(project.path(), config_path);
        app.agents.set_claude_dir(fake.dir());
        Self { app, rx, fake, config_dir: config, _project: project }
    }

    fn start(&mut self, prompt: Option<&str>) -> u64 {
        let start = wire::Start {
            agent: Some("claude".into()),
            name: Some("fixer".into()),
            prompt: prompt.map(str::to_string),
            ..wire::Start::default()
        };
        ask(&mut self.app, None, Command::Start(start));
        let started = done(answered(&mut self.app, &self.rx, "the agent is ready"));
        started.ids.pane.expect("the agent's pane")
    }

    fn finish(&self) {
        self.fake.signal("finish");
    }

    fn until(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        let (app, rx) = (&mut self.app, &self.rx);
        pump_until(app, rx, what, |a| cond(a));
    }

    fn refreshing(&mut self, what: &str, cond: impl Fn(&App) -> bool) {
        refreshing(&mut self.app, &self.rx, what, cond);
    }

    fn status(&self, id: u64) -> Option<activity::Status> {
        pane(&self.app, id).agent.status()
    }

    fn answered(&mut self, what: &str) -> Response {
        answered(&mut self.app, &self.rx, what)
    }
}

#[cfg(unix)]
impl Agent {
    fn transcript(&mut self, id: u64, lines: &[&str]) {
        let (app, rx) = (&mut self.app, &self.rx);
        refreshing(app, rx, "the pane knows its conversation", |a| {
            pane(a, id).context.record_for(crate::agents::Known::Claude).is_some()
        });
        let Some(context::Record::Claude(path)) = pane(&self.app, id).context.record_for(crate::agents::Known::Claude)
        else {
            panic!("not a transcript")
        };
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("open the transcript");
        let text: String = lines.iter().flat_map(|line| [*line, "\n"]).collect();
        std::io::Write::write_all(&mut file, text.as_bytes()).expect("write the transcript");
    }
}

#[cfg(unix)]
#[test]
fn a_send_leaves_a_scrolled_pane_where_it_is() {
    let (mut app, rx) = app();
    let id = app.term_mut().expect("a pane").id;
    numbered(app.term_mut().expect("a pane"), 100);
    app.term_mut().expect("a pane").emulator.scroll(-3);

    ask(
        &mut app,
        None,
        Command::Send(wire::SendText { pane: Some(id), text: Some("x".into()), ..wire::SendText::default() }),
    );
    done(answered(&mut app, &rx, "the send is answered"));

    assert!(pane(&app, id).emulator.below() >= 3, "the view moved: {}", pane(&app, id).emulator.below());
}

#[cfg(unix)]
fn pane_screen(app: &App, id: u64) -> String {
    pane(app, id).emulator.screen_text().expect("the screen")
}

#[cfg(unix)]
fn first(app: &App) -> u64 {
    term(app, app.active).id
}

#[cfg(unix)]
fn new_tab(command: Option<&str>) -> Command {
    Command::NewTab(wire::NewTab { command: command.map(str::to_string), ..wire::NewTab::default() })
}

#[cfg(unix)]
fn wait_for(pane: u64, until: Until, timeout: Option<f64>) -> Command {
    Command::Wait(wire::Wait { pane: Some(pane), until, timeout, ..wire::Wait::default() })
}

#[cfg(unix)]
fn wait_on(panes: &[u64], tabs: &[u64], all: bool, until: Until, timeout: Option<f64>) -> Command {
    let (panes, tabs) = (panes.to_vec(), tabs.to_vec());
    Command::WaitSeveral(wire::Wait { panes, tabs, all, until, timeout, ..wire::Wait::default() })
}

#[cfg(unix)]
fn send_text(pane: u64, text: &str, enter: bool, wait: bool) -> Command {
    let text = Some(text.to_string()).filter(|t| !t.is_empty());
    Command::Send(wire::SendText { pane: Some(pane), text, enter, wait, ..wire::SendText::default() })
}

#[cfg(unix)]
fn send_enter(app: &mut App, text: &str) {
    let pane = first(app);
    ask(app, None, send_text(pane, text, true, false));
}

#[cfg(unix)]
fn last_message(app: &mut App, rx: &Receiver<AppEvent>, pane: u64) -> Value {
    let mut message = None;
    wait_until("the conversation is found and its last message read", || {
        let wanted = wire::LastMessage { pane: Some(pane), ..wire::LastMessage::default() };
        ask(app, None, Command::LastMessage(wanted));
        match answered(app, rx, "the last message is read") {
            Response::Ok(value) => message = Some(value),
            Response::Error(error) if error.contains("has not found where") => {}
            Response::Error(error) => panic!("last message failed: {error}"),
        }
        message.is_some()
    });
    message.expect("the last message")
}

#[cfg(unix)]
fn a_local_command_needs_no_record(app: &mut App, rx: &Receiver<AppEvent>, text: &str) {
    send_enter(app, text);
    done(answered(app, rx, "the local command is submitted"));
    assert_eq!(app.term().and_then(|t| t.context.prompted()), None);
}

#[cfg(unix)]
fn an_unconfirmed_send_gives_up(app: &mut App, rx: &Receiver<AppEvent>) {
    app.clients.set_confirm_within(Duration::from_millis(300));
    send_enter(app, "fix the login");
    let message = error(answered(app, rx, "the send gives up"));
    assert!(message.contains("not confirmed"), "{message}");
}

#[cfg(unix)]
fn a_waiting_agent_refuses_a_send(app: &mut App) {
    send_enter(app, "continue");
    assert!(error(answers(app).remove(0)).contains("waiting for an answer"));
}

#[cfg(unix)]
fn press_keys(pane: u64, keys: &[&str]) -> Command {
    Command::Keys(wire::Keys { pane: Some(pane), tab: None, keys: keys.iter().map(ToString::to_string).collect() })
}

#[cfg(unix)]
fn follow(app: &mut App, panes: &[u64]) {
    follow_with(app, panes, false);
}

#[cfg(unix)]
fn follow_with(app: &mut App, panes: &[u64], host: bool) {
    let ack = done(now(app, None, Command::Events(wire::Events { panes: panes.to_vec(), host, notices: false })));
    assert_eq!(ack, Done::default());
}

#[cfg(unix)]
fn streamed(app: &mut App) -> Vec<Streamed> {
    app.trace();
    let streamed = app.take_events();
    assert!(streamed.iter().all(|(client, _)| *client == CLIENT), "{streamed:?}");
    streamed.into_iter().map(|(_, streamed)| streamed).collect()
}

#[cfg(unix)]
fn lines(app: &mut App) -> Vec<Value> {
    streamed(app)
        .into_iter()
        .filter_map(|streamed| match streamed {
            Streamed::Snapshot(text) => match serde_json::from_str(&text) {
                Ok(Response::Ok(value)) => Some(value),
                other => panic!("not a snapshot: {other:?}"),
            },
            Streamed::Event(_) | Streamed::End => None,
        })
        .collect()
}

#[cfg(unix)]
fn until_told(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    what: &str,
    enough: impl Fn(&[Streamed]) -> bool,
) -> Vec<Streamed> {
    let mut got = Vec::new();
    wait_until(what, || {
        drain(app, rx);
        app.refresh(Instant::now());
        got.extend(streamed(app));
        enough(&got)
    });
    got
}

#[cfg(unix)]
fn statuses(streamed: &[Streamed], pane: u64) -> Vec<(Option<String>, Option<String>)> {
    told(streamed)
        .into_iter()
        .filter_map(|(what, ids)| match what {
            wire::What::Status { from, to, .. } if ids.pane == Some(pane) => Some((from, to)),
            _ => None,
        })
        .collect()
}

#[cfg(unix)]
fn change(from: Option<&str>, to: Option<&str>) -> (Option<String>, Option<String>) {
    (from.map(str::to_string), to.map(str::to_string))
}

const CHAT: i64 = 42;
const USER: i64 = 9;

struct Phone {
    via: Via,
    orders: Receiver<Order>,
    next: i64,
    seen: Vec<(i64, Order)>,
}

fn chat() -> Chat {
    Chat { id: CHAT, user: USER, name: "@ada".into(), paired: 1 }
}

#[cfg(unix)]
fn html(order: &Order) -> &str {
    match order {
        Order::Send { text, .. } | Order::Edit { text, .. } => text,
        _ => "",
    }
}

#[cfg(unix)]
fn keyboard(order: &Order) -> &Keyboard {
    static NONE: Keyboard = Vec::new();
    match order {
        Order::Send { keyboard, .. } | Order::Edit { keyboard, .. } => keyboard,
        _ => &NONE,
    }
}

#[cfg(unix)]
fn button(order: &Order, text: &str) -> String {
    let key = keyboard(order).iter().flatten().find(|key| key.text.contains(text));
    key.unwrap_or_else(|| panic!("no `{text}` button in {order:?}")).data.clone()
}

fn update(app: &mut App, via: Via, update: Update) {
    let event = AppEvent::Messenger(MessengerEvent::Link(via, LinkEvent::Updates(1, vec![update])));
    app.handle_event(event, AREA).expect("the update is handled");
}

impl Phone {
    fn on(app: &mut App, via: Via, chat: Option<Chat>) -> Self {
        wiring::set_up(app, via, "1:t", "cornercase_bot", chat);
        Self { via, orders: wiring::capture(app, via), next: 1000, seen: Vec::new() }
    }

    fn set_up(app: &mut App, chat: Option<Chat>) -> Self {
        Self::on(app, Via::Telegram, chat)
    }

    fn paired(app: &mut App) -> Self {
        Self::set_up(app, Some(chat()))
    }

    fn read(&mut self, app: &mut App) -> Vec<(i64, Order)> {
        let mut got = Vec::new();
        while let Ok(order) = self.orders.try_recv() {
            self.next += 1;
            if let Some(tag) = order.tag() {
                let sent = LinkEvent::Sent { link: 1, tag, message: self.next };
                app.handle_event(AppEvent::Messenger(MessengerEvent::Link(self.via, sent)), AREA).expect("sent");
            }
            got.push((self.next, order));
        }
        self.seen.extend(got.iter().cloned());
        got
    }

    fn message(&mut self, app: &mut App, text: &str, reply_to: Option<i64>, from: (i64, i64)) -> i64 {
        self.next += 1;
        let (chat, user) = from;
        let message = Message {
            id: self.next,
            chat,
            user,
            private: true,
            guild: None,
            date: 0,
            name: "@ada".into(),
            text: text.into(),
            reply_to,
        };
        update(app, self.via, Update::Message(message));
        self.next
    }

    fn says(&mut self, app: &mut App, text: &str) -> i64 {
        self.message(app, text, None, (CHAT, USER))
    }

    #[cfg(unix)]
    fn replies(&mut self, app: &mut App, to: i64, text: &str) -> i64 {
        self.message(app, text, Some(to), (CHAT, USER))
    }

    #[cfg(unix)]
    fn taps(&self, app: &mut App, message: i64, data: &str) {
        let press = Press { id: format!("q{message}"), chat: CHAT, user: USER, message, data: data.into() };
        update(app, self.via, Update::Press(press));
    }

    #[cfg(unix)]
    fn waits(&mut self, agent: &mut Agent, what: &str, pick: impl Fn(&Order) -> bool) -> (i64, Order) {
        let mut found = None;
        wait_until(what, || {
            drain(&mut agent.app, &agent.rx);
            agent.app.refresh(Instant::now());
            found = self.read(&mut agent.app).into_iter().find(|(_, order)| pick(order));
            found.is_some()
        });
        found.expect("an order")
    }

    #[cfg(unix)]
    fn says_in_html(&mut self, agent: &mut Agent, what: &str, text: &str) -> (i64, Order) {
        self.waits(agent, what, |order| html(order).contains(text))
    }
}

#[cfg(unix)]
fn reacted(message: i64) -> impl Fn(&Order) -> bool {
    move |order| matches!(order, Order::React { message: m, .. } if *m == message)
}

#[cfg(unix)]
mod account_home;
mod account_login;
mod accounts;
mod agent_models;
mod agents;
mod arranging;
mod build_folders;
mod changes;
#[cfg(unix)]
mod cloning;
mod close_agent;
mod closed_agents;
mod code_workspace;
mod compacting;
mod config;
mod config_share;
mod context_read;
mod contract;
mod copilot;
mod creating;
mod cursor;
mod discord;
mod discord_forum;
mod driving;
mod events;
#[cfg(unix)]
mod files;
mod flow_in_a_view;
mod flow_sharing;
#[cfg(unix)]
mod flows;
mod following_the_app;
mod fresh_branches;
mod gemini;
mod grok;
mod groups;
mod held;
mod holding;
mod issues;
mod journal;
mod merge_check;
mod messages;
mod one_folder;
mod ordering;
mod pair;
mod pane_conversation;
mod pane_moves;
mod parents;
#[cfg(unix)]
mod pasting;
#[cfg(unix)]
mod placed;
mod plugin_views;
mod plugins;
mod ports;
mod processes;
mod pull_request;
mod queue;
mod queue_show;
mod recap;
#[cfg(unix)]
mod repos;
mod run_setup;
mod searching;
mod setup;
mod start_server;
mod stats_cached;
mod subagents;
mod surfaces;
mod talking;
mod telegram;
mod theme;
mod todo_sending;
mod todos;
mod tours;
mod tunnels;
mod updating;
mod usage;
mod views;
mod watching;
mod worktree_list;
mod worktrees;
mod worktrees_remove;
