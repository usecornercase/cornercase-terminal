use std::sync::Arc;
use std::thread;

use parking_lot::Mutex;
use serde_json::json;

use super::*;
use crate::control::contract::{Code, Refusal, Structured};
use crate::error::Result;
use crate::machines::link::Line;
use crate::machines::link::fake::{Once, line, request, respond};
use crate::protocol::{self, ServerMessage};
use crate::remote::Killer;

#[derive(Clone)]
struct Machine {
    listed: Value,
    refuses: Option<&'static str>,
    asked: Arc<Mutex<Vec<Value>>>,
}

impl Machine {
    fn new(listed: Value) -> Self {
        Self { listed, refuses: None, asked: Arc::default() }
    }

    fn asked(&self) -> Vec<Value> {
        self.asked.lock().clone()
    }
}

impl Connect for Machine {
    fn connect(&self, _: &Killer) -> Result<Line> {
        let (line, mut theirs) = line("0.12.300", None);
        let machine = self.clone();
        thread::spawn(move || {
            let asked = request(&mut theirs);
            let command = asked["command"].as_str().unwrap_or_default().to_string();
            machine.asked.lock().push(asked);
            if command == "flow-list" {
                respond(&mut theirs, &Response::Ok(machine.listed.clone()));
            } else if machine.refuses == Some(command.as_str()) {
                let refusal = Structured::Error(Refusal::new(Code::Malformed, "the flow cannot run: no roles"));
                let text = serde_json::to_string(&refusal).expect("a refusal");
                protocol::send(&mut theirs, &ServerMessage::Response(text)).expect("refuse");
            } else {
                respond(&mut theirs, &Response::Ok(json!({"changed": true})));
            }
        });
        Ok(line)
    }
}

fn spec(name: &str) -> FlowSpec {
    FlowSpec { name: name.into(), description: "theirs".into(), ..FlowSpec::default() }
}

fn listed(flows: &[(&str, &str, Option<&str>)], hidden: &[&str]) -> Value {
    let info = |(name, origin, changed): &(&str, &str, Option<&str>)| FlowInfo {
        name: (*name).into(),
        origin: (*origin).into(),
        changed: changed.map(str::to_string),
        flow: spec(name),
        ..FlowInfo::default()
    };
    let hidden = hidden.iter().map(|name| FlowInfo { name: (*name).into(), ..FlowInfo::default() }).collect();
    let flows = Flows {
        flows: flows.iter().map(info).collect(),
        hidden,
        builtins: vec!["Quick".into(), "Arena".into()],
        ..Flows::default()
    };
    serde_json::to_value(flows).expect("a listing")
}

fn sending(names: &[&str]) -> Sending {
    let flows = names.iter().map(|name| Held { name: (*name).into(), changed: None, spec: FlowSpec::default() });
    Sending {
        ours: Ours { flows: flows.collect(), hidden: vec!["Arena".into()], events: Vec::new(), synced: None },
        texts: names.iter().map(|name| ((*name).to_string(), format!("name = \"{name}\""))).collect(),
        from: Some("laptop".into()),
    }
}

mod a_sync {
    use super::*;

    #[test]
    fn imports_each_flow_the_machine_lacks_and_deletes_a_built_in_deleted_here() {
        let machine = Machine::new(listed(&[("Theirs", "user", None), ("Quick", "built-in", None)], &[]));

        let synced = sync(&machine, "vps", &sending(&["Duo"]));

        let asked = machine.asked();
        assert_eq!(
            asked.iter().map(|asked| asked["command"].clone()).collect::<Vec<_>>(),
            ["flow-list", "flow-import", "flow-hide"]
        );
        assert_eq!(asked[1]["args"]["text"], "name = \"Duo\"");
        assert_eq!(asked[1]["args"]["place"], "yours");
        assert_eq!(asked[1]["args"]["replace"], true);
        assert_eq!(asked[1]["args"]["synced_from"], "laptop");
        assert_eq!(asked[2]["args"]["name"], "Arena");
        assert_eq!((synced.report.sent, synced.report.hidden), (vec!["Duo".to_string()], vec!["Arena".to_string()]));
        assert!(synced.started.is_some());
    }

    #[test]
    fn lists_a_refused_flow_with_why_and_goes_on() {
        let mut machine = Machine::new(listed(&[], &["Arena"]));
        machine.refuses = Some("flow-import");

        let synced = sync(&machine, "vps", &sending(&["Duo", "Pair"]));

        let failed: Vec<(&str, &str)> =
            synced.report.failed.iter().map(|f| (f.name.as_str(), f.error.as_str())).collect();
        assert_eq!(failed, [("Duo", "the flow cannot run: no roles"), ("Pair", "the flow cannot run: no roles")]);
        assert_eq!(machine.asked().len(), 3);
    }

    #[test]
    fn of_a_machine_out_of_reach_says_why_and_sends_nothing() {
        let synced = sync(&Once(Mutex::new(None)), "vps", &sending(&["Duo"]));

        assert!(synced.report.error.is_some_and(|error| error.contains("Connection refused")));
        assert_eq!((synced.report.sent.len(), synced.started), (0, None));
    }

    #[test]
    fn leaves_a_flow_saved_there_later_and_counts_the_same_ones() {
        let machine = Machine::new(listed(
            &[("Duo", "user", Some("2026-11-20T10:00:00.000Z")), ("Pair", "user", None)],
            &["Arena"],
        ));
        let mut sending = sending(&["Duo", "Pair"]);
        sending.ours.flows[1].spec = spec("Pair");

        let synced = sync(&machine, "vps", &sending);

        assert_eq!((synced.report.kept, synced.report.same), (vec!["Duo".to_string()], 1));
        assert_eq!(synced.same, [("Pair".to_string(), None)]);
        assert_eq!(machine.asked().len(), 1);
    }
}
