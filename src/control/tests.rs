use crate::issues::Secret;
use serde_json::json;

use super::*;

fn read(pane: u64) -> Command {
    Command::Read(Read { pane: Some(pane), ..Read::default() })
}

#[test]
fn a_request_names_its_command_next_to_its_arguments() {
    let request = Request {
        caller: Some(7),
        server: Some("1-2".into()),
        contract: None,
        boot: None,
        view: None,
        command: read(3),
    };

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

    assert_eq!(
        request,
        Request { caller: Some(7), server: None, contract: None, boot: None, view: None, command: read(3) }
    );
}

#[test]
fn fields_an_older_command_lacks_take_their_defaults() {
    let request: Request = serde_json::from_str(r#"{"command": "new-tab", "args": {}}"#).expect("a request");

    assert_eq!(request.command, Command::NewTab(NewTab::default()));
}

#[test]
fn every_command_is_listed_by_name() {
    let commands = vec![
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
        Command::Close(Close { item: Item::Tab(1), remove_worktree: false, force: false, unlock: false }),
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
        Command::Queue(Queue::List),
        Command::StartWith(Start::default()),
        Command::FlowList(FlowList::default()),
        Command::FlowRun(FlowRun::default()),
        Command::FlowExport(FlowExport::default()),
        Command::FlowImport(FlowImport::default()),
        Command::Search(Search::default()),
        Command::Contract(Contract::default()),
        Command::StartServer(StartServer::default()),
        Command::Watch(Watch::default()),
        Command::Surface(Surface::default()),
        Command::View(OpenView::default()),
        Command::ViewSet(SetView::default()),
        Command::Input(Input::default()),
        Command::Scroll(Scroll::default()),
        Command::Selection(Selection::default()),
        Command::Resize(Resize::default()),
        Command::Ratio(Ratio::default()),
        Command::Link(Link::default()),
        Command::TodoSend(TodoSend::default()),
        Command::Theme(Theme::default()),
        Command::Go(Go::default()),
        Command::Redraw(Redraw::default()),
        Command::Config(Configure::default()),
        Command::Move(Move { item: Item::Tab(1), before: None, into: None, ungroup: false }),
        Command::MovePane(MovePane::default()),
        Command::Collapse(Fold { item: Item::Group(1) }),
        Command::Expand(Fold { item: Item::Group(1) }),
        Command::GroupNew(GroupNew::default()),
        Command::GroupStyle(GroupStyle::default()),
        Command::GroupDelete(GroupDelete::default()),
        Command::RemovalCheck(RemovalCheck::default()),
        Command::IssueSources(IssueSources::default()),
        Command::IssueList(IssueList::default()),
        Command::IssueRead(IssueRead::default()),
        Command::IssuePeople(IssuePeople::default()),
        Command::UpdateCheck(UpdateCheck::default()),
        Command::UpdateInstall(UpdateInstall::default()),
        Command::App(AppWatch::default()),
        Command::Changes(Changes::default()),
        Command::ChangesMode(ChangesMode::default()),
        Command::ChangesFile(ChangesFile::default()),
        Command::ChangesGap(ChangesGap::default()),
        Command::ChangesBase(ChangesBase::default()),
        Command::ChangesViewed(ChangesViewed::default()),
        Command::Usage(Usage::default()),
        Command::PluginList(PluginList::default()),
        Command::PluginActions(PluginActions::default()),
        Command::PluginRun(PluginRun { plugin: "a".into(), action: "b".into(), revision: 0, item: Item::Pane(1) }),
    ];

    let names: Vec<String> = commands
        .iter()
        .map(|command| {
            let request = Request {
                caller: None,
                server: None,
                contract: None,
                boot: None,
                view: None,
                command: command.clone(),
            };
            let sent = serde_json::to_value(request).expect("json");
            command_name(&sent).expect("a name").to_string()
        })
        .collect();

    assert_eq!(names, Command::NAMES);
    assert_eq!(commands.iter().map(Command::name).collect::<Vec<_>>(), Command::NAMES);
}

#[test]
fn a_todo_send_names_the_item_and_its_target_and_logs_no_text() {
    let request: Request =
        serde_json::from_str(r#"{"command": "todo-send", "args": {"id": 3, "pane": 5, "force": true}}"#)
            .expect("a request");

    assert_eq!(request.command, Command::TodoSend(TodoSend { id: 3, pane: Some(5), tab: None, force: true }));
    assert_eq!(
        request.command.fields(),
        [
            ("todo", "send".to_string()),
            ("id", "3".to_string()),
            ("pane", "5".to_string()),
            ("force", "true".to_string())
        ]
    );
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
        Command::Search(Search { query: secret.into() }),
        Command::Todo(Todo::Add(secret.into())),
        Command::Todo(Todo::Edit { id: 3, text: secret.into() }),
        Command::FlowRun(FlowRun { name: "Quick".into(), prompt: secret.into(), ..FlowRun::default() }),
        Command::Input(Input {
            events: vec![terminal::InputEvent::Text(secret.into()), terminal::InputEvent::Paste(secret.into())],
            ..Input::default()
        }),
        Command::Config(Configure::Secret(config::SecretChange { key: "linear".into(), value: Secret(secret.into()) })),
        Command::Config(Configure::Set([("jira_jql".to_string(), Value::String(secret.into()))].into())),
        Command::Config(Configure::Project(config::ProjectChange {
            setup: Some(secret.into()),
            teardown: Some(secret.into()),
            ..config::ProjectChange::default()
        })),
    ];

    for command in commands {
        let fields = command.fields();
        assert!(fields.iter().all(|(_, value)| !value.contains(secret)), "{fields:?}");
    }
}

#[test]
fn a_search_travels_as_its_query_and_logs_only_its_size() {
    let request = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        command: Command::Search(Search { query: "checkout".into() }),
    };
    let sent = serde_json::to_string(&request).expect("json");

    assert_eq!(sent, r#"{"caller":null,"server":null,"command":"search","args":{"query":"checkout"}}"#);
    assert_eq!(serde_json::from_str::<Request>(&sent).expect("a request"), request);
    assert_eq!(request.command.fields(), [("bytes", "8".to_string())]);
}

#[test]
fn a_search_answer_an_older_server_lacks_fields_of_still_reads() {
    let found: Matches = serde_json::from_str(r#"{"matches":[{"pane":4,"text":"hit"}]}"#).expect("matches");

    assert_eq!(found.matches[0], Hit { pane: 4, text: "hit".into(), ..Hit::default() });
    assert!(!found.capped);
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
                        context caller shown active tab_name workspace_name branch worktree project_name path survey \
                        resuming";

    assert!(PaneRow::COLUMNS.join(" ").starts_with(released), "{:?}", PaneRow::COLUMNS);
}

#[test]
fn turn_over_is_spelt_as_on_the_command_line() {
    let wait = Command::Wait(Wait { until: Until::TurnOver, ..Wait::default() });

    let sent = serde_json::to_value(Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        command: wait,
    })
    .expect("json");

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

#[test]
fn an_imports_logged_fields_carry_its_size_and_never_its_text() {
    let import = FlowImport { text: "name = \"SECRET\"".into(), name: Some("Mine".into()), ..FlowImport::default() };
    let fields = Command::FlowImport(import).fields();
    assert!(fields.contains(&("text_bytes", "15".to_string())), "{fields:?}");
    assert!(fields.iter().all(|(_, value)| !value.contains("SECRET")), "{fields:?}");
}

mod events_v1 {
    use rstest::rstest;

    use super::*;
    use crate::test_util::fixture;

    const TIME: &str = "2026-10-16T09:30:00.123Z";

    fn event(what: What, ids: Ids) -> Event {
        Event { time: TIME.into(), what, ids }
    }

    fn pane() -> Ids {
        Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) }
    }

    fn samples() -> [(Event, &'static str); 10] {
        [
            (
                event(What::Opened { kind: Kind::Tab, parent: None }, Ids { pane: None, ..pane() }),
                fixture!("control/events/v1/opened.json"),
            ),
            (event(What::Closed { kind: Kind::Pane }, pane()), fixture!("control/events/v1/closed.json")),
            (
                event(
                    What::Status { agent: "claude".into(), from: Some("working".into()), to: Some("waiting".into()) },
                    pane(),
                ),
                fixture!("control/events/v1/status.json"),
            ),
            (event(What::Exited { program: "cargo".into() }, pane()), fixture!("control/events/v1/exited.json")),
            (event(What::Dropped { count: 12 }, Ids::default()), fixture!("control/events/v1/dropped.json")),
            (
                event(What::Clipboard { text: "hello\nworld".into() }, pane()),
                fixture!("control/events/v1/clipboard.json"),
            ),
            (event(What::Bell { count: 3 }, pane()), fixture!("control/events/v1/bell.json")),
            (
                event(
                    What::Notice {
                        token: "cc-ab12cd-3".into(),
                        agent: "claude".into(),
                        status: "waiting".into(),
                        place: "shop › default".into(),
                        message: "claude needs you in shop › default".into(),
                        panes: vec![4],
                        sound: None,
                    },
                    Ids { pane: None, tab: None, ..pane() },
                ),
                fixture!("control/events/v1/notice.json"),
            ),
            (
                event(What::Opened { kind: Kind::Pane, parent: Some(2) }, pane()),
                fixture!("control/events/v1/opened-parent.json"),
            ),
            (
                event(
                    What::Notice {
                        token: "cc-ab12cd-4".into(),
                        agent: "claude".into(),
                        status: "done".into(),
                        place: "shop › default".into(),
                        message: "claude finished in shop › default".into(),
                        panes: vec![4],
                        sound: Some("finished".into()),
                    },
                    Ids { pane: None, tab: None, ..pane() },
                ),
                fixture!("control/events/v1/notice-sound.json"),
            ),
        ]
    }

    #[test]
    fn the_fixtures_are_of_the_events_version_the_contract_names() {
        assert_eq!(contract::EVENTS_VERSION, 1);
    }

    #[rstest]
    #[case::opened(0)]
    #[case::closed(1)]
    #[case::status(2)]
    #[case::exited(3)]
    #[case::dropped(4)]
    #[case::clipboard(5)]
    #[case::bell(6)]
    #[case::opened_from_a_pane(8)]
    #[case::notice(7)]
    #[case::notice_with_a_sound(9)]
    fn an_event_is_written_as_its_fixture(#[case] index: usize) {
        let (event, fixture) = &samples()[index];

        assert_eq!(serde_json::to_string(event).expect("json"), fixture.trim_end());
    }

    #[rstest]
    #[case::opened(0)]
    #[case::closed(1)]
    #[case::status(2)]
    #[case::exited(3)]
    #[case::dropped(4)]
    #[case::clipboard(5)]
    #[case::bell(6)]
    #[case::opened_from_a_pane(8)]
    #[case::notice(7)]
    #[case::notice_with_a_sound(9)]
    fn a_fixture_reads_back_as_its_event(#[case] index: usize) {
        let (event, fixture) = &samples()[index];

        assert_eq!(&serde_json::from_str::<Event>(fixture).expect("an event"), event);
    }

    #[rstest]
    #[case::opened(0, false)]
    #[case::status(2, false)]
    #[case::dropped(4, false)]
    #[case::clipboard(5, true)]
    #[case::bell(6, true)]
    fn only_what_programs_ask_is_a_host_event(#[case] index: usize, #[case] host: bool) {
        let (event, _) = &samples()[index];

        assert_eq!(event.host(), host);
        assert_eq!(event.kind() == Some(Kind::Pane), host || index == 2);
    }

    #[test]
    fn a_streamed_event_is_its_fixture_under_ok() {
        let (event, fixture) = &samples()[2];

        let line: Value = serde_json::from_str(&event.streamed().expect("a line")).expect("json");

        assert_eq!(line, json!({"ok": serde_json::from_str::<Value>(fixture).expect("json")}));
    }
}

mod answers {
    use rstest::rstest;

    use super::*;
    use crate::control::contract::{Code, Refusal};
    use crate::test_util::fixture;

    const MISSING: &str = "there is no pane 9; `cornercase status` lists them";

    fn found() -> Value {
        json!({"pane": 4, "tab": 3})
    }

    #[rstest]
    #[case::legacy_ok(Ok(found()), false, fixture!("control/responses/legacy-ok.json"))]
    #[case::legacy_error(Err(Refusal::new(Code::NotFound, MISSING)), false, fixture!("control/responses/legacy-error.json"))]
    #[case::structured_ok(Ok(found()), true, fixture!("control/responses/structured-ok.json"))]
    #[case::structured_error(
        Err(Refusal::new(Code::NotFound, MISSING)),
        true,
        fixture!("control/responses/structured-error.json")
    )]
    fn an_answer_is_written_as_its_fixture(
        #[case] reply: Result<Value, Refusal>,
        #[case] structured: bool,
        #[case] fixture: &str,
    ) {
        assert_eq!(contract::render(reply, structured).expect("an answer"), fixture.trim_end());
    }

    #[test]
    fn a_legacy_answer_reads_back_as_a_response() {
        let ok: Response = serde_json::from_str(fixture!("control/responses/legacy-ok.json")).expect("ok");
        let error: Response = serde_json::from_str(fixture!("control/responses/legacy-error.json")).expect("error");

        assert_eq!((ok, error), (Response::Ok(found()), Response::Error(MISSING.into())));
    }

    #[test]
    fn a_structured_error_reads_back_with_its_code() {
        let text = fixture!("control/responses/structured-error.json");

        let error: contract::Structured = serde_json::from_str(text).expect("an error");

        assert_eq!(error, contract::Structured::Error(Refusal::new(Code::NotFound, MISSING)));
    }
}

mod watch_v1 {
    use super::*;
    use crate::project::split::{Dir, Node};
    use crate::test_util::fixture;

    fn split() -> Layout {
        let node = Node::Split {
            dir: Dir::Right,
            ratio: 0.6,
            first: Box::new(Node::Leaf(4)),
            second: Box::new(Node::Leaf(5)),
        };
        Layout::from(&node)
    }

    fn pane(id: u64, active: bool) -> PaneInfo {
        PaneInfo {
            id,
            path: Some(PathBuf::from("/home/me/web-shop")),
            program: Some("claude".into()),
            agent: Some("claude".into()),
            status: Some("working".into()),
            model: Some("Opus 5.5".into()),
            context: Some(23),
            active,
            title: active.then(|| "✳ Claude Code".into()),
            ..PaneInfo::default()
        }
    }

    fn snapshot() -> Snapshot {
        let tab = TabInfo {
            id: 3,
            name: "agents".into(),
            status: Some("working".into()),
            active: true,
            panes: vec![pane(4, true), PaneInfo { parent: Some(4), subagents: 2, ..pane(5, false) }],
            custom_name: Some("agents".into()),
            layout: Some(split()),
            controller: None,
        };
        let workspace = WorkspaceInfo {
            id: 2,
            name: "feat/dark-mode".into(),
            path: PathBuf::from("/home/me/.cornercase/worktrees/web-shop/feat-dark-mode"),
            branch: Some("feat/dark-mode".into()),
            worktree: true,
            active: true,
            tabs: vec![tab],
            collapsed: true,
            ..WorkspaceInfo::default()
        };
        let project = ProjectInfo {
            id: 1,
            name: "shop".into(),
            path: PathBuf::from("/home/me/web-shop"),
            group: Some(10),
            active: true,
            workspaces: vec![workspace],
            custom_name: Some("shop".into()),
            collapsed: false,
        };
        let group = GroupInfo { id: 10, name: "work".into(), collapsed: false, icon: "●".into(), colour: 33 };
        Snapshot {
            revision: 7,
            report: Report {
                version: "0.12.111".into(),
                boot: "4242-1760600000".into(),
                caller: None,
                shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
                groups: vec![group],
                projects: vec![project],
                keep_awake: None,
                view: None,
                theme: None,
                config: Some(ConfigInfo { revision: 2 }),
            },
        }
    }

    #[test]
    fn the_fixture_is_of_the_watch_version_the_contract_names() {
        assert_eq!(contract::WATCH_VERSION, 1);
    }

    #[test]
    fn a_snapshot_is_written_as_its_fixture() {
        let written = serde_json::to_string_pretty(&snapshot()).expect("json");

        assert_eq!(written, fixture!("control/watch/v1/snapshot.json").trim_end());
    }

    #[test]
    fn the_fixture_reads_back_as_its_snapshot() {
        let read: Snapshot = serde_json::from_str(fixture!("control/watch/v1/snapshot.json")).expect("a snapshot");

        assert_eq!(read, snapshot());
    }

    fn viewed() -> Snapshot {
        let mut snapshot = snapshot();
        snapshot.report.projects[0].workspaces[0].tabs[0].controller = Some(41);
        let shown = snapshot.report.shown;
        snapshot.report.view = Some(ViewInfo {
            id: 41,
            projection: 3,
            active: true,
            cols: Some(120),
            rows: Some(40),
            shown,
            controls: true,
            visible: true,
        });
        snapshot
    }

    #[test]
    fn a_view_snapshot_is_written_as_its_fixture_and_reads_back() {
        let written = serde_json::to_string_pretty(&viewed()).expect("json");
        let read: Snapshot = serde_json::from_str(fixture!("control/view/v1/snapshot.json")).expect("a snapshot");

        assert_eq!(written, fixture!("control/view/v1/snapshot.json").trim_end());
        assert_eq!(read, viewed());
    }

    #[test]
    fn a_snapshot_without_a_view_leaves_its_keys_out() {
        let sent = serde_json::to_string(&snapshot()).expect("json");

        assert!(!sent.contains("\"view\"") && !sent.contains("controller"), "{sent}");
    }

    #[test]
    fn a_tab_layout_is_the_split_tree_of_its_pane_ids() {
        let sent: Value = serde_json::from_str(&serde_json::to_string(&snapshot()).expect("json")).expect("json");

        let layout = &sent["projects"][0]["workspaces"][0]["tabs"][0]["layout"];
        assert_eq!(
            *layout,
            json!({"split": {"dir": "right", "ratio": 0.6, "first": {"leaf": 4}, "second": {"leaf": 5}}})
        );
    }

    #[test]
    fn a_streamed_snapshot_is_one_line_holding_it_under_ok() {
        let line = snapshot().streamed().expect("a line");

        assert!(line.contains(r#""ratio":0.6,"#), "{line}");
        let sent: Value = serde_json::from_str(&line).expect("json");
        let streamed: Snapshot = serde_json::from_value(sent["ok"].clone()).expect("a snapshot under ok");
        assert_eq!((streamed, line.contains('\n')), (snapshot(), false));
    }

    #[test]
    fn a_report_from_before_the_snapshot_fields_takes_their_defaults() {
        let old = json!({"version": "0.12.110", "boot": "1-2", "shown": {}, "groups": [{"id": 10, "name": "work",
            "collapsed": true}], "projects": [{"id": 1, "name": "shop", "path": "/p", "workspaces": [{"id": 2,
            "name": "main", "path": "/p", "tabs": [{"id": 3, "name": "zsh", "panes": [{"id": 4}]}]}]}]});

        let report: Report = serde_json::from_value(old).expect("a report");

        let group = &report.groups[0];
        assert_eq!((group.icon.as_str(), group.colour), ("", 0));
        let project = &report.projects[0];
        assert_eq!((project.custom_name.as_deref(), project.collapsed), (None, false));
        let workspace = &project.workspaces[0];
        assert_eq!((workspace.custom_name.as_deref(), workspace.collapsed), (None, false));
        let tab = &workspace.tabs[0];
        assert_eq!((tab.custom_name.as_deref(), tab.layout.as_ref()), (None, None));
        assert_eq!(tab.panes[0].title, None);
    }

    #[test]
    fn a_snapshot_without_a_revision_reads_as_revision_zero() {
        let snapshot: Snapshot = serde_json::from_value(json!({"boot": "1-2"})).expect("a snapshot");

        assert_eq!((snapshot.revision, snapshot.report.boot.as_str()), (0, "1-2"));
    }
}

mod colours {
    use super::*;

    #[test]
    fn a_report_says_where_the_colours_come_from() {
        let theme = crate::test_util::theme(libghostty_vt::style::RgbColor { r: 251, g: 250, b: 246 });
        let report = Report { theme: Some(ThemeInfo::of(theme::ThemeFrom::Window, &theme)), ..Report::default() };

        let sent = serde_json::to_value(&report).expect("json");

        assert_eq!(sent["theme"], json!({"from": "window", "scheme": "light"}));
        assert!(serde_json::to_value(Report::default()).expect("json").get("theme").is_none());
    }
}

#[test]
fn a_close_from_an_older_command_does_not_unlock() {
    let request: Request =
        serde_json::from_str(r#"{"command": "close", "args": {"item": {"workspace": 4}, "remove_worktree": true}}"#)
            .expect("a request");

    assert_eq!(
        request.command,
        Command::Close(Close { item: Item::Workspace(4), remove_worktree: true, force: false, unlock: false })
    );
}

#[rstest::rstest]
#[case::add(r#"{"add": "write"}"#, Todo::Add("write".into()))]
#[case::list(r#""list""#, Todo::List)]
#[case::done(r#"{"done": 3}"#, Todo::Done(3))]
#[case::rm(r#"{"rm": 3}"#, Todo::Rm(3))]
#[case::edit(r#"{"edit": {"id": 3, "text": "again"}}"#, Todo::Edit { id: 3, text: "again".into() })]
#[case::undone(r#"{"undone": 3}"#, Todo::Undone(3))]
#[case::move_before(r#"{"move": {"id": 3, "before": 5}}"#, Todo::Move { id: 3, before: Some(5) })]
#[case::move_last(r#"{"move": {"id": 3}}"#, Todo::Move { id: 3, before: None })]
#[case::clear_done(r#""clear-done""#, Todo::ClearDone)]
#[case::restore(r#"{"restore": 7}"#, Todo::Restore(7))]
fn todo_actions_read_by_name(#[case] text: &str, #[case] expected: Todo) {
    let todo: Todo = serde_json::from_str(text).expect("a todo action");
    assert_eq!(todo, expected);
    assert_eq!(serde_json::from_str::<Todo>(&serde_json::to_string(&todo).expect("json")).expect("back"), expected);
}
