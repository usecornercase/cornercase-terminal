use serde_json::Value;

use super::*;
use crate::control::{self as wire, Command, Done, Item, Report, Response, Until};
use crate::test_util::{FakeClaude, write_executable};

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
    app.projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .flat_map(Workspace::terms)
        .find(|t| t.id == id)
        .expect("the pane")
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
    Command::Keys(wire::Keys {
        pane: Some(pane),
        tab: None,
        keys: keys.iter().map(ToString::to_string).collect(),
    })
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

mod creating;
mod talking;
mod gemini;
mod agents;
mod events;
mod arranging;
mod worktrees;
