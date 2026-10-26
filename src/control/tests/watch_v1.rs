use super::*;
use crate::project::split::{Dir, Node};
use crate::test_util::fixture;

fn split() -> Layout {
    let node =
        Node::Split { dir: Dir::Right, ratio: 0.6, first: Box::new(Node::Leaf(4)), second: Box::new(Node::Leaf(5)) };
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

#[test]
fn a_pane_leaves_collapsed_and_compacted_out_unless_they_are_true() {
    let plain = serde_json::to_value(pane(4, false)).expect("a pane");
    let on = PaneInfo { collapsed: true, compacted: true, ..pane(4, false) };
    let folded = serde_json::to_value(on).expect("a pane");

    assert!(plain.get("collapsed").is_none() && plain.get("compacted").is_none(), "{plain}");
    assert_eq!((&folded["collapsed"], &folded["compacted"]), (&json!(true), &json!(true)));
    let read = serde_json::from_value::<PaneInfo>(plain).expect("a pane");
    assert!(!read.collapsed && !read.compacted);
}

#[test]
fn a_pane_leaves_its_accounts_out_unless_it_has_some() {
    let accounts: BTreeMap<String, String> = [("claude".to_string(), "work".to_string())].into();
    let plain = serde_json::to_value(pane(4, false)).expect("a pane");
    let given = serde_json::to_value(PaneInfo { accounts: accounts.clone(), ..pane(4, false) }).expect("a pane");

    assert!(plain.get("accounts").is_none(), "{plain}");
    assert_eq!(given["accounts"], serde_json::json!({ "claude": "work" }));
    assert_eq!(serde_json::from_value::<PaneInfo>(given).expect("a pane").accounts, accounts);
}

#[test]
fn a_pane_leaves_its_pid_and_memory_out_unless_it_has_them() {
    let plain = serde_json::to_value(pane(4, false)).expect("a pane");
    let given =
        serde_json::to_value(PaneInfo { pid: Some(321), memory: Some(4096), ..pane(4, false) }).expect("a pane");

    assert!(plain.get("pid").is_none() && plain.get("memory").is_none(), "{plain}");
    assert_eq!((&given["pid"], &given["memory"]), (&json!(321), &json!(4096)));
    let read = serde_json::from_value::<PaneInfo>(given).expect("a pane");
    assert_eq!((read.pid, read.memory), (Some(321), Some(4096)));
}

#[test]
fn a_pane_row_gives_its_pid_and_memory_or_null() {
    let mut project = shop_project();
    project.workspaces[0].tabs[0].panes[0].pid = Some(321);
    project.workspaces[0].tabs[0].panes[0].memory = Some(4096);
    let report = Report { projects: vec![project], ..Report::default() };

    let rows = serde_json::to_value(report.panes()).expect("rows");

    assert_eq!((&rows[0]["pid"], &rows[0]["memory"]), (&json!(321), &json!(4096)));
    assert_eq!((&rows[1]["pid"], &rows[1]["memory"]), (&Value::Null, &Value::Null));
}

fn agents_tab() -> TabInfo {
    TabInfo {
        id: 3,
        name: "agents".into(),
        status: Some("working".into()),
        active: true,
        panes: vec![
            pane(4, true),
            PaneInfo { parent: Some(4), subagents: 2, role: Some("coder".into()), ..pane(5, false) },
        ],
        custom_name: Some("agents".into()),
        layout: Some(split()),
        controller: None,
    }
}

fn shop_project() -> ProjectInfo {
    let workspace = WorkspaceInfo {
        id: 2,
        name: "feat/dark-mode".into(),
        path: PathBuf::from("/home/me/.cornercase/worktrees/web-shop/feat-dark-mode"),
        branch: Some("feat/dark-mode".into()),
        worktree: true,
        active: true,
        tabs: vec![agents_tab()],
        collapsed: true,
        ..WorkspaceInfo::default()
    };
    ProjectInfo {
        id: 1,
        name: "shop".into(),
        path: PathBuf::from("/home/me/web-shop"),
        group: Some(10),
        active: true,
        workspaces: vec![workspace],
        custom_name: Some("shop".into()),
        collapsed: false,
    }
}

fn snapshot() -> Snapshot {
    let group = GroupInfo { id: 10, name: "work".into(), collapsed: false, icon: "●".into(), colour: 33 };
    Snapshot {
        revision: 7,
        report: Report {
            version: "0.12.111".into(),
            boot: "4242-1760600000".into(),
            caller: None,
            shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
            groups: vec![group],
            projects: vec![shop_project()],
            keep_awake: None,
            view: None,
            theme: None,
            config: Some(ConfigInfo { revision: 2 }),
            home: None,
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
    assert_eq!(*layout, json!({"split": {"dir": "right", "ratio": 0.6, "first": {"leaf": 4}, "second": {"leaf": 5}}}));
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
