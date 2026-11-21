use serde_json::{Value, json};

use super::*;
use crate::control::{FlowInfo, FlowSpec, Flows, FlowsSynced, Response};
use crate::flows::store;
use crate::flows::{Flow, ORCHESTRATOR, Role, Stage};
use crate::machines;
use crate::machines::link::fake::{request, respond};
use crate::paths;

const CLIENT: u64 = 9;

#[derive(Clone, Default)]
struct There {
    listed: Arc<Mutex<Vec<FlowInfo>>>,
    asked: Arc<Mutex<Vec<Value>>>,
    refuse_once: Arc<Mutex<Option<&'static str>>>,
}

impl There {
    fn count(&self, command: &str) -> usize {
        self.commands().iter().filter(|asked| *asked == command).count()
    }

    fn answer(&self, command: &str) -> Response {
        let refused = {
            let mut refused = self.refuse_once.lock();
            refused.take_if(|refused| *refused == command).is_some()
        };
        if refused {
            return Response::Error("the file is busy".into());
        }
        if command == "contract" {
            return Response::Ok(json!({"features": ["flow-sync"]}));
        }
        if command == "flow-list" {
            let flows = Flows { flows: self.listed.lock().clone(), builtins: vec!["Quick".into()], ..Flows::default() };
            return Response::Ok(serde_json::to_value(flows).expect("a listing"));
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

fn linked(there: &There) -> Rig {
    let mut rig = rig_with(vec![machine("vps", "vps.lan")], there);
    rig.passes("the machine links", |app| app.machines.held()[0].status.state == State::Linked);
    rig
}

impl Rig {
    fn save(&mut self, name: &str) -> PathBuf {
        let yours = self.yours();
        self.app.save_flow_file(&yours, &duo(name), None).expect("save a flow")
    }

    fn sync_now(&mut self, args: &Value) -> FlowsSynced {
        let answer = self.ask_flows(args);
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

    fn ask_flows(&mut self, args: &Value) -> Option<std::result::Result<Value, (String, String)>> {
        let text = json!({"contract": 1, "command": "flow-sync", "args": args}).to_string();
        self.app.request(CLIENT, &text, Some(AREA), Instant::now());
        self.app.take_answers().pop().map(|(_, text)| replied(&text))
    }

    fn refused(&mut self, args: &Value) -> (String, String) {
        self.ask_flows(args).expect("an answer at once").expect_err("refused")
    }

    fn answered_flows(&mut self, mut answer: Option<std::result::Result<Value, (String, String)>>) -> FlowsSynced {
        self.passes("the request is answered", |app| {
            answer = answer.take().or_else(|| app.take_answers().pop().map(|(_, text)| replied(&text)));
            answer.is_some()
        });
        serde_json::from_value(answer.expect("an answer").expect("synced")).expect("parse")
    }

    fn import_from(&mut self, flow: &Flow, from: &str) -> std::result::Result<Value, (String, String)> {
        let text = crate::flows::share::to_share(flow).expect("share");
        let args = json!({"text": text, "place": "yours", "replace": true, "synced_from": from});
        let request = json!({"contract": 1, "command": "flow-import", "args": args}).to_string();
        self.app.request(CLIENT, &request, Some(AREA), Instant::now());
        self.app.take_answers().pop().map(|(_, text)| replied(&text)).expect("an answer at once")
    }

    fn toast(&self) -> Option<String> {
        self.app.messages.toast.as_ref().map(|toast| toast.message.clone())
    }
}

fn listed_as_yours(name: &str, changed: Option<&str>) -> Vec<FlowInfo> {
    let changed = changed.map(str::to_string);
    vec![FlowInfo { name: name.into(), origin: "user".into(), changed, ..FlowInfo::default() }]
}

mod nothing {
    use super::*;

    #[test]
    fn is_sent_on_its_own_when_a_flow_changes_or_a_machine_links() {
        let there = There::default();
        let mut rig = rig_with(Vec::new(), &there);
        let path = rig.save("Duo");
        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(3));
        rig.passes("the machine links", |app| app.machines.held()[0].status.state == State::Linked);

        rig.save("Trio");
        rig.app.delete_flow_file(&path, None).expect("delete a flow");
        rig.refresh(Duration::from_secs(60));

        assert_eq!(there.commands(), Vec::<String>::new());
    }
}

mod the_request {
    use super::*;

    #[test]
    fn sends_every_flow_of_yours_and_answers_what_each_machine_got() {
        let there = There::default();
        let mut rig = linked(&there);
        rig.save("Duo");

        let synced = rig.sync_now(&json!({}));

        assert_eq!(there.commands(), ["contract", "flow-list", "flow-import"]);
        assert!(there.imported()[0].contains("name = \"Duo\""), "{:?}", there.imported());
        assert!(there.asked.lock()[2]["args"]["synced_from"].is_string());
        assert_eq!((synced.machines[0].machine.as_str(), synced.machines[0].sent.clone()), ("vps", vec!["Duo".into()]));
    }

    #[test]
    fn sends_only_the_flow_it_names() {
        let there = There::default();
        let mut rig = linked(&there);
        rig.save("Duo");
        rig.save("Trio");

        let synced = rig.sync_now(&json!({"name": "trio"}));

        assert_eq!((there.imported().len(), synced.machines[0].sent.clone()), (1, vec!["Trio".into()]));
    }

    #[test]
    fn refuses_a_flow_that_is_not_yours_and_says_why() {
        let mut rig = linked(&There::default());

        let (missing, _) = rig.refused(&json!({"name": "Nope"}));
        let (built_in, why) = rig.refused(&json!({"name": "Quick"}));

        assert_eq!((missing.as_str(), built_in.as_str()), ("not-found", "conflict"));
        assert!(why.contains("only your own flows sync"), "{why}");
    }

    #[test]
    fn leaves_a_flow_saved_later_on_the_machine_and_says_so() {
        let there = There::default();
        *there.listed.lock() = listed_as_yours("Duo", Some("2099-01-01T00:00:00.000Z"));
        let mut rig = linked(&there);
        rig.save("Duo");

        let synced = rig.sync_now(&json!({}));

        assert_eq!((synced.machines[0].kept.clone(), there.count("flow-import")), (vec!["Duo".into()], 0));
    }

    #[test]
    fn says_why_a_machine_still_connecting_got_nothing_without_waiting() {
        let there = There::default();
        let mut rig = rig_with(vec![machine("vps", "vps.lan")], &there);
        let connecting = machines::Status { state: State::Connecting, ..machines::Status::default() };
        rig.app.machines.set_status(0, connecting);

        let synced = rig.ask_flows(&json!({})).expect("an answer at once").expect("synced");

        assert_eq!(synced["machines"][0]["error"], "vps is still connecting");
    }

    #[test]
    fn tells_to_update_a_machine_whose_link_found_an_older_cornercase() {
        let there = There::default();
        let mut rig = rig_with(vec![machine("vps", "vps.lan")], &there);
        let older = machines::Status { state: State::Older, ..machines::Status::default() };
        rig.app.machines.set_status(0, older);

        let synced = rig.ask_flows(&json!({})).expect("an answer at once").expect("synced");

        assert_eq!(synced["machines"][0]["error"], "it runs an older cornercase: update it there, then sync again");
        assert_eq!(there.commands(), Vec::<String>::new());
    }

    #[test]
    fn is_refused_when_no_machine_is_shown_or_the_one_named_is_hidden() {
        let mut none = rig_with(Vec::new(), &There::default());
        let hidden = Machine { shown: false, ..machine("vps", "vps.lan") };
        let mut one = rig_with(vec![hidden], &There::default());

        let (code, _) = none.refused(&json!({}));
        let (all_hidden, how) = one.refused(&json!({}));
        let (hidden_code, why) = one.refused(&json!({"machine": "vps"}));

        assert_eq!((code.as_str(), all_hidden.as_str(), hidden_code.as_str()), ("conflict", "conflict", "not-found"));
        assert!(why.contains("cornercase machines show vps") && how.contains("cornercase machines show vps"), "{how}");
    }

    #[test]
    fn reports_a_flow_too_large_to_share_as_failed() {
        let there = There::default();
        let mut rig = linked(&there);
        let huge = Flow { notes: "x".repeat(300 * 1024), ..duo("Huge") };
        store::save(&rig.yours(), &huge, None).expect("save a large flow by hand");

        let synced = rig.sync_now(&json!({}));

        let failed: Vec<&str> = synced.machines[0].failed.iter().map(|failed| failed.name.as_str()).collect();
        assert_eq!((failed, there.count("flow-import")), (vec!["Huge"], 0));
    }
}

mod settings_flows {
    use super::*;
    use crate::app::tests::flows_dialog::button;
    use crate::flows::dialog::{Dialog, SYNC, SYNC_ALL};

    const WIDE: Rect = Rect { x: 0, y: 0, width: 150, height: 44 };

    fn shown(rig: &Rig) -> &Dialog {
        let Some(Overlay::Flows { dialog, .. }) = &rig.app.overlay else { panic!("the flows tab is not open") };
        dialog
    }

    fn buttons(rig: &Rig) -> Vec<&'static str> {
        let ui::Overlay::Flows(view) = overlay_shown(&rig.app, WIDE) else { panic!("the flows tab is not shown") };
        view.buttons
    }

    fn select(rig: &mut Rig, name: &str) {
        let dialog = rig.app.flows_dialog().expect("the flows tab");
        let at = dialog.flows.iter().position(|saved| saved.flow.name == name).expect("the flow is listed");
        dialog.select(at);
    }

    fn opened(there: &There) -> Rig {
        let mut rig = linked(there);
        store::save(&rig.yours(), &duo("Duo"), None).expect("save a flow by hand");
        store::save(&rig.yours(), &duo("Trio"), None).expect("save a flow by hand");
        rig.app.open_flows_tab();
        rig
    }

    fn after_the_click(rig: &mut Rig, label: &str) -> (String, String) {
        button(&mut rig.app, WIDE, label);
        let sending = shown(rig).hint();
        rig.passes("the machine answers", |app| {
            app.flows_dialog().is_some_and(|dialog| dialog.notice.as_ref().is_some_and(|n| !n.ends_with('…')))
        });
        (sending, shown(rig).hint())
    }

    #[test]
    fn offer_sync_all_and_a_flow_s_sync_only_while_a_machine_is_shown_here() {
        let hidden = Machine { shown: false, ..machine("vps", "vps.lan") };
        let mut alone = rig_with(vec![hidden], &There::default());
        store::save(&alone.yours(), &duo("Duo"), None).expect("save a flow by hand");
        alone.app.open_flows_tab();
        select(&mut alone, "Duo");
        let mut rig = opened(&There::default());
        select(&mut rig, "Duo");
        let on_yours = buttons(&rig);
        select(&mut rig, "Quick");
        let on_a_built_in = buttons(&rig);

        assert!(!buttons(&alone).contains(&SYNC_ALL) && !buttons(&alone).contains(&SYNC));
        assert!(on_yours.contains(&SYNC_ALL) && on_yours.contains(&SYNC), "{on_yours:?}");
        assert!(on_a_built_in.contains(&SYNC_ALL) && !on_a_built_in.contains(&SYNC), "{on_a_built_in:?}");
    }

    #[test]
    fn follow_a_machine_shown_or_hidden_while_the_tab_is_open() {
        let hidden = || vec![Machine { shown: false, ..machine("vps", "vps.lan") }];
        let mut rig = rig_with(hidden(), &There::default());
        store::save(&rig.yours(), &duo("Duo"), None).expect("save a flow by hand");
        rig.app.open_flows_tab();
        select(&mut rig, "Duo");
        let before = buttons(&rig);

        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(3));
        let shown = buttons(&rig);
        rig.edit(hidden(), Duration::from_secs(5));
        let hidden_again = buttons(&rig);

        assert!(!before.contains(&SYNC_ALL) && !before.contains(&SYNC), "{before:?}");
        assert!(shown.contains(&SYNC_ALL) && shown.contains(&SYNC), "{shown:?}");
        assert!(!hidden_again.contains(&SYNC_ALL) && !hidden_again.contains(&SYNC), "{hidden_again:?}");
    }

    #[test]
    fn leave_a_notice_put_after_the_click_alone_when_the_machines_answer() {
        let there = There::default();
        let mut rig = opened(&there);
        button(&mut rig.app, WIDE, SYNC_ALL);

        rig.app.flows_dialog().expect("the flows tab").notice = Some("saved Duo".into());
        rig.passes("the machines answer", |app| app.machines.flows.waiting() == 0);

        assert_eq!((shown(&rig).hint(), there.imported().len()), ("saved Duo".to_string(), 2));
    }

    #[test]
    fn offer_no_sync_on_a_project_s_flow() {
        let mut rig = linked(&There::default());
        let project = rig.dir.path().join("shop");
        std::fs::create_dir_all(&project).expect("a project folder");
        rig.app.add_project(project.clone(), AREA).expect("open the project");
        rig.app.save_flow_file(&store::project_dir(&project), &duo("Shared"), None).expect("save");
        rig.app.open_flows_tab();
        select(&mut rig, "Shared");

        let on_the_project_s = buttons(&rig);

        assert!(on_the_project_s.contains(&SYNC_ALL) && !on_the_project_s.contains(&SYNC), "{on_the_project_s:?}");
    }

    #[test]
    fn sync_all_sends_every_flow_of_yours_and_then_says_what_it_sent() {
        let there = There::default();
        let mut rig = opened(&there);

        let (sending, after) = after_the_click(&mut rig, SYNC_ALL);

        assert_eq!(sending, "sending your flows to vps…");
        assert_eq!(after, "vps: sent Duo, Trio");
    }

    #[test]
    fn a_flow_s_sync_sends_only_it() {
        let there = There::default();
        let mut rig = opened(&there);
        select(&mut rig, "Trio");

        let (sending, after) = after_the_click(&mut rig, SYNC);

        assert_eq!((sending.as_str(), after.as_str()), ("sending Trio to vps…", "vps: sent Trio"));
        assert_eq!(there.imported().len(), 1);
    }

    #[test]
    fn sync_all_says_why_a_machine_got_nothing() {
        let there = There::default();
        let mut rig = opened(&there);
        *there.refuse_once.lock() = Some("flow-list");

        let (_, after) = after_the_click(&mut rig, SYNC_ALL);

        assert!(after.starts_with("vps: not synced: ") && after.ends_with("the file is busy"), "{after}");
    }
}

mod an_import_from_a_machine {
    use super::*;

    #[test]
    fn with_a_time_in_the_future_takes_this_computer_s() {
        let mut rig = rig_with(Vec::new(), &There::default());
        let pinned = Flow { changed: Some("9999-12-31T00:00:00.000Z".into()), ..duo("Duo") };

        rig.import_from(&pinned, "laptop").expect("imported");

        let saved = store::read(&rig.yours().join("duo.toml")).expect("the imported flow");
        let changed = saved.changed.as_deref().and_then(crate::time::Stamp::parse).expect("a time");
        assert!(changed <= SystemTime::now(), "{:?}", saved.changed);
    }

    #[test]
    fn of_a_new_flow_says_where_it_came_from_in_one_clean_line() {
        let mut rig = rig_with(Vec::new(), &There::default());

        rig.import_from(&duo("Duo"), "lap\u{202e}top\nmini").expect("imported");

        assert_eq!(rig.toast().as_deref(), Some("Duo arrived from laptop mini's flows: it synced them here"));
    }

    #[test]
    fn without_a_sender_replaces_as_asked_whatever_its_time() {
        let mut rig = rig_with(Vec::new(), &There::default());
        let path = rig.save("Duo");
        let older = Flow { changed: Some("2020-01-01T00:00:00.000Z".into()), notes: "older".into(), ..duo("Duo") };
        let text = crate::flows::share::to_share(&older).expect("share");
        let args = json!({"text": text, "place": "yours", "replace": true});
        let request = json!({"contract": 1, "command": "flow-import", "args": args}).to_string();

        rig.app.request(CLIENT, &request, Some(AREA), Instant::now());

        assert_eq!(store::read(&path).expect("the flow here").notes, "older");
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
    fn with_a_time_ahead_are_saved_with_this_computer_s_time_and_are_then_the_same_there() {
        let there = There::default();
        let mut rig = linked(&there);
        let ahead = Flow { changed: Some("2099-01-01T00:00:00.000Z".into()), ..duo("Duo") };
        store::save(&rig.yours(), &ahead, None).expect("save a flow by hand");

        let first = rig.sync_now(&json!({}));
        let fixed = store::read(&rig.yours().join("duo.toml")).expect("the flow");
        let info =
            FlowInfo { flow: FlowSpec::from(&fixed), ..listed_as_yours("Duo", fixed.changed.as_deref()).remove(0) };
        *there.listed.lock() = vec![info];
        let second = rig.sync_now(&json!({}));

        assert_ne!(fixed.changed.as_deref(), Some("2099-01-01T00:00:00.000Z"));
        assert_eq!((first.machines[0].sent.clone(), second.machines[0].same), (vec!["Duo".into()], 1));
    }
}

#[cfg(unix)]
mod file_modes {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    #[test]
    fn only_an_open_project_s_flows_folder_is_not_private() {
        let mut rig = rig_with(Vec::new(), &There::default());
        let project = rig.dir.path().join("shop");
        std::fs::create_dir_all(&project).expect("a project folder");
        rig.app.add_project(project.clone(), AREA).expect("open the project");
        let elsewhere = rig.dir.path().join("other");

        let theirs = rig.app.save_flow_file(&store::project_dir(&project), &duo("Shared"), None).expect("save");
        let ordinary = store::project_dir(&project).join("ordinary.txt");
        std::fs::write(&ordinary, "plain").expect("an ordinary file");
        let unknown = rig.app.save_flow_file(&store::project_dir(&elsewhere), &duo("Odd"), None).expect("save");
        let yours = rig.save("Duo");

        assert_eq!(mode(&theirs), mode(&ordinary));
        assert_eq!((mode(&unknown), mode(&yours)), (0o600, 0o600));
    }

    #[test]
    fn a_flow_whose_time_ahead_cannot_be_fixed_is_reported_and_not_sent() {
        let there = There::default();
        let mut rig = linked(&there);
        let ahead = Flow { changed: Some("2099-01-01T00:00:00.000Z".into()), ..duo("Ahead") };
        let path = store::save(&rig.yours(), &ahead, None).expect("save a flow by hand");
        let folder = rig.yours();
        let locked = |mode| std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(mode));
        locked(0o500).expect("lock the folder");

        let synced = rig.sync_now(&json!({}));
        locked(0o700).expect("unlock the folder");

        let failed = &synced.machines[0].failed;
        assert_eq!(failed.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Ahead"]);
        assert!(failed[0].error.contains("ahead"), "{failed:?}");
        assert_eq!((there.count("flow-import"), store::read(&path).expect("unchanged").changed), (0, ahead.changed));
    }

    #[test]
    fn a_folder_of_your_flows_that_cannot_be_read_is_reported_as_failed() {
        let there = There::default();
        let mut rig = linked(&there);
        rig.save("Duo");
        let folder = rig.yours();
        let locked = |mode| std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(mode));
        locked(0o000).expect("lock the folder");

        let synced = rig.sync_now(&json!({}));
        locked(0o700).expect("unlock the folder");

        let failed = &synced.machines[0].failed;
        assert_eq!(failed.iter().map(|f| f.name.clone()).collect::<Vec<_>>(), [folder.display().to_string()]);
        assert!(failed[0].error.contains("could not be read"), "{failed:?}");
        assert_eq!(there.count("flow-import"), 0);
    }
}
