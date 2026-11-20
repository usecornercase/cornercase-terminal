use serde_json::{Value, json};

use super::*;
use crate::control::{FlowInfo, FlowSpec, Flows, FlowsSynced, Response};
use crate::flows::store;
use crate::flows::sync::{Change, Journal};
use crate::flows::{Flow, ORCHESTRATOR, Role, Stage};
use crate::machines;
use crate::machines::link::fake::{request, respond};
use crate::paths;

const CLIENT: u64 = 9;

#[derive(Clone, Default)]
struct There {
    listed: Arc<Mutex<Vec<FlowInfo>>>,
    asked: Arc<Mutex<Vec<Value>>>,
    gate: Arc<Mutex<Option<Receiver<()>>>>,
    refuse_once: Arc<Mutex<Option<&'static str>>>,
}

impl There {
    fn count(&self, command: &str) -> usize {
        self.commands().iter().filter(|asked| *asked == command).count()
    }

    fn held_back(&self) -> Sender<()> {
        let (open, gate) = mpsc::channel();
        *self.gate.lock() = Some(gate);
        open
    }

    fn answer(&self, command: &str) -> Response {
        if command == "flow-list" {
            if let Some(gate) = self.gate.lock().take() {
                let _ = gate.recv();
            }
            let flows = Flows { flows: self.listed.lock().clone(), builtins: vec!["Quick".into()], ..Flows::default() };
            return Response::Ok(serde_json::to_value(flows).expect("a listing"));
        }
        let mut refused = self.refuse_once.lock();
        if refused.is_some_and(|refused| refused == command) {
            *refused = None;
            return Response::Error("the file is busy".into());
        }
        Response::Ok(json!({"changed": true}))
    }

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
            let command = asked["command"].as_str().unwrap_or_default().to_string();
            there.asked.lock().push(asked);
            respond(&mut theirs, &there.answer(&command));
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

fn synced_once(there: &There) -> Rig {
    let mut rig = rig_with(vec![syncing("vps", "vps.lan")], there);
    rig.until_rounds(1);
    rig
}

fn held_in_first_round(there: &There) -> (Rig, Sender<()>) {
    let open = there.held_back();
    let mut rig = rig_with(vec![syncing("vps", "vps.lan")], there);
    rig.passes("the first round asks the machine", |_| there.count("flow-list") == 1);
    (rig, open)
}

impl Rig {
    fn save(&mut self, name: &str) -> PathBuf {
        let yours = self.yours();
        self.app.save_flow_file(&yours, &duo(name), None).expect("save a flow")
    }

    fn until_rounds(&mut self, rounds: u64) {
        self.passes("the rounds end", |app| app.machines.flows_rounds("vps.lan") >= rounds);
    }

    fn sync_now(&mut self) -> FlowsSynced {
        let answer = self.ask_flows(&json!({}));
        self.answered_flows(answer)
    }

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

    fn answered_flows(&mut self, mut answer: Option<std::result::Result<Value, (String, String)>>) -> FlowsSynced {
        self.passes("the request is answered", |app| {
            answer = answer.take().or_else(|| app.take_answers().pop().map(|(_, text)| replied(&text)));
            answer.is_some()
        });
        serde_json::from_value(answer.expect("an answer").expect("synced")).expect("parse")
    }

    fn rounds(&self) -> u64 {
        self.app.machines.flows_rounds("vps.lan")
    }

    fn import_from(&mut self, flow: &Flow, from: &str) {
        let text = crate::flows::share::to_share(flow).expect("share");
        let args = json!({"text": text, "place": "yours", "replace": true, "synced_from": from});
        let request = json!({"contract": 1, "command": "flow-import", "args": args}).to_string();
        self.app.request(CLIENT, &request, Some(AREA), Instant::now());
    }

    fn toast(&self) -> Option<String> {
        self.app.messages.toast.as_ref().map(|toast| toast.message.clone())
    }
}

fn listed_as_yours(name: &str, changed: Option<&str>) -> Vec<FlowInfo> {
    let changed = changed.map(str::to_string);
    vec![FlowInfo { name: name.into(), origin: "user".into(), changed, ..FlowInfo::default() }]
}

mod a_machine_that_syncs_flows {
    use super::*;

    #[test]
    fn gets_your_flows_once_linked() {
        let there = There::default();
        let mut rig = rig_with(Vec::new(), &there);
        rig.save("Duo");
        rig.edit(vec![syncing("vps", "vps.lan")], Duration::from_secs(3));

        rig.synced_rounds(&there, 1);

        assert_eq!(there.commands()[..2], ["flow-list", "flow-import"]);
        assert!(there.imported()[0].contains("name = \"Duo\""), "{:?}", there.imported());
        assert!(there.asked.lock()[1]["args"]["synced_from"].is_string());
    }

    #[test]
    fn gets_a_flow_saved_here_at_once_and_is_named_on_it() {
        let there = There::default();
        let mut rig = synced_once(&there);

        rig.save("Duo");
        rig.synced_rounds(&there, 1);
        rig.until_rounds(2);

        let flow = store::read(&rig.yours().join("duo.toml")).expect("the saved flow");
        assert_eq!(rig.app.flow_synced_to(&flow), ["vps"]);
    }

    #[test]
    fn is_told_when_a_flow_is_deleted_here() {
        let there = There::default();
        let mut rig = synced_once(&there);
        let path = rig.save("Duo");
        rig.until_rounds(2);
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

        rig.save("Duo");
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
        rig.save("Duo");

        let answer = rig.ask_flows(&json!({}));
        let synced = rig.answered_flows(answer);

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

        let path = rig.save("Duo");

        assert!(store::read(&path).expect("read it").changed.is_some());
    }

    #[test]
    fn renamed_leave_a_deletion_of_the_old_name_in_the_journal() {
        let mut rig = rig_with(Vec::new(), &There::default());
        let state = rig.dir.path().join("state").join("flow-sync.json");
        rig.app.set_flow_journal(state.clone());
        let path = rig.save("Duo");

        rig.app.save_flow_file(&rig.yours(), &duo("Trio"), Some(&path)).expect("rename it");

        assert!(!rig.dir.path().join("flow-sync.json").exists());
        let journal = Journal::read(&state);
        let events: Vec<(&str, Change)> = journal.events.iter().map(|e| (e.name.as_str(), e.change)).collect();
        assert_eq!(events, [("Duo", Change::Deleted)]);
    }

    #[test]
    fn imported_from_a_machine_keep_its_time_and_say_they_were_replaced() {
        let mut rig = rig_with(Vec::new(), &There::default());
        rig.save("Duo");
        let theirs = Flow { changed: Some("2026-11-20T10:00:00.000Z".into()), notes: "theirs".into(), ..duo("Duo") };

        rig.import_from(&theirs, "laptop");

        let saved = store::read(&rig.yours().join("duo.toml")).expect("the imported flow");
        assert_eq!((saved.changed.as_deref(), saved.notes.as_str()), (Some("2026-11-20T10:00:00.000Z"), "theirs"));
        assert_eq!(rig.toast().as_deref(), Some("Duo was replaced by laptop's copy: your flows sync from there"));
    }
}

mod a_round {
    use super::*;

    #[test]
    fn retries_a_deletion_the_machine_refused() {
        let there = There::default();
        let mut rig = synced_once(&there);
        let path = rig.save("Duo");
        rig.until_rounds(2);
        *there.listed.lock() = listed_as_yours("Duo", None);
        *there.refuse_once.lock() = Some("flow-delete");
        rig.app.delete_flow_file(&path, None).expect("delete the flow");
        rig.until_rounds(3);

        let synced = rig.sync_now();

        assert_eq!(there.count("flow-delete"), 2);
        assert_eq!((synced.machines[0].deleted.clone(), synced.machines[0].failed.len()), (vec!["Duo".into()], 0));
    }

    #[test]
    fn changed_while_it_runs_is_followed_by_one_more_that_the_request_waits_for() {
        let there = There::default();
        let (mut rig, open) = held_in_first_round(&there);
        rig.save("Duo");
        rig.save("Trio");
        let pending = rig.ask_flows(&json!({}));
        assert!(pending.is_none(), "it waits for the rounds");

        drop(open);
        let synced = rig.answered_flows(pending);

        assert_eq!((rig.rounds(), there.count("flow-list")), (2, 2));
        let mut sent = synced.machines[0].sent.clone();
        sent.sort();
        assert_eq!(sent, ["Duo", "Trio"]);
    }

    #[test]
    fn whose_link_falls_answers_the_request_waiting_on_it() {
        let there = There::default();
        let (mut rig, open) = held_in_first_round(&there);
        let pending = rig.ask_flows(&json!({}));
        let down = machines::Status { state: State::Unreachable, ..machines::Status::default() };
        rig.app.machines.set_status(0, down);

        drop(open);
        let synced = rig.answered_flows(pending);

        let error = synced.machines[0].error.clone().unwrap_or_default();
        assert!(error.ends_with("your flows go there when its link comes back"), "{error}");
    }

    #[test]
    fn leaves_a_flow_saved_later_on_the_machine_and_says_so() {
        let there = There::default();
        *there.listed.lock() = listed_as_yours("Duo", Some("2099-01-01T00:00:00.000Z"));
        let mut rig = synced_once(&there);
        rig.save("Duo");

        let synced = rig.sync_now();

        assert_eq!((synced.machines[0].kept.clone(), there.count("flow-import")), (vec!["Duo".into()], 0));
    }

    #[test]
    fn stops_at_once_when_the_machine_no_longer_syncs_flows() {
        let there = There::default();
        let mut rig = synced_once(&there);
        let off = json!({"contract": 1, "command": "machines", "args": {"action": "set", "entry": {"name": "vps", "sync_flows": false}}});
        rig.app.request(CLIENT, &off.to_string(), Some(AREA), Instant::now());

        rig.save("Duo");

        assert_eq!((rig.rounds(), there.count("flow-list")), (1, 1));
    }
}

mod an_import_from_a_machine {
    use super::*;

    #[test]
    fn with_a_time_in_the_future_takes_this_computer_s() {
        let mut rig = rig_with(Vec::new(), &There::default());
        let pinned = Flow { changed: Some("9999-12-31T00:00:00.000Z".into()), ..duo("Duo") };

        rig.import_from(&pinned, "laptop");

        let saved = store::read(&rig.yours().join("duo.toml")).expect("the imported flow");
        let changed = saved.changed.as_deref().and_then(crate::time::Stamp::parse).expect("a time");
        assert!(changed <= SystemTime::now(), "{:?}", saved.changed);
    }

    #[test]
    fn of_a_new_flow_says_where_it_came_from_in_one_clean_line() {
        let mut rig = rig_with(Vec::new(), &There::default());

        rig.import_from(&duo("Duo"), "lap\u{202e}top\nmini");

        assert_eq!(rig.toast().as_deref(), Some("Duo arrived from laptop mini's flows: your flows sync from there"));
    }
}

mod a_flow_of_yours_with_a_time_ahead {
    use super::*;

    #[test]
    fn is_saved_with_this_computer_s_time_and_is_then_the_same_there() {
        let there = There::default();
        let mut rig = synced_once(&there);
        let ahead = Flow { changed: Some("2099-01-01T00:00:00.000Z".into()), ..duo("Duo") };
        store::save(&rig.yours(), &ahead, None).expect("save a flow by hand");

        let first = rig.sync_now();
        let fixed = store::read(&rig.yours().join("duo.toml")).expect("the flow");
        let info =
            FlowInfo { flow: FlowSpec::from(&fixed), ..listed_as_yours("Duo", fixed.changed.as_deref()).remove(0) };
        *there.listed.lock() = vec![info];
        let second = rig.sync_now();

        assert_ne!(fixed.changed.as_deref(), Some("2099-01-01T00:00:00.000Z"));
        assert_eq!((first.machines[0].sent.clone(), second.machines[0].same), (vec!["Duo".into()], 1));
        assert_eq!(rig.app.flow_synced_to(&fixed), ["vps"]);
    }
}
