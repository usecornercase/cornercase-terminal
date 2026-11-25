use super::*;
use crate::text::GB;

const CLIENT: u64 = 7;
const TAB: u64 = 9;

fn with_free(available: u64) -> Value {
    json!({ "here": { "icon": "▭", "version": version::CURRENT, "memory": { "total": 32 * GB, "available": available } } })
}

fn roomy(command: &str) -> Value {
    match command {
        "machines" => with_free(11 * GB),
        "start" | "start-with" => json!({ "workspace": WORKSPACE, "tab": TAB, "pane": TAB + 1 }),
        "flow-run" => json!({ "workspace": WORKSPACE, "members": [{ "tab": TAB, "pane": TAB + 1 }] }),
        _ => json!({}),
    }
}

fn cramped(command: &str) -> Value {
    match command {
        "machines" => with_free(GB),
        command => roomy(command),
    }
}

fn older(command: &str) -> Value {
    match command {
        "machines" => json!({ "here": { "icon": "▭", "version": "0.12.0" } }),
        command => roomy(command),
    }
}

struct Auto {
    remote: Remote,
    _dir: TempDir,
}

fn auto(answer: fn(&str) -> Value, projects: Vec<ProjectInfo>) -> Auto {
    let report = control::Report { boot: BOOT.into(), projects, ..control::Report::default() };
    let (remote, dir) = with_shop_here(Reply::Each(answer), report);
    Auto { remote, _dir: dir }
}

impl Auto {
    fn project(&self) -> u64 {
        self.remote.rig.app.projects[0].id
    }

    fn start(&mut self, args: &Value) -> std::result::Result<Value, (String, String)> {
        let text = json!({"contract": 1, "command": "start", "args": args}).to_string();
        self.remote.rig.app.request(CLIENT, &text, Some(AREA), Instant::now());
        let mut answer = None;
        let (app, rx) = (&mut self.remote.rig.app, &self.remote.rig.rx);
        wait_until("the start is answered", || {
            drain(app, rx);
            app.refresh_at(Instant::now(), crate::time::now());
            answer = app.take_answers().into_iter().find(|(client, _)| *client == CLIENT).map(|(_, text)| text);
            answer.is_some()
        });
        replied(&answer.expect("an answer"))
    }

    fn commands(&self) -> Vec<String> {
        self.remote.asked().iter().map(|asked| asked["command"].as_str().unwrap_or_default().to_string()).collect()
    }

    fn sent_start(&self) -> Value {
        let asked = self.remote.asked();
        asked.into_iter().find(|asked| asked["command"] == "start").expect("a start was sent")
    }
}

mod a_start {
    use super::*;

    #[test]
    fn goes_to_the_machine_with_more_room_with_its_own_project_id() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let project = auto.project();

        let answer =
            auto.start(&json!({ "machine": "auto", "project": project, "prompt": "fix it" })).expect("started");

        assert_eq!((&answer["machine"], &answer["auto"]), (&json!("vps"), &json!("auto → vps (11 GB free)")));
        let sent = auto.sent_start();
        assert_eq!((&sent["args"]["project"], &sent["args"]["machine"]), (&json!(1), &json!(null)));
        assert_eq!(auto.commands(), ["machines", "start"]);
    }

    #[test]
    fn reads_the_memory_once_while_it_is_recent() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let project = auto.project();
        auto.start(&json!({ "machine": "auto", "project": project })).expect("started");

        auto.start(&json!({ "machine": "auto", "project": project })).expect("started again");

        assert_eq!(auto.commands(), ["machines", "start", "start"]);
    }

    #[test]
    fn counts_what_it_just_sent_there() {
        let mut auto = auto(roomy, vec![web_shop()]);
        auto.remote.rig.app.config.default_memory = "7GB".into();
        let project = auto.project();
        auto.start(&json!({ "machine": "auto", "project": project })).expect("the first start");

        let (code, message) = auto.start(&json!({ "machine": "auto", "project": project })).expect_err("no room");

        assert_eq!(code, "no-room");
        assert_eq!(message, "no machine has 7 GB free (best: this computer, 6 GB)");
    }

    #[test]
    fn fails_with_the_best_when_nothing_has_room() {
        let mut auto = auto(cramped, vec![web_shop()]);
        auto.remote.rig.app.config.default_memory = "8GB".into();
        let project = auto.project();

        let (code, message) = auto.start(&json!({ "machine": "auto", "project": project })).expect_err("no room");

        assert_eq!(
            (code.as_str(), message.as_str()),
            ("no-room", "no machine has 8 GB free (best: this computer, 6 GB)")
        );
        assert_eq!(auto.commands(), ["machines"]);
    }

    #[test]
    fn leaves_out_a_machine_without_the_project() {
        let mut auto = auto(roomy, Vec::new());
        auto.remote.rig.app.config.default_memory = "8GB".into();
        let project = auto.project();

        let (_, message) = auto.start(&json!({ "machine": "auto", "project": project })).expect_err("no room");

        assert_eq!(message, "no machine has 8 GB free (best: this computer, 6 GB)");
        assert_eq!(auto.commands(), ["machines"]);
    }

    #[test]
    fn never_picks_a_machine_whose_memory_it_cannot_read() {
        let mut auto = auto(older, vec![web_shop()]);
        auto.remote.rig.app.config.default_memory = "8GB".into();
        let project = auto.project();

        let (code, _) = auto.start(&json!({ "machine": "auto", "project": project })).expect_err("no room");

        assert_eq!(code, "no-room");
    }

    #[test]
    fn takes_no_boot_since_its_ids_are_this_computer_s() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let project = auto.project();

        let (code, _) =
            auto.start(&json!({ "machine": "auto", "project": project, "machine_boot": BOOT })).expect_err("refused");

        assert_eq!(code, "malformed");
        assert_eq!(auto.commands(), Vec::<String>::new());
    }
}

mod the_memory {
    use super::*;

    #[test]
    fn of_this_computer_and_each_machine_is_listed() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let project = auto.project();
        auto.start(&json!({ "machine": "auto", "project": project })).expect("started");
        let text = json!({"contract": 1, "command": "machines", "args": {"action": "list"}}).to_string();

        auto.remote.rig.app.request(CLIENT, &text, Some(AREA), Instant::now());

        let answers = auto.remote.rig.app.take_answers();
        let listed = replied(&answers[0].1).expect("the machines");
        assert_eq!(
            (&listed["here"]["memory"]["available"], &listed["machines"][0]["memory"]["available"]),
            (&json!(6 * GB), &json!(11 * GB))
        );
        assert!(listed["machines"][0]["memory"]["read_at"].is_string(), "{listed}");
    }
}

mod the_queue {
    use super::*;
    use crate::queue::State;

    impl Auto {
        fn asks(&mut self, command: &str, args: &Value) -> std::result::Result<Value, (String, String)> {
            let text = json!({"contract": 1, "command": command, "args": args}).to_string();
            self.remote.rig.app.request(CLIENT, &text, Some(AREA), Instant::now());
            let answers = self.remote.rig.app.take_answers();
            replied(&answers.last().expect("an answer at once").1)
        }

        fn waits(&mut self, needs: &str) -> u64 {
            self.remote.rig.app.config.default_memory = needs.into();
            self.remote.rig.app.set_queue_area(AREA);
            let project = self.project();
            let entry = json!({ "when": "free", "project": project, "prompt": "fix it", "agent": "claude" });
            let added = self.asks("queue", &json!({ "action": "add", "entry": entry })).expect("added");
            added["id"].as_u64().expect("an id")
        }

        fn drive(&mut self) {
            self.remote.rig.app.refresh_at(Instant::now(), crate::time::now());
            self.remote.rig.until("the memory is read", |app| !app.memory_asking());
        }

        fn listed(&mut self, id: u64) -> Value {
            let listed = self.asks("queue", &json!({ "action": "list" })).expect("the list");
            let entries = listed["entries"].as_array().cloned().unwrap_or_default();
            entries.into_iter().find(|entry| entry["id"] == id).expect("the entry")
        }
    }

    #[test]
    fn an_entry_that_waits_for_room_starts_on_the_machine_that_has_it() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let id = auto.waits("8GB");

        auto.drive();
        auto.remote.rig.until("it starts there", |app| {
            app.panels.queued.queue().entry(id).is_some_and(|entry| matches!(entry.state, State::Started { .. }))
        });

        let entry = auto.remote.rig.app.panels.queued.queue().entry(id).expect("the entry").clone();
        assert_eq!(entry.there.map(|there| there.machine).as_deref(), Some("vps"));
        let sent = auto.sent_start();
        assert_eq!((&sent["args"]["project"], &sent["args"]["agent"]), (&json!(1), &json!("claude")));
        assert!(sent["args"]["prompt"].as_str().is_some_and(|prompt| prompt.starts_with("fix it")), "{sent}");
        assert_eq!(toast(&auto.remote.rig.app), Some(format!("started queued agent {id} on vps").as_str()));
    }

    #[test]
    fn an_entry_waits_with_why_while_nothing_has_room_and_the_next_waits_behind_it() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let first = auto.waits("16GB");
        let second = auto.waits("16GB");

        auto.drive();

        let listed = auto.listed(first);
        assert_eq!(listed["state"], json!("pending"));
        assert_eq!(
            (&listed["waiting"]["position"], &listed["waiting"]["why"], &listed["waiting"]["needs"]),
            (&json!(1), &json!("needs 16 GB · best: vps 11 GB"), &json!(16u64 << 30))
        );
        assert_eq!(auto.listed(second)["waiting"]["why"], json!(format!("behind queued agent {first}")));
        assert_eq!(auto.commands(), ["machines"]);
        let told = toast(&auto.remote.rig.app).unwrap_or_default().to_string();
        assert!(told.contains("waits for room: 2nd in line"), "{told}");
    }

    #[test]
    fn first_puts_an_entry_at_the_head_of_the_line() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let one = auto.waits("16GB");
        let two = auto.waits("16GB");

        auto.asks("queue", &json!({ "action": "first", "entry": two })).expect("first");
        auto.drive();

        assert_eq!(auto.listed(two)["waiting"]["position"], json!(1));
        assert_eq!(auto.listed(one)["waiting"]["why"], json!(format!("behind queued agent {two}")));
    }

    fn started_there(auto: &mut Auto) -> u64 {
        let id = auto.waits("8GB");
        auto.drive();
        auto.remote.rig.until("it starts there", |app| {
            app.panels.queued.queue().entry(id).is_some_and(|entry| matches!(entry.state, State::Started { .. }))
        });
        id
    }

    fn row_status(auto: &Auto, id: u64) -> String {
        let rows = auto.remote.rig.app.queued_rows();
        rows.into_iter().find(|row| row.id == id).map(|row| row.status).expect("the row")
    }

    #[test]
    fn a_run_there_is_followed_through_the_machine_s_snapshot_and_paused_while_it_is_down() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let id = started_there(&mut auto);
        let pane = PaneInfo { id: TAB + 1, status: Some("working".into()), ..PaneInfo::default() };
        let tab = TabInfo { id: TAB, panes: vec![pane], ..TabInfo::default() };
        let held = &mut auto.remote.rig.app.machines.held_mut()[0];
        let seen = held.seen.as_mut().expect("the machine's snapshot");
        seen.projects[0].workspaces[0].tabs = vec![tab];

        assert_eq!(row_status(&auto, id), "started on vps · working");

        let down = crate::machines::Status { state: crate::machines::State::Unreachable, ..Default::default() };
        auto.remote.rig.app.machines.set_status(0, down);

        assert_eq!(row_status(&auto, id), "started on vps · paused while vps is unreachable");
    }

    #[rstest]
    #[case::a_time(json!({ "when": "free", "at": "14:30", "prompt": "x" }), "an entry that waits for room has no time")]
    #[case::a_repeat(json!({ "when": "free", "repeat": "2h", "prompt": "x" }), "a routine has its own times")]
    #[case::a_machine_alone(json!({ "machine": "vps", "prompt": "x" }), "--machine goes with --when free")]
    #[case::another_kind(json!({ "when": "later", "prompt": "x" }), "`when` takes free")]
    fn refuses_what_does_not_go_with_waiting(#[case] entry: Value, #[case] why: &str) {
        let mut auto = auto(roomy, vec![web_shop()]);

        let (_, message) = auto.asks("queue", &json!({ "action": "add", "entry": entry })).expect_err("refused");

        assert!(message.starts_with(why), "{message}");
    }

    #[test]
    fn refuses_a_machine_it_does_not_know() {
        let mut auto = auto(roomy, vec![web_shop()]);
        let project = auto.project();
        let entry =
            json!({ "when": "free", "machine": "nowhere", "project": project, "prompt": "x", "agent": "claude" });

        let (code, _) = auto.asks("queue", &json!({ "action": "add", "entry": entry })).expect_err("refused");

        assert_eq!(code, "not-found");
    }

    #[test]
    fn counts_the_entries_waiting_for_room_and_opens_the_queue_on_a_click() {
        let mut auto = auto(roomy, vec![web_shop()]);
        auto.remote.rig.app.config.default_memory = "16GB".into();
        auto.remote.rig.app.set_queue_area(AREA);
        let project = auto.project();
        for _ in 0..2 {
            let entry = json!({ "when": "free", "project": project, "prompt": "fix it", "agent": "claude" });
            let text =
                json!({"contract": 1, "command": "queue", "args": {"action": "add", "entry": entry}}).to_string();
            auto.remote.rig.app.request(CLIENT, &text, Some(AREA), Instant::now());
        }
        let areas = app_areas(&auto.remote.rig.app);
        let button = ui::queue_button(areas.more, 2);

        let drawn = render(&mut auto.remote.rig.app, AREA).backend().to_string();
        assert!(drawn.contains("queue 2"), "{drawn}");

        click(&mut auto.remote.rig.app, button.as_position());
        assert!(matches!(auto.remote.rig.app.overlay, Some(Overlay::QueueList(_))));
    }
}

mod the_start_flow_dialog {
    use super::*;
    use crate::agents::Known;
    use crate::flows::{builtin, store};

    fn with_flows(auto: &mut Auto) {
        let dir = paths::flows(auto.remote.rig.app.config_file.path());
        let quick = builtin::flows(&[Known::Claude], Some(Known::Claude)).remove(0);
        for (name, memory) in [("Small", "2GB"), ("Big", "12GB")] {
            let flow = crate::flows::Flow { name: name.into(), memory: Some(memory.into()), ..quick.clone() };
            store::save(&dir, &flow, None).expect("save the flow");
        }
    }

    fn open_on(auto: &mut Auto, name: &str) {
        let app = &mut auto.remote.rig.app;
        let start = app.start_flow_here();
        app.open_start_flow(start);
        let Some(Overlay::StartFlow { start, .. }) = &mut app.overlay else { panic!("Start flow is not open") };
        let at = start.flows.iter().position(|saved| saved.flow.name == name).expect("the flow is listed");
        start.select(at);
    }

    fn wheel_to(auto: &mut Auto, name: &str) {
        let Some(Overlay::StartFlow { start, .. }) = &mut auto.remote.rig.app.overlay else {
            panic!("Start flow is not open")
        };
        let at = start.flows.iter().position(|saved| saved.flow.name == name).expect("the flow is listed");
        let delta = isize::try_from(at).expect("small") - isize::try_from(start.cursor).expect("small");
        start.scroll_list(delta);
    }

    fn choose_auto(auto: &mut Auto) {
        let app = &mut auto.remote.rig.app;
        let Some(Overlay::StartFlow { start, .. }) = &mut app.overlay else { panic!("Start flow is not open") };
        let at = start.run_on.as_ref().map(|run_on| run_on.targets.len());
        start.choose_machine(at);
        settle(auto);
    }

    fn settle(auto: &mut Auto) {
        let app = &mut auto.remote.rig.app;
        app.follow_start_flow_run_on();
        auto.remote.rig.until("auto settles", |app| {
            matches!(&app.overlay, Some(Overlay::StartFlow { start, .. })
                if start.run_on.as_ref().is_some_and(|run_on| !run_on.auto.wanted))
        });
    }

    fn row(auto: &Auto) -> (Option<String>, Option<(String, bool)>) {
        let Some(Overlay::StartFlow { start, .. }) = &auto.remote.rig.app.overlay else {
            panic!("Start flow is not open")
        };
        let run_on = start.run_on.as_ref().expect("the run on row");
        (run_on.machine().map(|target| target.name.clone()), run_on.auto.note.clone())
    }

    #[test]
    fn counts_the_selected_flow_s_memory_and_picks_again_when_the_flow_changes() {
        let mut auto = auto(roomy, vec![web_shop()]);
        with_flows(&mut auto);
        open_on(&mut auto, "Big");

        choose_auto(&mut auto);

        let refused = ("no machine has 12 GB free (best: vps, 11 GB)".to_string(), true);
        assert_eq!(row(&auto), (None, Some(refused)));

        wheel_to(&mut auto, "Small");
        settle(&mut auto);

        let picked = ("auto → vps (11 GB free, needs 2 GB)".to_string(), false);
        assert_eq!(row(&auto), (Some("vps".to_string()), Some(picked)));
    }
}
