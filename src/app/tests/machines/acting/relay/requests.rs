use super::*;
use crate::control::TabInfo;

const ASKER: u64 = 11;
const PROJECT: u64 = 1;
const LEAD: u64 = 21;
const CODER: u64 = 22;

fn made() -> Value {
    json!({ "project": PROJECT, "workspace": NEW })
}

fn start() -> Value {
    json!({ "agent": "claude", "project": PROJECT, "prompt": "fix it", "machine": "VPS", "focus": true })
}

fn new_workspace() -> Value {
    json!({ "name": "web", "project": PROJECT, "worktree": true, "machine": "VPS", "focus": true })
}

fn flow_run() -> Value {
    json!({ "name": "Pair", "prompt": "fix it", "project": PROJECT, "machine": "VPS", "focus": true })
}

fn on(name: &str, mut args: Value) -> Value {
    args["machine"] = json!(name);
    args
}

fn without_focus(mut args: Value) -> Value {
    args["focus"] = json!(false);
    args
}

fn flow_made() -> Value {
    json!({ "project": PROJECT, "workspace": NEW, "members": [{ "role": "orchestrator", "tab": LEAD }, { "tab": CODER }] })
}

fn with_team(boot: &str) -> control::Report {
    let tab = |id, active| TabInfo { id, active, ..TabInfo::default() };
    let mut report = with_new(boot);
    report.projects[0].workspaces[1].tabs = vec![tab(LEAD, false), tab(CODER, true)];
    report
}

impl Remote {
    fn ask(&mut self, command: &str, args: &Value) -> std::result::Result<Value, (String, String)> {
        self.put(None, command, args);
        self.answered()
    }

    fn put(&mut self, view: Option<u64>, command: &str, args: &Value) {
        let text = json!({ "contract": 1, "command": command, "args": args, "view": view }).to_string();
        self.rig.app.request(ASKER, &text, Some(AREA), Instant::now());
    }

    fn answered(&mut self) -> std::result::Result<Value, (String, String)> {
        let mut answer = None;
        let (app, rx) = (&mut self.rig.app, &self.rig.rx);
        wait_until("the request is answered", || {
            drain(app, rx);
            answer = app.take_answers().pop().map(|(_, text)| text);
            answer.is_some()
        });
        replied(&answer.expect("an answer"))
    }
}

mod a_start_on_a_machine {
    use super::*;

    #[rstest]
    #[case::start("start", start())]
    #[case::new_workspace("new-workspace", new_workspace())]
    #[case::flow_run("flow-run", flow_run())]
    fn reaches_it_with_its_own_ids_and_boot_without_its_name_or_the_focus(#[case] command: &str, #[case] args: Value) {
        let mut remote = seeing(Reply::Agree);

        remote.ask(command, &args).expect("an answer");

        let asked = remote.asked();
        let [asked] = &asked[..] else { panic!("one question, not {asked:?}") };
        assert_eq!((asked["command"].as_str(), asked["boot"].as_str()), (Some(command), Some(BOOT)));
        let sent = &asked["args"];
        assert_eq!((sent.get("machine"), &sent["focus"], &sent["project"]), (None, &json!(false), &json!(PROJECT)));
    }

    #[rstest]
    #[case::start("start", start())]
    #[case::new_workspace("new-workspace", new_workspace())]
    #[case::flow_run("flow-run", flow_run())]
    fn answers_what_it_made_and_its_name(#[case] command: &str, #[case] args: Value) {
        let mut remote = seeing(Reply::Made(made));

        let answer = remote.ask(command, &args);

        assert_eq!(answer, Ok(json!({ "project": PROJECT, "workspace": NEW, "machine": "vps" })));
    }

    #[test]
    fn keeps_the_code_it_refused_with() {
        let mut remote = seeing(Reply::Restarted);

        let answer = remote.ask("start", &start());

        let why = "this request is for the cornercase server started as vps-1";
        assert_eq!(answer, Err(("stale-boot".into(), why.into())));
    }

    #[test]
    fn refuses_a_machine_that_is_not_listed() {
        let mut remote = seeing(Reply::Agree);

        let (code, why) = remote.ask("new-workspace", &on("box", new_workspace())).expect_err("a refusal");

        assert_eq!((code.as_str(), why.contains("`box`")), ("not-found", true));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn refuses_a_machine_that_is_not_linked() {
        let mut remote = seeing(Reply::Agree);
        remote.falls();

        let answer = remote.ask("flow-run", &flow_run());

        assert_eq!(answer, Err(("unavailable".into(), "vps unreachable".into())));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[rstest]
    #[case::wait(json!({ "wait": true }))]
    #[case::until(json!({ "until": "idle" }))]
    #[case::timeout(json!({ "timeout": 30.0 }))]
    fn refuses_to_wait_for_the_agent(#[case] waits: Value) {
        let mut remote = seeing(Reply::Agree);
        let mut args = start();
        args.as_object_mut().expect("an object").extend(waits.as_object().expect("an object").clone());

        let (code, why) = remote.ask("start", &args).expect_err("a refusal");

        assert_eq!((code.as_str(), why.contains("`machine`")), ("malformed", true));
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}

mod its_focus {
    use super::*;

    #[test]
    fn selects_what_the_machine_made_once_its_snapshot_lists_it() {
        let mut remote = seeing(Reply::Made(made));

        remote.ask("new-workspace", &new_workspace()).expect("an answer");
        assert_eq!(remote.selected(), None);
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), Some(NEW));
    }

    #[test]
    fn of_a_flow_run_selects_its_orchestrator() {
        let mut remote = seeing(Reply::Made(flow_made));

        remote.ask("flow-run", &flow_run()).expect("an answer");
        remote.serves(2, with_team(BOOT));

        let shown = remote.rig.app.shown_remote().expect("a machine's tab shown");
        assert_eq!((shown.workspace, shown.tab), (NEW, Some(LEAD)));
    }

    #[rstest]
    #[case::a_workspace_of_the_machine(Some(WORKSPACE))]
    #[case::one_of_this_computer(None)]
    fn is_dropped_when_something_is_picked_before_the_answer(#[case] picked: Option<u64>) {
        let mut remote = seeing(Reply::Made(made));

        remote.put(None, "new-workspace", &new_workspace());
        match picked {
            Some(workspace) => assert!(remote.rig.app.select_remote("vps", RemoteItem::Workspace(workspace))),
            None => remote.rig.app.leave_remote(),
        }
        remote.answered().expect("an answer");
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), picked);
    }

    #[rstest]
    #[case::start("start", start())]
    #[case::new_workspace("new-workspace", new_workspace())]
    fn takes_no_view(#[case] command: &str, #[case] args: Value) {
        let mut remote = seeing(Reply::Made(made));

        remote.put(Some(1), command, &args);
        let (code, why) = remote.answered().expect_err("a refusal");

        assert_eq!((code.as_str(), why.contains("the window's")), ("malformed", true), "{why}");
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn is_left_alone_when_not_asked_for() {
        let mut remote = seeing(Reply::Made(made));

        remote.ask("new-workspace", &without_focus(new_workspace())).expect("an answer");
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), None);
    }
}
