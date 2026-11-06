use std::process::Stdio;

use serde_json::{Value, json};

use super::commands::Followed;
use super::*;

const TYPED: &str = r#"{"text": "half written"}"#;

fn shell_pane(session: &Session) -> String {
    session.cold(&["start-server"]);
    let pane = session.says(&["new-tab"]);
    wait_until("the shell's prompt", || session.says(&["read", "--pane", &pane]).contains('$'));
    pane
}

fn id(pane: &str) -> u64 {
    pane.parse().expect("a pane's id")
}

fn parsed(text: &str) -> Value {
    serde_json::from_str(text).expect("a JSON answer")
}

fn items<'a>(value: &'a Value, key: &'static str) -> impl Iterator<Item = &'a Value> {
    value[key].as_array().into_iter().flatten()
}

fn pane_in(state: &Value, pane: &str) -> Value {
    let workspaces = items(state, "projects").flat_map(|project| items(project, "workspaces"));
    let tabs = workspaces.flat_map(|workspace| items(workspace, "tabs"));
    let mut panes = tabs.flat_map(|tab| items(tab, "panes"));
    panes.find(|found| found["id"].as_u64() == Some(id(pane))).cloned().unwrap_or_default()
}

fn pane_of(session: &Session, pane: &str) -> Value {
    pane_in(&parsed(&session.says(&["status", "--json"])), pane)
}

fn held_in(session: &Session, pane: &str) {
    wait_until("the send is held", || pane_of(session, pane)["held_sends"] == 1);
}

fn typed_and_held(session: &Session, pane: &str) -> std::process::Child {
    session.says(&["input", "--pane", pane, TYPED]);
    let send = session.spawn(&["send", "--pane", pane, "--enter", "from a script"]);
    held_in(session, pane);
    send
}

fn column(table: &str, pane: &str, name: &str) -> String {
    let mut lines = table.lines();
    let names: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    let at = names.iter().position(|column| *column == name).expect("the column");
    let row = lines.find(|row| row.split('\t').next() == Some(pane)).expect("the pane's row");
    row.split('\t').nth(at).expect("the cell").to_string()
}

#[test]
fn send_now_sends_the_message_after_the_persons_half_written_line() {
    let session = Session::new();
    let pane = shell_pane(&session);
    let send = typed_and_held(&session, &pane);
    let table = session.says(&["status", "--panes"]);
    let waiting = pane_of(&session, &pane);

    let said = parsed(&session.says(&["held", "--pane", &pane, "--send-now", "--json"]));

    assert_eq!((column(&table, &pane, "held_sends").as_str(), column(&table, &pane, "draft").as_str()), ("1", "true"));
    assert_eq!(
        (&waiting["draft"], &waiting["held_from"], &waiting["held_queued"]),
        (&json!(true), &json!(null), &json!(false))
    );
    assert_eq!(said, json!({"pane": id(&pane), "held_sends": 0, "held_from": null, "held_queued": false}));
    Session::output(send);
    wait_until("the shell reads both as one line", || {
        session.says(&["read", "--pane", &pane]).contains("half writtenfrom a script")
    });
    let contract = parsed(&session.says(&["contract", "--json"]));
    assert!(items(&contract, "operations").any(|operation| operation["name"] == "held"), "{contract}");
}

#[test]
fn wait_turn_says_the_person_saw_it_and_whoever_follows_gets_a_line() {
    let session = Session::new();
    let pane = shell_pane(&session);
    let send = typed_and_held(&session, &pane);
    let mut following = session.spawn(&["status", "--follow"]);
    let mut followed = Followed::of(&mut following);
    followed.until("the held message", |state| pane_in(state, &pane)["held_sends"] == 1);

    let said = session.says(&["held", "--pane", &pane, "--wait-turn"]);

    assert_eq!(said, format!("1 message held in pane {pane}, queued to go right after the person's"));
    followed.until("the line that says it", |state| pane_in(state, &pane)["held_queued"] == true);
    assert_eq!(pane_of(&session, &pane)["held_sends"], 1, "nothing went out");
    let _ = following.kill();
    let _ = following.wait();
    session.says(&["input", "--pane", &pane, r#"{"key": {"code": "enter"}}"#]);
    Session::output(send);
    wait_until("the message goes after the person's", || pane_of(&session, &pane)["held_queued"] == false);
}

#[test]
fn nothing_held_is_refused_in_words_and_a_button_is_required() {
    let session = Session::new();
    let pane = shell_pane(&session);
    let nothing = format!("no message is held in pane {pane}");

    let (now, wait) = (
        session.cli(&["held", "--pane", &pane, "--send-now"]),
        session.cli(&["held", "--pane", &pane, "--wait-turn", "--json"]),
    );
    let (neither, both) =
        (session.cli(&["held", "--pane", &pane]), session.cli(&["held", "--pane", &pane, "--send-now", "--wait-turn"]));

    assert!(refused(&now, 1, &nothing) && now.stdout.is_empty(), "{now:?}");
    assert!(refused(&wait, 1, &nothing) && wait.stdout.is_empty(), "{wait:?}");
    assert!(refused(&neither, 2, "--send-now"), "{neither:?}");
    assert!(refused(&both, 2, "cannot be used with"), "{both:?}");
}

#[test]
fn held_from_names_the_pane_that_sent_it_even_once_that_pane_closed() {
    let session = Session::new();
    let pane = shell_pane(&session);
    let sender = session.says(&["new-tab"]);
    let boot = parsed(&session.says(&["status", "--json"]))["boot"].as_str().expect("a boot").to_string();
    session.says(&["input", "--pane", &pane, TYPED]);
    let mut send = session.command();
    send.env(PANE_ENV, &sender).env("CORNERCASE_SERVER", boot).args(["send", "--pane", &pane, "from a pane"]);
    let send = send.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("run cornercase");
    held_in(&session, &pane);
    let named = pane_of(&session, &pane)["held_from"].clone();

    session.says(&["close", "--pane", &sender]);
    wait_until("the sender is gone", || pane_of(&session, &sender).is_null());

    assert_eq!(named, json!(id(&sender)));
    assert_eq!(pane_of(&session, &pane)["held_from"], json!(id(&sender)));
    session.says(&["held", "--pane", &pane, "--send-now"]);
    Session::output(send);
}
