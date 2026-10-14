use serde_json::json;

use super::*;

fn read(pane: u64) -> Command {
    Command::Read(Read { pane: Some(pane), ..Read::default() })
}

#[test]
fn a_request_names_its_command_next_to_its_arguments() {
    let request = Request { caller: Some(7), server: Some("1-2".into()), command: read(3) };

    let sent = serde_json::to_value(&request).expect("json");

    assert_eq!(
        sent,
        json!({"caller": 7, "server": "1-2", "command": "read", "args": {"pane": 3, "tab": null, "lines": null}})
    );
}

#[test]
fn fields_from_a_newer_command_are_ignored() {
    let text = r#"{"caller": 7, "command": "read", "args": {"pane": 3, "colour": "red"}, "sent_at": 1}"#;

    let request: Request = serde_json::from_str(text).expect("a request");

    assert_eq!(request, Request { caller: Some(7), server: None, command: read(3) });
}

#[test]
fn fields_an_older_command_lacks_take_their_defaults() {
    let request: Request = serde_json::from_str(r#"{"command": "new-tab", "args": {}}"#).expect("a request");

    assert_eq!(request.command, Command::NewTab(NewTab::default()));
}

#[test]
fn every_command_is_listed_by_name() {
    let commands = [
        Command::Status(Status::default()),
        Command::Open(Open::default()),
        Command::NewWorkspace(NewWorkspace::default()),
        Command::NewTab(NewTab::default()),
        Command::Split(Split::default()),
        Command::Start(Start::default()),
        Command::Send(SendText::default()),
        Command::Keys(Keys::default()),
        read(1),
        Command::Wait(Wait::default()),
        Command::Close(Close { item: Item::Tab(1), remove_worktree: false, force: false }),
        Command::Rename(Rename::default()),
        Command::Focus(Focus { item: Item::Pane(1) }),
        Command::Notify(Notify::default()),
        Command::Todo(Todo::List),
        Command::WaitSeveral(Wait::default()),
        Command::Events(Events::default()),
        Command::LastMessage(LastMessage::default()),
        Command::RestartWhenIdle(RestartWhenIdle::default()),
        Command::Stats(Stats::default()),
        Command::Away(Away::default()),
        Command::Mark(Mark::default()),
        Command::StartWith(Start::default()),
        Command::FlowList(FlowList::default()),
        Command::FlowRun(FlowRun::default()),
    ];

    let names: Vec<String> = commands
        .iter()
        .map(|command| {
            let request = Request { caller: None, server: None, command: command.clone() };
            let sent = serde_json::to_value(request).expect("json");
            command_name(&sent).expect("a name").to_string()
        })
        .collect();

    assert_eq!(names, Command::NAMES);
    assert_eq!(commands.iter().map(Command::name).collect::<Vec<_>>(), Command::NAMES);
}

#[test]
fn the_fields_to_log_leave_out_what_is_typed() {
    let secret = "hunter2-marker";
    let commands = [
        Command::Send(SendText { text: Some(secret.into()), enter: true, ..SendText::default() }),
        Command::Keys(Keys { keys: vec![secret.into()], ..Keys::default() }),
        Command::Start(Start { prompt: Some(secret.into()), ..Start::default() }),
        Command::NewTab(NewTab { command: Some(secret.into()), ..NewTab::default() }),
        Command::Split(Split { command: Some(secret.into()), ..Split::default() }),
        Command::Wait(Wait { until: Until::Text(secret.into()), ..Wait::default() }),
        Command::WaitSeveral(Wait { until: Until::Text(secret.into()), panes: vec![3, 4], ..Wait::default() }),
        Command::Notify(Notify { text: secret.into() }),
        Command::Todo(Todo::Add(secret.into())),
        Command::FlowRun(FlowRun { name: "Quick".into(), prompt: secret.into(), ..FlowRun::default() }),
    ];

    for command in commands {
        let fields = command.fields();
        assert!(fields.iter().all(|(_, value)| !value.contains(secret)), "{fields:?}");
    }
}

mod workspace_on {
    use super::*;

    fn workspace(id: u64, name: &str, branch: Option<&str>, worktree: bool) -> WorkspaceInfo {
        WorkspaceInfo {
            id,
            name: name.into(),
            branch: branch.map(str::to_string),
            worktree,
            ..WorkspaceInfo::default()
        }
    }

    fn project(id: u64, name: &str, workspaces: Vec<WorkspaceInfo>) -> ProjectInfo {
        ProjectInfo { id, name: name.into(), workspaces, ..ProjectInfo::default() }
    }

    fn shop() -> ProjectInfo {
        project(
            1,
            "shop",
            vec![
                workspace(2, "default", Some("main"), false),
                workspace(3, "fix/login", Some("fix/login"), true),
                workspace(4, "experiments", Some("main"), false),
            ],
        )
    }

    fn api(caller: bool) -> ProjectInfo {
        let mut api = project(5, "api", vec![workspace(6, "fix/login", Some("fix/login"), true)]);
        let pane = PaneInfo { id: 8, caller, ..PaneInfo::default() };
        api.workspaces[0].tabs = vec![TabInfo { id: 7, panes: vec![pane], ..TabInfo::default() }];
        api
    }

    fn shown(project: u64, projects: Vec<ProjectInfo>) -> Report {
        Report { shown: Ids { project: Some(project), ..Ids::default() }, projects, ..Report::default() }
    }

    #[test]
    fn finds_the_workspace_on_the_branch_in_the_shown_project() {
        let report = shown(1, vec![api(false), shop()]);

        assert_eq!(report.workspace_on("fix/login"), Ok(3));
    }

    #[test]
    fn looks_in_the_project_of_the_pane_that_asks_before_the_shown_one() {
        let report = shown(1, vec![shop(), api(true)]);

        assert_eq!(report.workspace_on("fix/login"), Ok(6));
    }

    #[test]
    fn a_missing_branch_lists_the_workspaces_of_the_project() {
        let report = shown(1, vec![shop(), api(false)]);

        assert_eq!(
            report.workspace_on("feat/x"),
            Err("no workspace of project 1 (shop) is on branch feat/x; its workspaces:\n\
                     \x20 workspace 2  default  branch main\n\
                     \x20 workspace 3  fix/login  branch fix/login  worktree\n\
                     \x20 workspace 4  experiments  branch main"
                .into())
        );
    }

    #[test]
    fn a_branch_in_several_workspaces_lists_them() {
        let report = shown(1, vec![shop()]);

        assert_eq!(
            report.workspace_on("main"),
            Err("branch main is checked out in 2 workspaces of project 1 (shop); pick one with --workspace:\n\
                     \x20 workspace 2  default  branch main\n\
                     \x20 workspace 4  experiments  branch main"
                .into())
        );
    }

    #[rstest::rstest]
    #[case::no_project(shown(1, vec![]), "no project is open")]
    #[case::no_workspace(shown(9, vec![project(9, "empty", vec![])]), "project 9 (empty) has no workspace")]
    fn says_why_nothing_is_found(#[case] report: Report, #[case] expected: &str) {
        let error = report.workspace_on("main").expect_err("nothing found");

        assert!(error.starts_with(expected), "{error}");
    }
}

#[test]
fn the_columns_of_a_pane_row_are_its_keys_in_order() {
    let row = PaneRow::default();

    let text = serde_json::to_string(&row).expect("json");

    let keys = serde_json::to_value(&row).expect("json").as_object().expect("an object").len();
    let at: Vec<usize> = PaneRow::COLUMNS
        .iter()
        .map(|column| text.find(&format!("\"{column}\":")).unwrap_or_else(|| panic!("no {column} in {text}")))
        .collect();
    assert_eq!(keys, PaneRow::COLUMNS.len());
    assert!(at.is_sorted(), "{text}");
}

#[test]
fn new_columns_of_a_pane_row_go_after_the_released_ones() {
    let released = "pane tab workspace project program agent status background_shell dialog at_prompt model \
                        context caller shown active tab_name workspace_name branch worktree project_name path";

    assert!(PaneRow::COLUMNS.join(" ").starts_with(released), "{:?}", PaneRow::COLUMNS);
}

#[test]
fn turn_over_is_spelt_as_on_the_command_line() {
    let wait = Command::Wait(Wait { until: Until::TurnOver, ..Wait::default() });

    let sent = serde_json::to_value(Request { caller: None, server: None, command: wait }).expect("json");

    assert_eq!(sent["args"]["until"], json!("turn-over"));
}

#[test]
fn an_event_names_what_happened_next_to_its_ids() {
    let event = Event {
        time: "2026-10-08T12:00:00.000Z".into(),
        what: What::Status { agent: "claude".into(), from: None, to: Some("working".into()) },
        ids: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
    };

    let sent = serde_json::to_value(&event).expect("json");

    assert_eq!(
        sent,
        json!({
            "time": "2026-10-08T12:00:00.000Z", "event": "status", "agent": "claude", "from": null,
            "to": "working", "project": 1, "workspace": 2, "tab": 3, "pane": 4
        })
    );
    assert_eq!(serde_json::from_value::<Event>(sent).expect("an event"), event);
}

#[test]
fn an_event_streams_as_one_more_answer() {
    let event = Event {
        time: "2026-10-08T12:00:00.000Z".into(),
        what: What::Closed { kind: Kind::Tab },
        ids: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: None },
    };

    let text = event.streamed().expect("json");

    let Ok(Response::Ok(value)) = serde_json::from_str(&text) else { panic!("not an answer: {text}") };
    assert_eq!(
        (value["event"].as_str(), value["kind"].as_str(), value.get("pane")),
        (Some("closed"), Some("tab"), None)
    );
}

#[test]
fn an_answer_carries_only_the_ids_it_has() {
    let done = Done { ids: Ids { pane: Some(4), ..Ids::default() }, ended: Some("idle".into()), ..Done::default() };

    assert_eq!(serde_json::to_value(done).expect("json"), json!({"pane": 4, "ended": "idle"}));
}
