use super::*;
use crate::app::tests::bots::secrets::turn;

const VALUE: &str = "hunter2-long-value";

fn asking(b: &mut Bots, run: Option<u64>) -> Value {
    let head = json!({"bot": "prs", "bot_run": run});
    let args = wire(&json!({"action": "secret-ask", "names": ["pw"], "about": "to log in", "replace": false}));
    let mut answers = send_headed(&mut b.app, &b.rx, &head, "bot", &args);
    assert_eq!(answers.len(), 1);
    reply_of(&answers.remove(0).1).unwrap_or_else(|e| panic!("secret-ask: {} {}", e.code, e.message))
}

fn owned(b: &mut Bots) {
    b.app.panels.bots.live[0].turn = Some(turn());
    b.app.clients.bots.owned(CLIENT, "prs");
    b.app.clients.bots.marked(CLIENT, "prs");
}

fn owned_in_run(b: &mut Bots, run: u64) {
    owned(b);
    let turn = crate::app::bots::Turn { thread: crate::app::bots::Thread::Run(run), ..turn() };
    b.app.panels.bots.live[0].turn = Some(turn);
}

fn entry_of(b: &mut Bots, bot: &str) -> u64 {
    let added = b.ask("queue", &json!({"action": "add", "bot": bot, "at": "in 2h", "prompt": "check"}));
    added["id"].as_u64().expect("an entry")
}

fn notice(b: &Bots, ask: u64) -> crate::bots::chat::Line {
    let path = b.app.panels.bots.live[0].folder.join(crate::bots::folder::CHAT_FILE);
    let lines = crate::bots::chat::read(&path);
    lines.into_iter().find(|line| line.ask == Some(ask)).expect("the ask's notice")
}

#[test]
fn an_ask_from_a_run_of_the_bot_carries_the_run_and_is_listed_with_the_bot() {
    let mut b = bots();
    b.new_prs();
    let entry = entry_of(&mut b, "prs");
    owned_in_run(&mut b, entry);
    follow(&mut b.app, &[]);
    let answer = asking(&mut b, Some(entry));
    let ask = answer["ask"].as_u64().expect("an ask");
    assert_eq!(notice(&b, ask).routine, Some(entry));
    let state = serde_json::to_value(b.app.app_state()).expect("json");
    assert_eq!(state["bots"][0]["asks"][0]["id"], json!(ask));
    assert_eq!(state["bots"][0]["asks"][0]["routine"], json!(entry));
    let lines: Vec<crate::bots::chat::Line> = told(&streamed(&mut b.app))
        .into_iter()
        .filter_map(|(what, _)| match what {
            wire::What::Bot { line, .. } => line,
            _ => None,
        })
        .collect();
    assert!(lines.iter().any(|line| line.ask == Some(ask)), "{lines:?}");
}

#[test]
fn a_run_that_is_not_the_bots_own_is_not_kept() {
    let mut b = bots();
    b.new_prs();
    b.ask("bot", &json!({"action": "new", "name": "Other", "agent": "claude", "about": ""}));
    let other = entry_of(&mut b, "other");
    owned(&mut b);
    let first = asking(&mut b, Some(other))["ask"].as_u64().expect("an ask");
    assert_eq!(notice(&b, first).routine, None);
    let args = wire(&json!({"action": "secret-ask", "names": ["user"], "about": "to log in", "replace": false}));
    let head = json!({"bot": "prs", "bot_run": 999});
    let mut answers = send_headed(&mut b.app, &b.rx, &head, "bot", &args);
    let second = reply_of(&answers.remove(0).1).expect("asked")["ask"].as_u64().expect("an ask");
    assert_eq!(notice(&b, second).routine, None);
}

#[test]
fn an_ask_given_cannot_be_declined() {
    let mut b = bots();
    b.new_prs();
    owned(&mut b);
    let ask = asking(&mut b, None)["ask"].as_u64().expect("an ask");
    b.app.clients.bots.forget(CLIENT, std::iter::empty());
    let given = b.ask("bot", &json!({"action": "secret-give", "bot": "prs", "ask": ask, "values": {"pw": VALUE}}));
    assert_eq!(given["saved"], json!(["pw"]));
    let refused = b.refuse(&json!({"action": "secret-decline", "bot": "prs", "ask": ask}));
    assert_eq!(refused.code, "not-found");
}

#[test]
fn a_run_that_is_not_running_cannot_ask_even_while_the_chat_answers() {
    let mut b = bots();
    b.new_prs();
    let entry = entry_of(&mut b, "prs");
    owned(&mut b);
    let args = wire(&json!({"action": "secret-ask", "names": ["pw"], "about": "to log in", "replace": false}));
    let head = json!({"bot": "prs", "bot_run": entry});
    let mut answers = send_headed(&mut b.app, &b.rx, &head, "bot", &args);
    let refused = reply_of(&answers.remove(0).1).expect_err("refused");
    assert_eq!((refused.code.as_str(), refused.message.as_str()), ("failed", crate::bots::asks::ONLY_ITSELF));
    assert_eq!(b.app.panels.bots.live[0].asks.len(), 0);
}
