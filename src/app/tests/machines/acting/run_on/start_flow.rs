use super::*;
use crate::agents::Known;
use crate::control::FlowSpec;
use crate::flows::builtin;
use crate::flows::start::{Start, There};

const QUICK: &str = "Quick";
const PROMPT: &str = "Read the task.\n\n  Then do it.";

fn flows() -> Value {
    let flows = builtin::flows(&[Known::Claude], Some(Known::Claude));
    let quick = flows.iter().find(|flow| flow.name == QUICK).expect("the Quick flow");
    let info = json!({ "name": QUICK, "origin": "builtin", "flow": FlowSpec::from(quick), "default": true });
    json!({ "flows": [info], "builtins": [QUICK] })
}

pub(super) fn running(command: &str) -> Structured {
    match command {
        "flow-list" => Structured::Ok(flows()),
        "flow-run" => Structured::Ok(json!({ "workspace": WORKSPACE })),
        command => ready(command),
    }
}

fn long_name() -> String {
    format!("{QUICK} {}", "x".repeat(250))
}

fn long_named(command: &str) -> Structured {
    match command {
        "flow-list" => {
            let mut listed = flows();
            let flow = &mut listed["flows"][0];
            flow["name"] = json!(long_name());
            flow["flow"]["name"] = json!(long_name());
            flow["flow"]["orchestrator"]["prompt"] = json!(PROMPT);
            Structured::Ok(listed)
        }
        command => running(command),
    }
}

fn refusing_the_run(command: &str) -> Structured {
    match command {
        "flow-run" => {
            Structured::Error(Refusal::new(Code::Failed, "a flow is already starting on branch add-dark-mode"))
        }
        command => running(command),
    }
}

pub(super) fn start(app: &App) -> &Start {
    let Some(Overlay::StartFlow { start, .. }) = &app.overlay else { panic!("Start flow is not open") };
    start
}

fn starting_flow(told: fn(&str) -> Structured, projects: Vec<ProjectInfo>) -> Reading {
    let mut reading = reading_with(Reply::Told(told), projects);
    let app = &mut reading.remote.rig.app;
    app.overlay = None;
    let start = app.start_flow_here();
    app.open_start_flow(start);
    reading
}

impl Reading {
    fn choose_vps(&mut self) {
        let app = &mut self.remote.rig.app;
        let ui::Overlay::StartFlow(view) = overlay_shown(app, AREA) else { panic!("Start flow is not shown") };
        let row = ui::start_run_on(AREA, view.shape());
        click(app, Position::new(row.x + 30, row.y));
    }

    fn flow_on_vps(&mut self) {
        self.choose_vps();
        self.remote.rig.until("the machine lists its flows and accounts", |app| {
            start(app).there().is_some_and(|there| there.listed) && app.accounts_there("vps") != Checked::Waiting
        });
    }

    fn run(&mut self, prompt: &str) {
        let app = &mut self.remote.rig.app;
        type_text(app, prompt);
        send_key(app, KeyCode::Tab, KeyModifiers::NONE);
        send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    }

    pub(super) fn flow_from_the_issue(&mut self) {
        self.press("start a flow");
        self.remote.rig.until("the machine lists its flows and accounts", |app| {
            start(app).there().is_some_and(|there| there.listed && there.accounted)
        });
        let app = &mut self.remote.rig.app;
        send_key(app, KeyCode::Tab, KeyModifiers::NONE);
        send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    }

    fn run_answered(&mut self) {
        self.remote.rig.until("the machine answers the run", |app| match &app.overlay {
            Some(Overlay::StartFlow { start, .. }) => !start.waiting,
            _ => true,
        });
    }
}

#[rstest::rstest]
#[case::without_an_issue(None, None)]
#[case::from_an_issue(Some(("#7", "github")), Some(json!({ "project": 1 })))]
fn asks_the_machine_for_its_trackers_only_from_an_issue(
    #[case] issue: Option<(&str, &str)>,
    #[case] sources: Option<Value>,
) {
    let mut reading = starting_flow(running, vec![web_shop()]);
    if let Some(Overlay::StartFlow { start, .. }) = &mut reading.remote.rig.app.overlay {
        start.asked = issue.map(|(key, source)| (key.into(), source.into()));
    }

    reading.flow_on_vps();

    let no_issue = issue.is_none();
    reading.remote.rig.until("the machine answers the checks", |app| {
        app.accounts_there("vps") != Checked::Waiting
            && (no_issue || app.tracker_there("vps", 1, "github") != Checked::Waiting)
    });
    assert_eq!((reading.sent("issue-sources"), reading.sent("flow-list")), (sources, Some(json!({ "project": 1 }))));
}

#[test]
fn lists_the_machine_s_flows_and_hides_edit() {
    let mut reading = starting_flow(running, vec![web_shop()]);

    reading.flow_on_vps();

    let app = &mut reading.remote.rig.app;
    let names: Vec<&str> = start(app).flows.iter().map(|saved| saved.flow.name.as_str()).collect();
    assert_eq!(names, [QUICK]);
    insta::assert_snapshot!(dialog(app, WIDE));
}

#[test]
fn sends_the_run_with_the_machine_s_project_and_closes_once_it_started() {
    let mut reading = starting_flow(running, vec![web_shop()]);
    reading.flow_on_vps();

    reading.run("Add dark mode");

    assert_eq!(start(&reading.remote.rig.app).starting_there().as_deref(), Some("starting on vps…"));
    reading.run_answered();
    let sent = reading.sent("flow-run").expect("a flow run");
    assert_eq!(
        (&sent["name"], &sent["prompt"], &sent["worktree"], &sent["project"]),
        (&json!(QUICK), &json!("Add dark mode"), &json!("add-dark-mode"), &json!(1))
    );
    assert_eq!((&sent["focus"], sent.get("machine"), sent.get("issue")), (&json!(false), None, Some(&Value::Null)));
    assert!(reading.remote.rig.app.overlay.is_none());
}

#[test]
fn runs_a_long_named_flow_by_its_whole_name_and_keeps_the_lines_of_its_prompt() {
    let mut reading = starting_flow(long_named, vec![web_shop()]);
    reading.flow_on_vps();

    reading.run("Add dark mode");
    reading.run_answered();

    assert_eq!(reading.sent("flow-run").expect("a flow run")["name"], json!(long_name()));
    let Checked::Read(listing) = reading.remote.rig.app.flows_there("vps", 1) else { panic!("no flows") };
    assert_eq!(listing.flows[0].flow.orchestrator.prompt, PROMPT);
}

#[test]
fn keeps_the_machine_s_refusal_in_red() {
    let mut reading = starting_flow(refusing_the_run, vec![web_shop()]);
    reading.flow_on_vps();

    reading.run("Add dark mode");
    reading.run_answered();

    let app = &reading.remote.rig.app;
    assert_eq!(start(app).notice.as_deref(), Some("a flow is already starting on branch add-dark-mode"));
}

#[test]
fn opens_the_clone_dialog_when_the_project_is_not_there() {
    let mut reading = starting_flow(running, Vec::new());
    let app = &mut reading.remote.rig.app;
    app.projects[0].repo.remotes = vec![("origin".into(), "https://github.com/acme/web-shop.git".into())];
    reading.choose_vps();
    reading.remote.rig.until("the machine answers", |app| start(app).there().is_some());

    reading.run("Add dark mode");

    assert!(matches!(reading.remote.rig.app.overlay, Some(Overlay::CloneOnMachine(_))));
}

#[test]
fn lists_the_flows_of_a_project_the_machine_lists_later() {
    let mut reading = starting_flow(running, Vec::new());
    reading.choose_vps();
    reading.remote.rig.until("the machine answers its accounts", |app| app.accounts_there("vps") != Checked::Waiting);

    serve(&mut reading.remote.theirs, 2, shop(vec![web_shop()]));

    reading.remote.rig.until("the machine lists its flows", |app| start(app).there().is_some_and(|there| there.listed));
    assert_eq!(reading.sent("flow-list"), Some(json!({ "project": 1 })));
}

#[test]
fn says_the_project_left_the_machine_instead_of_starting_nothing_in_silence() {
    let mut reading = starting_flow(running, vec![web_shop()]);
    reading.flow_on_vps();
    serve(&mut reading.remote.theirs, 2, shop(Vec::new()));
    reading
        .remote
        .rig
        .until("the project leaves the machine", |app| start(app).there().is_some_and(|there| !there.cloned));
    if let Some(Overlay::StartFlow { start, .. }) = &mut reading.remote.rig.app.overlay {
        start.there = start.there.take().map(|there| There { cloned: true, ..there });
    }

    reading.run("Add dark mode");

    let app = &reading.remote.rig.app;
    assert_eq!((start(app).notice.as_deref(), start(app).waiting), (Some("the project is not on vps"), false));
    assert_eq!(reading.sent("flow-run"), None);
}

#[test]
fn from_an_issue_keeps_the_machine_and_sends_the_issue() {
    let mut reading = reading_with(Reply::Told(running), vec![web_shop()]);
    reading.checked();

    reading.flow_from_the_issue();
    reading.run_answered();

    let sent = reading.sent("flow-run").expect("a flow run");
    assert_eq!((&sent["issue"], &sent["source"], &sent["project"]), (&json!("#7"), &json!("github"), &json!(1)));
    assert_eq!(sent["prompt"], json!(""));
}

#[test]
fn from_an_issue_runs_with_the_account_picked_on_the_machine() {
    let mut reading = reading_with(Reply::Told(running), vec![web_shop()]);
    reading.checked();
    reading.pick("account…", "home");

    reading.flow_from_the_issue();
    reading.run_answered();

    assert_eq!(reading.sent("flow-run").expect("a flow run")["accounts"], json!({ "claude": "home" }));
}
