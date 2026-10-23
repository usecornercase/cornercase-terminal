use std::time::Duration;

use serde_json::json;

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};
use crate::plugins::manifest::{Manifest, Process};
use crate::plugins::registry::{Entry, Source};
use crate::test_util::fixture;

fn process(id: &str) -> Process {
    Process { id: id.into(), command: vec!["./serve".into()] }
}

fn services() -> Vec<ServiceInfo> {
    let now = Instant::now();
    let seen = [
        ("sync".to_string(), Seen::Running { pid: 4242 }),
        ("watch".to_string(), Seen::Stopped { until: now + Duration::from_millis(2500) }),
    ];
    ServiceInfo::of(&[process("sync"), process("watch"), process("late")], &seen, now)
}

fn listed() -> PluginsInfo {
    let row = |id: &str, state: &str, error: Option<&str>, services| PluginInfo {
        id: id.into(),
        enabled: state == "on",
        state: state.into(),
        error: error.map(str::to_string),
        label: format!("{id} 1.0"),
        note: "service sync runs".into(),
        source: json!({"kind": "link", "path": "/code/notes"}),
        log: format!("/state/plugins/state/{id}/plugin.log").into(),
        manifest: error.is_none().then(|| json!({"id": id, "name": "Notes", "version": "1.0"})),
        services,
        button: None,
    };
    let button = ButtonInfo { title: "Deploy".into(), icon: "D".into(), actions: vec!["status".into()] };
    PluginsInfo {
        revision: Some(3),
        problem: None,
        plugins: vec![
            PluginInfo { button: Some(button), ..row("acme.notes", "on", None, services()) },
            row("acme.broken", "cannot-run", Some("cannot read the manifest"), Vec::new()),
        ],
    }
}

fn actions() -> ActionsInfo {
    ActionsInfo {
        revision: 3,
        actions: vec![
            ActionInfo {
                plugin: "acme.notes".into(),
                name: "Notes".into(),
                action: "note".into(),
                title: "take a note".into(),
                contexts: vec!["pane".into(), "tab".into()],
                placement: Some("job".into()),
                width: None,
                height: None,
            },
            ActionInfo {
                plugin: "acme.notes".into(),
                name: "Notes".into(),
                action: "status".into(),
                title: "Deploy status".into(),
                contexts: vec!["button".into()],
                placement: Some("popup".into()),
                width: Some(100),
                height: Some(30),
            },
        ],
    }
}

fn ran() -> ActionRan {
    ActionRan {
        plugin: "acme.notes".into(),
        action: "fail".into(),
        context: "pane".into(),
        ids: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(5) },
        ended: "exited".into(),
        code: Some(3),
        signal: None,
        error: Some("exit status: 3".into()),
        ms: 41,
        log: "/state/plugins/state/acme.notes/plugin.log".into(),
    }
}

fn written(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).expect("json")
}

#[test]
fn the_answers_are_written_as_their_fixtures() {
    assert_eq!(written(&listed()), fixture!("control/plugins/list.json").trim_end());
    assert_eq!(written(&actions()), fixture!("control/plugins/actions.json").trim_end());
    assert_eq!(written(&ran()), fixture!("control/plugins/ran.json").trim_end());
}

#[test]
fn the_fixtures_read_back() {
    let list: PluginsInfo = serde_json::from_str(fixture!("control/plugins/list.json")).expect("list");
    assert_eq!(list, listed());
    let actions_back: ActionsInfo = serde_json::from_str(fixture!("control/plugins/actions.json")).expect("actions");
    assert_eq!(actions_back, actions());
    let ran_back: ActionRan = serde_json::from_str(fixture!("control/plugins/ran.json")).expect("ran");
    assert_eq!(ran_back, ran());
}

#[test]
fn a_newer_server_may_add_keys() {
    let list: PluginsInfo = serde_json::from_value(json!({
        "plugins": [{"id": "x", "state": "dormant", "source": {"kind": "teleport"}, "new": 1}],
        "extra": true
    }))
    .expect("list");
    assert_eq!((list.plugins[0].state.as_str(), list.revision), ("dormant", None));
    let ran: ActionRan = serde_json::from_value(json!({"ended": "vanished", "newer": 1})).expect("ran");
    assert_eq!(ran.ended, "vanished");
}

#[test]
fn services_are_running_stopped_or_starting() {
    let found = services();
    let states: Vec<_> = found.iter().map(|s| (s.id.as_str(), s.state.as_str(), s.pid, s.restart_in)).collect();
    assert_eq!(
        states,
        [("sync", "running", Some(4242), None), ("watch", "stopped", None, Some(3)), ("late", "starting", None, None)]
    );
}

#[test]
fn the_requests_parse_and_have_no_stray_field() {
    let cases = [
        json!({"command": "plugin-list", "args": {}}),
        json!({"command": "plugin-actions", "args": {"context": "pane"}}),
        json!({"command": "plugin-run", "args": {"plugin": "a", "action": "b", "revision": 3, "item": {"pane": 5}}}),
        json!({"command": "plugin-actions", "args": {"context": "button"}}),
        json!({"command": "plugin-run", "args": {"plugin": "a", "action": "b", "revision": 3}}),
        json!({"command": "plugin-popup-close", "args": {}}),
    ];
    for raw in cases {
        let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
        assert_eq!(unknown_fields(&raw, &parsed), None, "{raw}");
    }
    let raw = json!({"command": "plugin-actions", "args": {"bogus": 1}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("args.bogus"));
}

#[test]
fn a_group_is_not_a_context_and_a_run_needs_every_field() {
    let group = json!({"command": "plugin-actions", "args": {"context": "group"}});
    assert!(serde_json::from_value::<Request>(group).is_err());
    let no_revision = json!({"command": "plugin-run", "args": {"plugin": "a", "action": "b", "item": {"pane": 5}}});
    assert!(serde_json::from_value::<Request>(no_revision).is_err());
}

#[test]
fn the_logged_fields_name_the_plugin_and_the_item() {
    let command = Command::PluginRun(PluginRun {
        plugin: "a".into(),
        action: "b".into(),
        revision: 3,
        item: Some(Item::Pane(5)),
    });
    let fields: Vec<_> = command.fields();
    assert_eq!(
        fields,
        [
            ("plugin", "a".to_string()),
            ("action", "b".to_string()),
            ("revision", "3".to_string()),
            ("item", "pane-5".to_string())
        ]
    );
    let all = Command::PluginActions(PluginActions::default()).fields();
    assert_eq!(all, [("context", "all".to_string())]);
    let shown = Command::PluginRun(PluginRun { plugin: "a".into(), action: "b".into(), revision: 3, item: None });
    assert_eq!(shown.fields().len(), 3);
}

fn loaded(manifest: &str) -> Plugin {
    let entry = Entry { id: "acme.notes".into(), source: Source::Link { path: "/code/notes".into() }, enabled: true };
    Plugin { entry, manifest: Manifest::parse(manifest) }
}

const MANIFEST: &str = "id = \"acme.notes\"\nname = \"Notes\"\nversion = \"1\"\n\
    [[actions]]\nid = \"note\"\ntitle = \"take a note\"\ncommand = [\"x\"]\ncontexts = [\"pane\", \"button\"]\n\
    [[actions]]\nid = \"status\"\ntitle = \"Deploy status\"\ncommand = [\"x\"]\ncontexts = [\"button\"]\n\
    placement = \"popup\"\nwidth = 100\nheight = 30\n";

#[test]
fn a_plugin_with_button_actions_lists_its_button() {
    let places = Places::new(std::path::Path::new("/config"), std::path::Path::new("/state"));
    let info = PluginInfo::of(&loaded(MANIFEST), &places, None, Vec::new());
    let button = ButtonInfo { title: "Notes".into(), icon: "N".into(), actions: vec!["note".into(), "status".into()] };
    assert_eq!(info.button, Some(button));
    let plain = MANIFEST.replace(", \"button\"", "").replace("[\"button\"]", "[\"tab\"]");
    let info = PluginInfo::of(&loaded(&plain), &places, None, Vec::new());
    assert_eq!((written(&info).contains("\"button\""), info.button), (false, None));
}

#[test]
fn each_action_says_where_it_shows_and_its_size() {
    let plugin = loaded(MANIFEST);
    let manifest = plugin.manifest.as_ref().expect("manifest");
    let infos: Vec<_> = manifest.actions.iter().map(|action| ActionInfo::of(&plugin, action)).collect();
    let shown: Vec<_> = infos.iter().map(|a| (a.placement.as_deref(), a.width, a.height)).collect();
    assert_eq!(shown, [(Some("job"), None, None), (Some("popup"), Some(100), Some(30))]);
    let job = written(&infos[0]);
    assert!(!job.contains("width") && !job.contains("height"), "{job}");
}

#[test]
fn an_older_answer_reads_without_the_new_keys() {
    let action: ActionInfo = serde_json::from_value(json!({"plugin": "p", "action": "a"})).expect("action");
    assert_eq!((action.placement, action.width, action.height), (None, None, None));
    let plugin: PluginInfo = serde_json::from_value(json!({"id": "p"})).expect("plugin");
    assert_eq!(plugin.button, None);
}

#[test]
fn a_run_without_an_item_leaves_the_key_out() {
    let run = PluginRun { plugin: "a".into(), action: "b".into(), revision: 3, item: None };
    assert_eq!(serde_json::to_value(&run).expect("json"), json!({"plugin": "a", "action": "b", "revision": 3}));
}

#[test]
fn the_outcome_of_each_ending() {
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::ExitStatus;
    let at = |ended| {
        let invoked = Invoked { plugin: "p", action: "a", context: Context::Tab, ids: Ids::default() };
        ActionRan::of(invoked, &Ran { ended, took: Duration::from_millis(7) }, "/log".into())
    };
    let signal = at(Ended::Status(ExitStatus::from_raw(9)));
    assert_eq!((signal.ended.as_str(), signal.code, signal.signal), ("signal", None, Some(9)));
    assert_eq!(signal.error.as_deref(), Some("signal: 9 (SIGKILL)"));
    let timeout = at(Ended::Timeout(Duration::from_secs(300)));
    assert_eq!((timeout.ended.as_str(), timeout.error.as_deref()), ("timeout", Some("stopped after 300 s")));
    let failed = at(Ended::Failed("cannot start ./x: no".into()));
    assert_eq!((failed.ended.as_str(), failed.error.as_deref(), failed.ms), ("error", Some("cannot start ./x: no"), 7));
    let fine = at(Ended::Status(ExitStatus::from_raw(0)));
    assert_eq!((fine.ended.as_str(), fine.code, fine.error), ("exited", Some(0), None));
}
