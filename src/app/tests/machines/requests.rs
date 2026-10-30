use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::app::Streamed;
use crate::control::{MachineList, Response};

const CLIENT: u64 = 7;

fn sent(app: &mut App, args: &Value) {
    let text = json!({"contract": 1, "command": "machines", "args": args}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
}

fn reply(text: &str) -> std::result::Result<MachineList, (String, String)> {
    let reply: Value = serde_json::from_str(text).expect("a reply");
    match &reply["error"] {
        Value::Null => Ok(serde_json::from_value(reply["ok"].clone()).expect("a machine list")),
        error => Err((
            error["code"].as_str().unwrap_or_default().into(),
            error["message"].as_str().unwrap_or_default().into(),
        )),
    }
}

impl Rig {
    fn ask(&mut self, args: &Value) -> std::result::Result<MachineList, (String, String)> {
        sent(&mut self.app, args);
        let answers = self.app.take_answers();
        assert_eq!(answers.len(), 1, "an answer at once");
        reply(&answers[0].1)
    }

    fn ask_later(&mut self, args: &Value) -> std::result::Result<MachineList, (String, String)> {
        sent(&mut self.app, args);
        assert_eq!(self.app.take_answers(), [], "no answer before the job ends");
        self.answered()
    }

    fn answered(&mut self) -> std::result::Result<MachineList, (String, String)> {
        let mut answer = None;
        let (app, rx) = (&mut self.app, &self.rx);
        wait_until("the request is answered", || {
            drain(app, rx);
            answer = app.take_answers().pop().map(|(_, text)| text);
            answer.is_some()
        });
        reply(&answer.expect("an answer"))
    }

    fn listed(&mut self) -> MachineList {
        self.ask(&json!({"action": "list"})).expect("the list")
    }

    fn saved(&self) -> Vec<Machine> {
        config::load(&self.dir.path().join("config.json")).machines
    }
}

fn refused(code: &str, message: &str) -> std::result::Result<MachineList, (String, String)> {
    Err((code.into(), message.into()))
}

mod list {
    use super::*;

    #[test]
    fn names_this_computer_then_each_machine_in_the_file_s_order() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("my vps", "box.lan")], There::Missing);

        let list = rig.listed();

        assert_eq!((list.here.icon.as_str(), list.here.version.as_str()), ("▭", version::CURRENT));
        let rows: Vec<(&str, State, Option<&str>)> =
            list.machines.iter().map(|m| (m.name.as_str(), m.state, m.problem.as_deref())).collect();
        let problem = "use letters, digits, `.`, `_` and `-`, at most 20 characters";
        assert_eq!(rows, [("vps", State::Connecting, None), ("my vps", State::Unreachable, Some(problem))]);
    }

    #[test]
    fn carries_each_link_s_state_and_why() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);

        let list = rig.listed();

        let vps = &list.machines[0];
        assert_eq!(
            (vps.state, vps.error.as_deref()),
            (State::Unreachable, Some("cornercase was not found on vps: install it there"))
        );
    }
}

mod set {
    use super::*;

    #[test]
    fn adds_a_machine_with_the_default_icon_and_folder() {
        let mut rig = Rig::new(Vec::new(), There::Missing);

        let list =
            rig.ask(&json!({"action": "set", "entry": {"name": "vps", "destination": "me@vps.lan", "create": true}}));

        let added =
            Machine {
                name: "vps".into(), icon: "☁".into(), destination: "me@vps.lan".into(), folder: "~/code".into()
            };
        assert_eq!(list.expect("added").machines[0].state, State::Connecting);
        assert_eq!(rig.saved(), [added]);
    }

    #[test]
    fn edits_only_what_it_was_given_and_keeps_the_order() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")], There::Missing);

        let edit =
            json!({"action": "set", "entry": {"name": "VPS", "new_name": "box", "icon": "laptop", "folder": "/srv"}});
        rig.ask(&edit).expect("edited");

        let edited =
            Machine { name: "box".into(), icon: "▭".into(), destination: "vps.lan".into(), folder: "/srv".into() };
        assert_eq!(rig.saved(), [edited, machine("pi", "pi.lan")]);
    }

    #[rstest]
    #[case::a_bad_name(json!({"name": "my vps", "destination": "x.lan"}), "malformed", "use letters, digits, `.`, `_` and `-`, at most 20 characters")]
    #[case::an_option_for_ssh(json!({"name": "box", "destination": "-oProxyCommand=sh"}), "malformed", "a destination cannot start with `-`")]
    #[case::no_destination(json!({"name": "box"}), "malformed", "the destination is required")]
    #[case::a_relative_folder(json!({"name": "box", "destination": "x.lan", "folder": "code"}), "malformed", "use an absolute path or one starting with ~/")]
    #[case::an_unknown_icon(json!({"name": "box", "destination": "x.lan", "icon": "castle"}), "malformed", "`castle` is not an icon; use one of ☁ server, ▭ laptop, ▣ desktop, ◫ mini, ⌂ home server, ◆ other")]
    #[case::a_used_destination(json!({"name": "box", "destination": "pi.lan"}), "conflict", "the machine `pi` already uses this destination")]
    #[case::a_used_name(json!({"name": "pi", "new_name": "VPS"}), "conflict", "another machine is already called `vps`")]
    #[case::adding_a_name_in_use(json!({"name": "PI", "destination": "x.lan", "create": true}), "conflict", "a machine is already called `pi`")]
    #[case::editing_an_unknown_one(json!({"name": "box", "folder": "/srv", "create": false}), "not-found", "no machine is called `box`; `cornercase machines` lists them")]
    fn refuses_what_the_form_refuses_and_writes_nothing(
        #[case] entry: Value,
        #[case] code: &str,
        #[case] message: &str,
    ) {
        let both = vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")];
        let mut rig = Rig::new(both.clone(), There::Missing);

        assert_eq!(rig.ask(&json!({"action": "set", "entry": entry})), refused(code, message));
        assert_eq!(rig.saved(), both);
    }
}

mod remove {
    use super::*;

    #[test]
    fn drops_the_machine_and_its_link_on_the_next_refresh() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")], There::Missing);

        let list = rig.ask(&json!({"action": "remove", "entry": "VPS"})).expect("removed");

        assert_eq!(list.machines.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["pi"]);
        assert_eq!(rig.saved(), [machine("pi", "pi.lan")]);
        rig.refresh(Duration::from_secs(2));
        wait_until("its link ends", || rig.seen("end vps.lan") == 1);
    }

    #[test]
    fn drops_only_the_first_of_two_entries_written_alike() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("vps", "vps.lan")], There::Missing);

        rig.ask(&json!({"action": "remove", "entry": "vps"})).expect("removed");

        assert_eq!(rig.saved(), [machine("vps", "vps.lan")]);
    }

    #[test]
    fn of_an_unknown_machine_is_not_found() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);

        assert_eq!(
            rig.ask(&json!({"action": "remove", "entry": "pi"})),
            refused("not-found", "no machine is called `pi`; `cornercase machines` lists them")
        );
    }
}

mod update {
    use super::*;

    fn older() -> Rig {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Older);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Older);
        rig
    }

    #[test]
    fn answers_once_the_install_ends() {
        let mut rig = older();
        rig.app.machines.set_updater(Arc::new(|_: &str| Ok(())));

        let list = rig.ask_later(&json!({"action": "update", "entry": "vps"})).expect("updated");

        assert!(!list.machines[0].updating);
        wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
    }

    #[test]
    fn that_fails_there_is_refused_with_what_was_printed() {
        let mut rig = older();
        rig.app.machines.set_updater(Arc::new(|_: &str| {
            Err(Error::Ssh { host: "vps.lan".into(), reason: "brew upgrade cornercase".into() })
        }));

        assert_eq!(
            rig.ask_later(&json!({"action": "update", "entry": "vps"})),
            refused("failed", "brew upgrade cornercase")
        );
    }

    #[test]
    fn that_fails_after_its_machine_was_removed_is_refused_without_its_destination() {
        let mut rig = older();
        let (go, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        rig.app.machines.set_updater(Arc::new(move |_: &str| {
            let _ = gate.lock().recv();
            Err(Error::Ssh { host: "vps.lan".into(), reason: "cannot reach vps.lan port 443".into() })
        }));
        sent(&mut rig.app, &json!({"action": "update", "entry": "vps"}));
        rig.edit(Vec::new(), Duration::from_secs(2));
        assert!(rig.app.machines.held().is_empty());

        go.send(()).expect("let the update fail");

        assert_eq!(rig.answered(), refused("failed", "cannot reach vps port 443"));
    }

    #[test]
    fn of_a_machine_that_is_not_older_is_a_conflict() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan"), machine("my vps", "box.lan")], There::Missing);

        for name in ["vps", "my vps"] {
            let message = format!("`{name}` is not older than this cornercase: nothing to install");
            assert_eq!(rig.ask(&json!({"action": "update", "entry": name})), refused("conflict", &message));
        }
    }
}

mod retry {
    use super::*;

    #[test]
    fn tries_a_link_again_at_once() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the first attempt fails", |app| app.machines.held()[0].status.state == State::Unreachable);

        rig.ask(&json!({"action": "retry", "entry": "vps"})).expect("retried");

        wait_until("the link tries again", || rig.seen("connect vps.lan") == 2);
    }

    #[test]
    fn of_every_machine_needs_no_name_and_of_an_unknown_one_is_not_found() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);

        assert!(rig.ask(&json!({"action": "retry"})).is_ok());
        assert_eq!(
            rig.ask(&json!({"action": "retry", "entry": "pi"})).map(|_| ()).map_err(|(code, _)| code),
            Err("not-found".into())
        );
    }
}

mod the_app_stream {
    use super::*;

    fn machines_in(streamed: Vec<(u64, Streamed)>) -> Vec<Vec<State>> {
        streamed
            .into_iter()
            .filter_map(|(_, streamed)| match streamed {
                Streamed::Snapshot(text) => match serde_json::from_str(&text) {
                    Ok(Response::Ok(value)) => Some(value),
                    other => panic!("not a snapshot: {other:?}"),
                },
                Streamed::Event(_) | Streamed::End => None,
            })
            .map(|value| {
                value["machines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| serde_json::from_value(m["state"].clone()).expect("a state"))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn sends_a_new_snapshot_when_a_machine_s_state_changes() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.app.clients.events.app.set_every(Duration::ZERO);
        let text = json!({"command": "app", "args": {}}).to_string();
        rig.app.request(CLIENT, &text, Some(AREA), Instant::now());
        let first = rig.app.take_answers();
        assert!(first[0].1.contains("\"connecting\""), "{first:?}");

        rig.app.trace_refresh();
        assert_eq!(machines_in(rig.app.take_events()), Vec::<Vec<State>>::new());
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);
        rig.app.trace_refresh();

        assert_eq!(machines_in(rig.app.take_events()), [[State::Unreachable]]);
    }
}
