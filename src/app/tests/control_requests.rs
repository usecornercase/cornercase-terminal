use serde_json::Value;

use super::*;
use crate::agents::claude::fake::FakeClaude;
use crate::app::messenger::{self as wiring, MessengerEvent};
use crate::control::{self as wire, Command, Done, Item, Report, Response, Until};
use crate::messenger::link::Event as LinkEvent;
use crate::messenger::{Chat, Keyboard, Message, Order, Press, Update, Via};
use crate::test_util::write_executable;

fn wait_after_typing(app: &mut App, rx: &Receiver<AppEvent>) -> Instant {
    let id = done(answered(app, rx, "the command is typed")).ids.pane.expect("the pane");
    let typed = Instant::now();
    ask(app, None, wait_for(id, Until::Shell, None));
    typed
}

const CLIENT: u64 = 9;

fn request(app: &mut App, caller: Option<u64>, command: Command, area: Option<Rect>) {
    let server = caller.map(|_| wire::server_token().to_string());
    let text = serde_json::to_string(&wire::Request { caller, server, command }).expect("json");
    app.request(CLIENT, &text, area, Instant::now());
}

fn ask(app: &mut App, caller: Option<u64>, command: Command) {
    request(app, caller, command, Some(AREA));
}

fn answers(app: &mut App) -> Vec<Response> {
    let answers = app.take_answers();
    assert!(answers.iter().all(|(client, _)| *client == CLIENT), "{answers:?}");
    answers.into_iter().map(|(_, text)| serde_json::from_str(&text).expect("a response")).collect()
}

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
    app.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms).find(|t| t.id == id).expect("the pane")
}

struct Agent {
    app: App,
    rx: Receiver<AppEvent>,
    fake: FakeClaude,
    config_dir: TempDir,
    _project: TempDir,
}

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
        app.agents.claude_dir = Some(fake.dir().to_path_buf());
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

fn pane_screen(app: &App, id: u64) -> String {
    pane(app, id).emulator.screen_text().expect("the screen")
}

fn first(app: &App) -> u64 {
    term(app, app.active).id
}

fn new_tab(command: Option<&str>) -> Command {
    Command::NewTab(wire::NewTab { command: command.map(str::to_string), ..wire::NewTab::default() })
}

fn wait_for(pane: u64, until: Until, timeout: Option<f64>) -> Command {
    Command::Wait(wire::Wait { pane: Some(pane), until, timeout, ..wire::Wait::default() })
}

fn wait_on(panes: &[u64], tabs: &[u64], all: bool, until: Until, timeout: Option<f64>) -> Command {
    let (panes, tabs) = (panes.to_vec(), tabs.to_vec());
    Command::WaitSeveral(wire::Wait { panes, tabs, all, until, timeout, ..wire::Wait::default() })
}

fn send_text(pane: u64, text: &str, enter: bool, wait: bool) -> Command {
    let text = Some(text.to_string()).filter(|t| !t.is_empty());
    Command::Send(wire::SendText { pane: Some(pane), text, enter, wait, ..wire::SendText::default() })
}

fn press_keys(pane: u64, keys: &[&str]) -> Command {
    Command::Keys(wire::Keys { pane: Some(pane), tab: None, keys: keys.iter().map(ToString::to_string).collect() })
}

fn follow(app: &mut App, panes: &[u64]) {
    let ack = done(now(app, None, Command::Events(wire::Events { panes: panes.to_vec() })));
    assert_eq!(ack, Done::default());
}

fn streamed(app: &mut App) -> Vec<Streamed> {
    app.trace();
    let streamed = app.take_events();
    assert!(streamed.iter().all(|(client, _)| *client == CLIENT), "{streamed:?}");
    streamed.into_iter().map(|(_, streamed)| streamed).collect()
}

fn told(streamed: &[Streamed]) -> Vec<(wire::What, wire::Ids)> {
    streamed
        .iter()
        .filter_map(|streamed| match streamed {
            Streamed::Event(text) => {
                let event: wire::Event = serde_json::from_value(match serde_json::from_str(text) {
                    Ok(Response::Ok(value)) => value,
                    other => panic!("not an event: {other:?}"),
                })
                .expect("an event");
                Some((event.what, event.ids))
            }
            Streamed::End => None,
        })
        .collect()
}

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

fn statuses(streamed: &[Streamed], pane: u64) -> Vec<(Option<String>, Option<String>)> {
    told(streamed)
        .into_iter()
        .filter_map(|(what, ids)| match what {
            wire::What::Status { from, to, .. } if ids.pane == Some(pane) => Some((from, to)),
            _ => None,
        })
        .collect()
}

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

fn html(order: &Order) -> &str {
    match order {
        Order::Send { text, .. } | Order::Edit { text, .. } => text,
        _ => "",
    }
}

fn keyboard(order: &Order) -> &Keyboard {
    static NONE: Keyboard = Vec::new();
    match order {
        Order::Send { keyboard, .. } | Order::Edit { keyboard, .. } => keyboard,
        _ => &NONE,
    }
}

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

    fn replies(&mut self, app: &mut App, to: i64, text: &str) -> i64 {
        self.message(app, text, Some(to), (CHAT, USER))
    }

    fn taps(&self, app: &mut App, message: i64, data: &str) {
        let press = Press { id: format!("q{message}"), chat: CHAT, user: USER, message, data: data.into() };
        update(app, self.via, Update::Press(press));
    }

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

    fn says_in_html(&mut self, agent: &mut Agent, what: &str, text: &str) -> (i64, Order) {
        self.waits(agent, what, |order| html(order).contains(text))
    }
}

fn reacted(message: i64) -> impl Fn(&Order) -> bool {
    move |order| matches!(order, Order::React { message: m, .. } if *m == message)
}

mod agents;
mod arranging;
mod creating;
mod discord;
mod events;
mod flow_sharing;
mod flows;
mod gemini;
mod grok;
mod one_folder;
mod searching;
mod talking;
mod telegram;
mod worktrees;
