use super::*;
use crate::control::AgainstBase;

fn pane(id: u64, program: &str, active: bool) -> PaneInfo {
    PaneInfo {
        id,
        path: Some(PathBuf::from("/home/ana/shop")),
        program: Some(program.into()),
        active,
        ..PaneInfo::default()
    }
}

fn tab() -> TabInfo {
    let claude = PaneInfo {
        agent: Some("claude".into()),
        status: Some("working".into()),
        model: Some("Opus 5.5".into()),
        context: Some(23),
        caller: true,
        ..pane(5, "claude", false)
    };
    TabInfo {
        id: 3,
        name: "claude".into(),
        status: Some("working".into()),
        active: true,
        panes: vec![pane(4, "zsh", true), claude],
        custom_name: None,
        layout: None,
        controller: None,
    }
}

fn workspace() -> WorkspaceInfo {
    WorkspaceInfo {
        id: 2,
        name: "fix/login".into(),
        path: PathBuf::from("/home/ana/.cornercase/worktrees/shop/fix-login"),
        branch: Some("fix/login".into()),
        worktree: true,
        behind: 2,
        pull_request: None,
        setup: None,
        run_dir: None,
        run: None,
        active: true,
        custom_name: None,
        collapsed: false,
        changes: None,
        against_base: None,
        closed_agents: None,
        post: None,
        shared: Vec::new(),
        tabs: vec![tab()],
        machine: None,
    }
}

fn project(id: u64, name: &str, group: Option<u64>) -> ProjectInfo {
    ProjectInfo { id, name: name.into(), path: format!("/home/ana/{name}").into(), group, ..ProjectInfo::default() }
}

fn report() -> Report {
    Report {
        version: "0.9.0".into(),
        boot: String::new(),
        caller: Some(5),
        shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
        groups: vec![GroupInfo { id: 8, name: "work".into(), collapsed: true, icon: String::new(), colour: 0 }],
        projects: vec![
            ProjectInfo { workspaces: vec![workspace()], ..project(1, "shop", None) },
            project(9, "api", Some(8)),
        ],
        keep_awake: None,
        view: None,
        theme: None,
        config: None,
        home: None,
        machines: Vec::new(),
        shown_remote: None,
    }
}

#[test]
fn status_is_a_tree_of_ids_with_what_matters() {
    let text = render(&report(), Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert_eq!(
        text,
        "project 1  shop  ~/shop  (shown)\n\
             \x20 workspace 2  fix/login  ~/.cornercase/worktrees/shop/fix-login  branch fix/login  worktree  2 behind  (shown)\n\
             \x20   tab 3  claude  working  (shown)\n\
             \x20     pane 4  zsh  ~/shop  (shown)\n\
             \x20     pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (you)\n\
             group 8  work  collapsed\n\
             \x20 project 9  api  ~/api\n"
    );
}

#[test]
fn a_workspace_with_a_pull_request_names_it_and_its_checks() {
    let mut report = report();
    let pull = |checks: Option<&str>| PullRequestInfo {
        number: 482,
        state: "open".into(),
        checks: checks.map(String::from),
        ..PullRequestInfo::default()
    };
    report.projects[0].workspaces[0].pull_request = Some(pull(Some("failing")));
    let failing = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);
    report.projects[0].workspaces[0].pull_request = Some(pull(None));
    let unchecked = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(failing.contains("worktree  2 behind  pull request #482 open, checks failing  (shown)\n"), "{failing}");
    assert!(unchecked.contains("worktree  2 behind  pull request #482 open  (shown)\n"), "{unchecked}");
}

#[test]
fn a_pull_request_with_conflicts_says_so_before_its_checks() {
    let mut report = report();
    let mut pull = PullRequestInfo {
        number: 482,
        state: "open".into(),
        checks: Some("passing".into()),
        conflicts: true,
        base: Some("main".into()),
        ..PullRequestInfo::default()
    };
    report.projects[0].workspaces[0].pull_request = Some(pull.clone());
    let named = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);
    pull.base = None;
    report.projects[0].workspaces[0].pull_request = Some(pull);
    let bare = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(named.contains("pull request #482 open, conflicts with main, checks passing  (shown)\n"), "{named}");
    assert!(bare.contains("pull request #482 open, conflicts, checks passing  (shown)\n"), "{bare}");
}

#[test]
fn a_pull_request_in_a_stack_names_its_layer() {
    let mut report = report();
    report.projects[0].workspaces[0].pull_request = Some(PullRequestInfo {
        number: 483,
        state: "open".into(),
        checks: Some("passing".into()),
        stack: Some(StackInfo { number: 484, base: "main".into(), position: 2, size: 3, layers: Vec::new() }),
        ..PullRequestInfo::default()
    });

    let said = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(
        said.contains("pull request #483 open, checks passing, layer 2 of 3 of stack #484 on main  (shown)\n"),
        "{said}"
    );
}

#[test]
fn a_workspace_with_a_merge_request_names_it_and_its_pipeline() {
    let mut report = report();
    report.projects[0].workspaces[0].pull_request = Some(PullRequestInfo {
        number: 486,
        state: "open".into(),
        checks: Some("passing".into()),
        conflicts: true,
        base: Some("main".into()),
        provider: "gitlab".into(),
        ..PullRequestInfo::default()
    });

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains("merge request !486 open, conflicts with main, pipeline passing  (shown)\n"), "{text}");
}

#[test]
fn an_agent_left_with_a_background_shell_still_reads_working() {
    let mut report = report();
    report.projects[0].workspaces[0].tabs[0].panes[1].background_shell = true;

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains("pane 5  claude  working (background shell)  Opus 5.5 · 23%  ~/shop  (you)\n"), "{text}");
}

#[test]
fn an_agent_whose_context_was_compacted_says_so_where_the_share_goes() {
    let mut report = report();
    let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[1];
    pane.context = None;
    pane.compacted = true;

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);
    assert!(text.contains("pane 5  claude  working  Opus 5.5 · compacted  ~/shop  (you)\n"), "{text}");

    report.projects[0].workspaces[0].tabs[0].panes[1].model = None;
    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);
    assert!(text.contains("pane 5  claude  working  compacted  ~/shop  (you)\n"), "{text}");
}

#[test]
fn an_agent_with_subagents_says_how_many() {
    let mut report = report();
    report.projects[0].workspaces[0].tabs[0].panes[1].subagents = 2;

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains("pane 5  claude  working (2 subagents)  Opus 5.5 · 23%  ~/shop  (you)\n"), "{text}");
}

#[test]
fn a_pane_started_from_another_names_its_parent() {
    let mut report = report();
    report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains("pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (parent 4, you)\n"), "{text}");
}

#[test]
fn a_collapsed_agent_says_so() {
    let mut report = report();
    report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);
    report.projects[0].workspaces[0].tabs[0].panes[1].collapsed = true;

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains("~/shop  (parent 4, collapsed, you)\n"), "{text}");
}

#[test]
fn a_flow_member_names_its_role() {
    let mut report = report();
    report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);
    report.projects[0].workspaces[0].tabs[0].panes[1].role = Some("coder".into());

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains("pane 5  claude  working  Opus 5.5 · 23%  ~/shop  (parent 4, role coder, you)\n"), "{text}");
}

#[test]
fn panes_are_one_row_each_under_a_header_with_a_dash_for_what_is_missing() {
    let mut report = report();
    report.projects[0].workspaces[0].tabs[0].panes[0].at_prompt = Some(true);
    report.projects[0].workspaces[0].tabs[0].panes[1].survey = true;
    report.projects[0].workspaces[0].tabs[0].panes[1].resuming = true;
    report.projects[0].workspaces[0].tabs[0].panes[1].parent = Some(4);
    report.projects[0].workspaces[0].tabs[0].panes[1].subagents = 2;
    report.projects[0].workspaces[0].tabs[0].panes[1].role = Some("coder".into());
    report.projects[0].workspaces[0].tabs[0].panes[1].held_sends = 1;
    report.projects[0].workspaces[0].tabs[0].panes[1].draft = true;
    report.projects[0].workspaces[0].tabs[0].panes[1].account_restart = Some("waiting".into());
    report.projects[0].workspaces[0].tabs[0].panes[1].pid = Some(321);
    report.projects[0].workspaces[0].tabs[0].panes[1].memory = Some(4096);
    report.projects[0].workspaces[0].tabs[0].panes[1].compacted = true;

    let text = pane_table(&report.panes()).expect("a table");

    assert_eq!(
        text,
        "pane\ttab\tworkspace\tproject\tprogram\tagent\tstatus\tbackground_shell\tdialog\tat_prompt\tmodel\t\
             context\tcaller\tshown\tactive\ttab_name\tworkspace_name\tbranch\tworktree\tproject_name\tpath\t\
             survey\tresuming\tparent\tsubagents\trole\theld_sends\tdraft\taccount_restart\tpid\tmemory\tcompacted\n\
             4\t3\t2\t1\tzsh\t-\t-\tfalse\tfalse\ttrue\t-\t-\tfalse\ttrue\ttrue\t\
             claude\tfix/login\tfix/login\ttrue\tshop\t/home/ana/shop\tfalse\tfalse\t-\t0\t-\t0\tfalse\t-\t-\t-\tfalse\n\
             5\t3\t2\t1\tclaude\tclaude\tworking\tfalse\tfalse\t-\tOpus 5.5\t23\ttrue\tfalse\tfalse\t\
             claude\tfix/login\tfix/login\ttrue\tshop\t/home/ana/shop\ttrue\ttrue\t4\t2\tcoder\t1\ttrue\twaiting\t321\t4096\ttrue\n"
    );
}

#[test]
fn panes_follow_the_sidebar_with_grouped_projects_after_the_loose_ones() {
    let one_pane = |project: u64, group| {
        let pane = PaneInfo { id: project * 10, ..PaneInfo::default() };
        let tab = TabInfo { id: project * 10 - 1, panes: vec![pane], ..TabInfo::default() };
        let workspace = WorkspaceInfo { id: project * 10 - 2, tabs: vec![tab], ..WorkspaceInfo::default() };
        ProjectInfo { id: project, group, workspaces: vec![workspace], ..ProjectInfo::default() }
    };
    let report = Report {
        groups: vec![GroupInfo { id: 8, ..GroupInfo::default() }, GroupInfo { id: 7, ..GroupInfo::default() }],
        projects: vec![one_pane(1, Some(7)), one_pane(2, Some(8)), one_pane(3, None), one_pane(4, Some(6))],
        ..Report::default()
    };

    let order: Vec<(u64, u64)> = report.panes().iter().map(|row| (row.project, row.pane)).collect();

    assert_eq!(order, [(3, 30), (4, 40), (2, 20), (1, 10)]);
}

#[rstest]
#[case::nothing(Value::Null, "-")]
#[case::an_empty_name(serde_json::json!(""), "-")]
#[case::a_name_with_a_tab_and_a_line_break(serde_json::json!("a\tb\nc"), "a b c")]
#[case::a_name_with_spaces(serde_json::json!("review #42"), "review #42")]
#[case::a_flag(serde_json::json!(false), "false")]
#[case::a_number(serde_json::json!(23), "23")]
fn a_cell_never_breaks_its_row(#[case] value: Value, #[case] expected: &str) {
    assert_eq!(cell(&value), expected);
}

#[test]
fn panes_as_json_are_an_object_holding_the_rows() {
    let rows = PaneRows { panes: report().panes() };

    let value = serde_json::to_value(&rows).expect("json");

    let panes = value["panes"].as_array().expect("a list");
    assert_eq!(panes.len(), 2);
    assert_eq!(
        (&panes[1]["pane"], &panes[1]["status"], &panes[1]["caller"], &panes[0]["agent"]),
        (&serde_json::json!(5), &serde_json::json!("working"), &serde_json::json!(true), &Value::Null)
    );
}

fn event(what: What, ids: Ids) -> Value {
    let event = Event { time: "2026-10-08T12:00:00.000Z".into(), what, ids };
    serde_json::to_value(event).expect("json")
}

const PANE: Ids = Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) };
const TAB: Ids = Ids { pane: None, ..PANE };

#[rstest]
#[case::a_pane_opened(What::Opened { kind: control::Kind::Pane, parent: None, accounts: std::collections::BTreeMap::new() }, PANE, "pane 4  opened in tab 3")]
#[case::a_pane_opened_from_another(
    What::Opened { kind: control::Kind::Pane, parent: Some(2), accounts: std::collections::BTreeMap::new() },
    PANE,
    "pane 4  opened in tab 3 from pane 2"
)]
#[case::a_project_opened(
    What::Opened { kind: control::Kind::Project, parent: None, accounts: std::collections::BTreeMap::new() },
    Ids { project: Some(1), ..Ids::default() },
    "project 1  opened"
)]
#[case::a_tab_closed(What::Closed { kind: control::Kind::Tab }, TAB, "tab 3  closed")]
#[case::an_agent_at_work(
    What::Status { agent: "claude".into(), from: Some("working".into()), to: Some("waiting".into()) },
    PANE,
    "pane 4  claude  working → waiting"
)]
#[case::an_agent_arrived(
    What::Status { agent: "codex".into(), from: None, to: Some("idle".into()) },
    PANE,
    "pane 4  codex  none → idle"
)]
#[case::a_program_ended(What::Exited { program: "cargo".into() }, PANE, "pane 4  cargo  exited")]
#[case::a_clipboard_write(What::Clipboard { text: "hello".into() }, PANE, "pane 4  clipboard  5 bytes")]
#[case::one_byte(What::Clipboard { text: "a".into() }, PANE, "pane 4  clipboard  1 byte")]
#[case::a_bell(What::Bell { count: 1 }, PANE, "pane 4  bell")]
#[case::bells(What::Bell { count: 3 }, PANE, "pane 4  bell  3 times")]
#[case::one_dropped(
    What::Dropped { count: 1 },
    Ids::default(),
    "1 event dropped: this command fell behind; `cornercase status` shows where things are now"
)]
#[case::a_notice_from_a_machine(
    What::Notice {
        token: "cc-1-2".into(),
        agent: "claude".into(),
        status: "waiting".into(),
        place: "shop › main".into(),
        message: "claude on vps needs you in shop › main".into(),
        panes: vec![4],
        sound: None,
        machine: Some("vps".into()),
    },
    Ids::default(),
    "machine vps  notice  claude on vps needs you in shop › main  cc-1-2"
)]
fn an_event_reads_as_one_line(#[case] what: What, #[case] ids: Ids, #[case] expected: &str) {
    assert_eq!(event_line(&event(what, ids)), expected);
}

#[test]
fn an_event_from_a_newer_server_is_printed_as_it_came() {
    let value = serde_json::json!({"time": "2026-10-08T12:00:00.000Z", "event": "renamed", "tab": 3});

    assert_eq!(event_line(&value), value.to_string());
}

#[rstest]
#[case::a_dialog(false, true, false, "idle (dialog open)")]
#[case::both(true, true, false, "working (background shell, dialog open)")]
#[case::the_survey(true, false, true, "working (background shell, survey open)")]
fn what_covers_an_agent_is_said_after_its_status(
    #[case] background_shell: bool,
    #[case] dialog: bool,
    #[case] survey: bool,
    #[case] shown: &str,
) {
    let mut report = report();
    let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[1];
    pane.status = Some(if background_shell { "working" } else { "idle" }.into());
    (pane.background_shell, pane.dialog, pane.survey) = (background_shell, dialog, survey);

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains(&format!("pane 5  claude  {shown}  Opus 5.5 · 23%")), "{text}");
}

#[rstest]
#[case::not_waiting(None, "working")]
#[case::waiting_to_restart(Some("waiting"), "working (restarts after this turn)")]
fn an_agent_waiting_to_restart_says_so_after_its_status(#[case] restart: Option<&str>, #[case] shown: &str) {
    let mut report = report();
    let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[1];
    pane.account_restart = restart.map(str::to_string);

    let text = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(text.contains(&format!("pane 5  claude  {shown}  Opus 5.5 · 23%")), "{text}");
}

#[rstest]
#[case::a_state(Some("idle"), None, &["idle"])]
#[case::a_matching_line(Some("text"), Some("test result: ok"), &["test result: ok"])]
#[case::no_wait(None, None, &[])]
fn the_ending_is_the_state_or_the_line(
    #[case] ended: Option<&str>,
    #[case] line: Option<&str>,
    #[case] expected: &[&str],
) {
    let done = Done { ended: ended.map(str::to_string), line: line.map(str::to_string), ..Done::default() };

    assert_eq!(ending(&done), expected);
}

#[test]
fn several_panes_end_one_line_each_with_the_id_they_were_given() {
    let pane = |tab, pane, ended: &str, line: Option<&str>| Done {
        ids: Ids { tab, pane: Some(pane), ..Ids::default() },
        ended: Some(ended.into()),
        line: line.map(str::to_string),
        ..Done::default()
    };
    let done = Done {
        panes: vec![pane(None, 4, "idle", None), pane(Some(9), 12, "text", Some("ok")), pane(None, 7, "closed", None)],
        ..Done::default()
    };

    assert_eq!(endings(&done), ["4 idle", "9 ok", "7 closed"]);
}

#[test]
fn a_workspace_says_what_it_changed_against_its_base() {
    let mut report = report();
    let against = |files| AgainstBase { base: "origin/main".into(), files, added: 640, removed: 9 };
    report.projects[0].workspaces[0].against_base = Some(against(16));
    let many = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);
    report.projects[0].workspaces[0].against_base = Some(against(1));
    let one = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);
    report.projects[0].workspaces[0].against_base = Some(against(0));
    let clean = render(&report, Some(Path::new("/home/ana")), UNIX_EPOCH);

    assert!(many.contains("2 behind  16 files +640 \u{2212}9 vs origin/main  (shown)\n"), "{many}");
    assert!(one.contains("1 file +640 \u{2212}9 vs origin/main"), "{one}");
    assert!(!clean.contains(" vs "), "{clean}");
}
