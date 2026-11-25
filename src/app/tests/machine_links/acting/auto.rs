use super::*;
use crate::text::GB;

const SHOP: &str = "github.com/acme/web-shop";
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

fn web_shop() -> ProjectInfo {
    ProjectInfo {
        id: 1,
        name: "web-shop".into(),
        path: PathBuf::from("/home/ana/code/web-shop"),
        repo: Some(SHOP.into()),
        workspaces: vec![workspace()],
        ..ProjectInfo::default()
    }
}

struct Auto {
    remote: Remote,
    _dir: TempDir,
}

fn auto(answer: fn(&str) -> Value, projects: Vec<ProjectInfo>) -> Auto {
    let report = control::Report { boot: BOOT.into(), projects, ..control::Report::default() };
    let mut remote = seeing_in(Reply::Each(answer), report);
    let dir = TempDir::new();
    let path = dir.path().join("web-shop");
    std::fs::create_dir(&path).expect("the project's folder");
    let app = &mut remote.rig.app;
    app.open_project(path, AREA).expect("open the project");
    app.projects[0].repo.identity = Some(SHOP.into());
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
