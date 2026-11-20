use serde_json::{Value, json};

use super::*;
use crate::control::{FlowInfo, Flows, FlowsSynced, Response};
use crate::flows::store;
use crate::flows::sync::{Change, Journal};
use crate::flows::{Flow, ORCHESTRATOR, Role, Stage};
use crate::machines::link::fake::{request, respond};
use crate::paths;

const CLIENT: u64 = 9;

#[derive(Clone, Default)]
struct There {
    listed: Arc<Mutex<Vec<FlowInfo>>>,
    asked: Arc<Mutex<Vec<Value>>>,
}

impl There {
    fn commands(&self) -> Vec<String> {
        self.asked.lock().iter().map(|asked| asked["command"].as_str().unwrap_or_default().to_string()).collect()
    }

    fn imported(&self) -> Vec<String> {
        let asked = self.asked.lock();
        let imports = asked.iter().filter(|asked| asked["command"] == "flow-import");
        imports.map(|asked| asked["args"]["text"].as_str().unwrap_or_default().to_string()).collect()
    }
}

struct Peer(There);

impl Connect for Peer {
    fn connect(&self, _: &Killer) -> Result<Line> {
        let (line, mut theirs) = line(version::CURRENT, None);
        let there = self.0.clone();
        std::thread::spawn(move || {
            let asked = request(&mut theirs);
            let listing = asked["command"] == "flow-list";
            there.asked.lock().push(asked);
            let value = if listing {
                let flows =
                    Flows { flows: there.listed.lock().clone(), builtins: vec!["Quick".into()], ..Flows::default() };
                serde_json::to_value(flows).expect("a listing")
            } else {
                json!({"changed": true})
            };
            respond(&mut theirs, &Response::Ok(value));
        });
        Ok(line)
    }
}

fn duo(name: &str) -> Flow {
    let role =
        |name: &str| Role { name: name.into(), agent: "claude".into(), prompt: "Do it.".into(), ..Role::default() };
    Flow {
        name: name.into(),
        orchestrator: role(ORCHESTRATOR),
        roles: vec![role("coder")],
        stages: vec![Stage { roles: vec!["coder".into()], back: None }],
        ..Flow::default()
    }
}

fn syncing(name: &str, destination: &str) -> Machine {
    Machine { sync_flows: true, ..machine(name, destination) }
}

fn rig_with(machines: Vec<Machine>, there: &There) -> Rig {
    let mut rig = Rig::new(Vec::new(), super::There::Missing);
    let seen = rig.seen.clone();
    let there = there.clone();
    rig.app.machines.set_connector(Box::new(move |destination| {
        if seen.count("link") > 0 {
            return Box::new(Peer(there.clone()));
        }
        seen.note("link".into());
        Box::new(Fake {
            destination: destination.into(),
            there: super::There::Server("vps-1".into()),
            seen: seen.clone(),
        })
    }));
    rig.edit(machines, Duration::from_secs(2));
    rig
}

impl Rig {
    fn yours(&self) -> PathBuf {
        paths::flows(self.app.config_file.path())
    }

    fn passes(&mut self, what: &str, mut done: impl FnMut(&mut App) -> bool) {
        let (app, rx, start) = (&mut self.app, &self.rx, self.start);
        wait_until(what, || {
            drain(app, rx);
            app.refresh_at(start + Duration::from_secs(3), CLOCK + 3);
            done(app)
        });
    }

    fn synced_rounds(&mut self, there: &There, imports: usize) {
        self.passes("the flows reach the machine", |_| there.imported().len() >= imports);
    }

    fn ask_flows(&mut self, args: &Value) -> Option<std::result::Result<Value, (String, String)>> {
        let text = json!({"contract": 1, "command": "flow-sync", "args": args}).to_string();
        self.app.request(CLIENT, &text, Some(AREA), Instant::now());
        self.app.take_answers().pop().map(|(_, text)| replied(&text))
    }
}

mod a_machine_that_syncs_flows {
    use super::*;

    #[test]
    fn gets_your_flows_once_linked() {
        let there = There::default();
        let mut rig = rig_with(Vec::new(), &there);
        rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");
        rig.edit(vec![syncing("vps", "vps.lan")], Duration::from_secs(3));

        rig.synced_rounds(&there, 1);

        assert_eq!(there.commands()[..2], ["flow-list", "flow-import"]);
        assert!(there.imported()[0].contains("name = \"Duo\""), "{:?}", there.imported());
        assert!(there.asked.lock()[1]["args"]["synced_from"].is_string());
    }

    #[test]
    fn gets_a_flow_saved_here_at_once_and_is_named_on_it() {
        let there = There::default();
        let mut rig = rig_with(vec![syncing("vps", "vps.lan")], &there);
        rig.passes("the first sync ends", |app| app.machines.flows_rounds("vps.lan") >= 1);

        rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");
        rig.synced_rounds(&there, 1);
        rig.passes("the sync ends", |app| app.machines.flows_rounds("vps.lan") >= 2);

        let flow = store::read(&rig.yours().join("duo.toml")).expect("the saved flow");
        assert_eq!(rig.app.flow_synced_to(&flow), ["vps"]);
    }

    #[test]
    fn is_told_when_a_flow_is_deleted_here() {
        let there = There::default();
        let mut rig = rig_with(vec![syncing("vps", "vps.lan")], &there);
        rig.passes("the first sync ends", |app| app.machines.flows_rounds("vps.lan") >= 1);
        let path = rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");
        rig.passes("the flow is sent", |app| app.machines.flows_rounds("vps.lan") >= 2);
        *there.listed.lock() = vec![FlowInfo { name: "Duo".into(), origin: "user".into(), ..FlowInfo::default() }];

        rig.app.delete_flow_file(&path, None).expect("delete the flow");
        rig.passes("the deletion is sent", |_| there.commands().contains(&"flow-delete".to_string()));

        let deleted = there.asked.lock().iter().find(|asked| asked["command"] == "flow-delete").cloned();
        assert_eq!(deleted.map(|asked| asked["args"]["name"].clone()), Some(json!("Duo")));
    }
}

mod a_machine_that_does_not_sync_flows {
    use super::*;

    #[test]
    fn gets_nothing() {
        let there = There::default();
        let mut rig = rig_with(vec![machine("vps", "vps.lan")], &there);
        rig.passes("the machine links", |app| app.machines.held()[0].status.state == State::Linked);

        rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");
        rig.refresh(Duration::from_secs(4));

        assert_eq!(there.commands(), Vec::<String>::new());
    }

    #[test]
    fn is_refused_by_name_with_how_to_turn_it_on() {
        let there = There::default();
        let mut rig = rig_with(vec![machine("vps", "vps.lan")], &there);

        let refused = rig.ask_flows(&json!({"machine": "vps"})).expect("an answer at once");

        let (code, message) = refused.expect_err("refused");
        assert_eq!(code, "conflict");
        assert!(message.contains("cornercase machines edit vps --sync-flows"), "{message}");
    }
}

mod the_request {
    use super::*;

    #[test]
    fn answers_what_it_sent_once_the_machine_answered() {
        let there = There::default();
        let mut rig = rig_with(vec![syncing("vps", "vps.lan")], &there);
        rig.passes("the machine links", |app| app.machines.held()[0].status.state == State::Linked);
        rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");

        let mut answer = rig.ask_flows(&json!({}));
        rig.passes("the request is answered", |app| {
            answer = answer.take().or_else(|| app.take_answers().pop().map(|(_, text)| replied(&text)));
            answer.is_some()
        });

        let synced: FlowsSynced = serde_json::from_value(answer.expect("an answer").expect("synced")).expect("parse");
        assert_eq!(synced.machines.len(), 1);
        assert_eq!((synced.machines[0].machine.as_str(), synced.machines[0].error.as_deref()), ("vps", None));
    }

    #[test]
    fn is_refused_when_no_machine_syncs_flows() {
        let mut rig = rig_with(Vec::new(), &There::default());

        let (code, _) = rig.ask_flows(&json!({})).expect("an answer at once").expect_err("refused");

        assert_eq!(code, "conflict");
    }
}

mod your_flows {
    use super::*;

    #[test]
    fn saved_here_carry_the_time_they_were_saved() {
        let mut rig = rig_with(Vec::new(), &There::default());

        let path = rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");

        assert!(store::read(&path).expect("read it").changed.is_some());
    }

    #[test]
    fn renamed_leave_a_deletion_of_the_old_name_in_the_journal() {
        let mut rig = rig_with(Vec::new(), &There::default());
        let path = rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");

        rig.app.save_flow_file(&rig.yours(), &duo("Trio"), Some(&path)).expect("rename it");

        let journal = Journal::read(&rig.dir.path().join("flow-sync.json"));
        let events: Vec<(&str, Change)> = journal.events.iter().map(|e| (e.name.as_str(), e.change)).collect();
        assert_eq!(events, [("Duo", Change::Deleted)]);
    }

    #[test]
    fn imported_from_a_machine_keep_its_time_and_say_they_were_replaced() {
        let mut rig = rig_with(Vec::new(), &There::default());
        rig.app.save_flow_file(&rig.yours(), &duo("Duo"), None).expect("save a flow");
        let theirs = Flow { changed: Some("2026-11-20T10:00:00.000Z".into()), notes: "theirs".into(), ..duo("Duo") };
        let text = crate::flows::share::to_share(&theirs).expect("share");
        let args = json!({"text": text, "place": "yours", "replace": true, "synced_from": "laptop"});
        let request = json!({"contract": 1, "command": "flow-import", "args": args}).to_string();

        rig.app.request(CLIENT, &request, Some(AREA), Instant::now());

        let saved = store::read(&rig.yours().join("duo.toml")).expect("the imported flow");
        assert_eq!((saved.changed.as_deref(), saved.notes.as_str()), (Some("2026-11-20T10:00:00.000Z"), "theirs"));
        let toast = rig.app.messages.toast.as_ref().map(|toast| toast.message.clone());
        assert_eq!(toast.as_deref(), Some("Duo was replaced by laptop's copy: your flows sync from there"));
    }
}
