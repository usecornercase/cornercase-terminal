use serde_json::json;

use super::super::plugins::{MANIFEST, Plugged, plugged};
use super::*;
use crate::control::plugins::{ActionRan, ActionsInfo, PluginsInfo};
use crate::plugins::Registry;
use crate::plugins::manifest::FILE;
use crate::test_util::write_executable;

fn revision(app: &mut App) -> u32 {
    actions(app, &json!({})).revision
}

fn actions(app: &mut App, args: &Value) -> ActionsInfo {
    serde_json::from_value(ok(app, "plugin-actions", args)).expect("actions")
}

fn list(app: &mut App) -> PluginsInfo {
    serde_json::from_value(ok(app, "plugin-list", &json!({}))).expect("plugins")
}

fn run(p: &mut Plugged, action: &str, item: &Value) -> std::result::Result<ActionRan, Refused> {
    let revision = revision(&mut p.app);
    run_at(p, action, item, revision)
}

fn run_at(p: &mut Plugged, action: &str, item: &Value, revision: u32) -> std::result::Result<ActionRan, Refused> {
    let args = json!({"plugin": "acme.notes", "action": action, "revision": revision, "item": item});
    asked(&mut p.app, &p.rx, "plugin-run", &args).map(|v| serde_json::from_value(v).expect("an outcome"))
}

fn ids(p: &Plugged) -> (u64, u64, u64, u64) {
    let project = &p.app.projects[0];
    let workspace = &project.workspaces[0];
    (project.id, workspace.id, workspace.tabs[0].id, workspace.tabs[0].panes[0].id)
}

fn names(info: &ActionsInfo) -> Vec<&str> {
    info.actions.iter().map(|a| a.action.as_str()).collect()
}

mod listing {
    use super::*;

    #[test]
    fn actions_come_in_the_menus_order_filtered_by_context() {
        let mut p = plugged(true);
        assert_eq!(names(&actions(&mut p.app, &json!({}))), ["note", "fail"]);
        assert_eq!(names(&actions(&mut p.app, &json!({"context": "pane"}))), ["note", "fail"]);
        assert_eq!(names(&actions(&mut p.app, &json!({"context": "tab"}))), ["note"]);
        assert_eq!(actions(&mut p.app, &json!({"context": "project"})).actions.len(), 0);
        let first = &actions(&mut p.app, &json!({})).actions[0];
        assert_eq!(
            (first.plugin.as_str(), first.name.as_str(), first.title.as_str()),
            ("acme.notes", "Notes", "take a note")
        );
        assert_eq!(first.contexts, ["pane", "tab"]);
    }

    #[test]
    fn a_plugin_that_is_off_lists_no_actions() {
        let mut p = plugged(false);
        assert_eq!(actions(&mut p.app, &json!({})).actions.len(), 0);
    }

    #[test]
    fn a_group_is_not_a_context() {
        let mut p = plugged(true);
        assert_eq!(refused(&mut p.app, "plugin-actions", &json!({"context": "group"})).code, "malformed");
    }

    #[test]
    fn the_list_says_what_settings_shows() {
        let mut p = plugged(false);
        let info = list(&mut p.app);
        assert_eq!(info.problem, None);
        let row = &info.plugins[0];
        assert_eq!((row.state.as_str(), row.enabled, row.label.as_str()), ("off", false, "Notes 1.0"));
        assert_eq!(row.note, "asks for read · runs as you");
        assert_eq!(row.source["kind"], "link");
        assert!(row.log.ends_with("plugin.log"));
        assert_eq!(row.manifest.as_ref().expect("manifest")["id"], "acme.notes");
        assert_eq!(row.services.len(), 0);
    }

    #[test]
    fn a_manifest_that_breaks_cannot_run() {
        let mut p = plugged(true);
        std::fs::write(p.root.path().join(FILE), "not toml").expect("manifest");
        let info = list(&mut p.app);
        let row = &info.plugins[0];
        assert_eq!(row.state, "cannot-run");
        assert!(row.error.is_some() && row.manifest.is_none());
        assert_eq!(row.note, row.error.clone().expect("error"));
    }

    #[test]
    fn a_registry_that_cannot_be_read_is_a_problem() {
        let mut p = plugged(true);
        std::fs::write(paths::plugins(p.app.config_file.path()), "garbage").expect("registry");
        let info = list(&mut p.app);
        assert!(info.problem.is_some_and(|text| text.contains("cannot read")));
    }

    #[test]
    fn services_are_starting_then_running_then_stopped() {
        let mut p = plugged(true);
        let manifest = format!("{MANIFEST}\n[[service]]\nid = \"sync\"\ncommand = [\"./serve\"]\n");
        std::fs::write(p.root.path().join(FILE), manifest).expect("manifest");
        let pids = p.root.path().join("pids");
        write_executable(
            &p.root.path().join("serve"),
            &format!("#!/bin/sh\necho $$ >> '{}'\nexec sleep 30\n", pids.display()),
        );
        let starting = list(&mut p.app).plugins[0].services.clone();
        assert_eq!((starting[0].id.as_str(), starting[0].state.as_str()), ("sync", "starting"));
        p.app.start_plugins();
        wait_until("the service runs", || list(&mut p.app).plugins[0].services[0].state == "running");
        let row = list(&mut p.app).plugins[0].clone();
        let pid = row.services[0].pid.expect("a pid");
        assert_eq!(row.note, "service sync runs");
        let pid_i32 = i32::try_from(pid).expect("pid");
        rustix::process::kill_process_group(
            rustix::process::Pid::from_raw(pid_i32).expect("pid"),
            rustix::process::Signal::KILL,
        )
        .expect("kill");
        wait_until("it is seen stopped", || list(&mut p.app).plugins[0].services[0].state == "stopped");
        let stopped = list(&mut p.app).plugins[0].services[0].clone();
        assert!(stopped.restart_in.is_some() && stopped.pid.is_none());
        p.app.stop_plugins();
    }
}

mod running {
    use super::*;

    #[test]
    fn an_action_on_a_pane_answers_how_it_ended_with_the_menus_environment() {
        let mut p = plugged(true);
        let (project, workspace, tab, pane) = ids(&p);
        let ran = run(&mut p, "note", &json!({"pane": pane})).expect("ran");
        assert_eq!((ran.ended.as_str(), ran.code, ran.signal, ran.error.clone()), ("exited", Some(0), None, None));
        assert_eq!((ran.plugin.as_str(), ran.action.as_str(), ran.context.as_str()), ("acme.notes", "note", "pane"));
        assert_eq!(
            (ran.ids.project, ran.ids.workspace, ran.ids.tab, ran.ids.pane),
            (Some(project), Some(workspace), Some(tab), Some(pane))
        );
        assert!(ran.log.ends_with("plugin.log"));
        assert_eq!(
            p.read("out"),
            format!("ctx=pane action=note pane={pane} tab={tab} workspace={workspace} project={project} nested=1\n")
        );
    }

    #[test]
    fn an_action_on_a_tab_has_no_pane() {
        let mut p = plugged(true);
        let (_, _, tab, _) = ids(&p);
        let ran = run(&mut p, "note", &json!({"tab": tab})).expect("ran");
        assert_eq!((ran.context.as_str(), ran.ids.pane, ran.ids.tab), ("tab", None, Some(tab)));
        assert!(p.read("out").contains("ctx=tab action=note pane= tab="));
    }

    #[test]
    fn a_failing_action_is_an_answer_and_never_a_toast() {
        let mut p = plugged(true);
        let (_, _, _, pane) = ids(&p);
        let ran = run(&mut p, "fail", &json!({"pane": pane})).expect("ran");
        assert_eq!((ran.ended.as_str(), ran.code), ("exited", Some(3)));
        assert_eq!(ran.error.as_deref(), Some("exit status: 3"));
        assert!(toast(&p.app).is_none());
        let log = std::fs::read_to_string(&ran.log).expect("log");
        assert!(log.contains("action failed"), "{log}");
    }

    #[test]
    fn a_client_that_leaves_does_not_stop_the_action() {
        let mut p = plugged(true);
        let (_, _, _, pane) = ids(&p);
        let revision = revision(&mut p.app);
        let args = json!({"plugin": "acme.notes", "action": "note", "revision": revision, "item": {"pane": pane}});
        let text = json!({"contract": 1, "command": "plugin-run", "args": args}).to_string();
        p.app.request(CLIENT, &text, Some(AREA), Instant::now());
        p.app.forget(CLIENT);
        wait_until("the action ran", || !p.read("out").is_empty());
        let (app, rx) = (&mut p.app, &p.rx);
        drain(app, rx);
        assert_eq!(app.take_answers().len(), 0);
    }
}

mod refusing {
    use super::*;

    #[test]
    fn a_stale_revision_is_a_conflict_and_nothing_runs() {
        let mut p = plugged(true);
        let (_, _, _, pane) = ids(&p);
        let old = revision(&mut p.app);
        let changed = MANIFEST.replace("take a note", "take a longer note");
        std::fs::write(p.root.path().join(FILE), changed).expect("manifest");
        let refused = run_at(&mut p, "note", &json!({"pane": pane}), old).expect_err("refused");
        assert_eq!(refused.code, "conflict");
        assert!(refused.message.contains("changed"), "{}", refused.message);
        assert_eq!(p.read("out"), "");
        assert!(revision(&mut p.app) > old);
    }

    #[test]
    fn a_missing_revision_is_malformed() {
        let mut p = plugged(true);
        let (_, _, _, pane) = ids(&p);
        let args = json!({"plugin": "acme.notes", "action": "note", "item": {"pane": pane}});
        assert_eq!(refused(&mut p.app, "plugin-run", &args).code, "malformed");
    }

    #[test]
    fn a_group_or_a_wrong_context_is_malformed() {
        let mut p = plugged(true);
        let (project, ..) = ids(&p);
        assert_eq!(run(&mut p, "note", &json!({"group": 1})).expect_err("group").code, "malformed");
        let wrong = run(&mut p, "note", &json!({"project": project})).expect_err("project");
        assert_eq!(wrong.code, "malformed");
        assert!(wrong.message.contains("pane, tab") && wrong.message.contains("not on a project"), "{}", wrong.message);
    }

    #[test]
    fn unknown_things_are_not_found() {
        let mut p = plugged(true);
        let (_, _, _, pane) = ids(&p);
        assert_eq!(run(&mut p, "nope", &json!({"pane": pane})).expect_err("action").code, "not-found");
        assert_eq!(run(&mut p, "note", &json!({"pane": 9999})).expect_err("pane").code, "not-found");
        let revision = revision(&mut p.app);
        let args = json!({"plugin": "nobody", "action": "note", "revision": revision, "item": {"pane": pane}});
        assert_eq!(refused(&mut p.app, "plugin-run", &args).code, "not-found");
    }

    #[test]
    fn a_plugin_turned_off_is_not_found() {
        let mut p = plugged(true);
        let (_, _, _, pane) = ids(&p);
        let mut registry = Registry::load(&paths::plugins(p.app.config_file.path()));
        registry.set_enabled("acme.notes", false).expect("off");
        registry.save().expect("save");
        let refused = run(&mut p, "note", &json!({"pane": pane})).expect_err("off");
        assert_eq!(refused.code, "not-found");
        assert!(refused.message.contains("is off"), "{}", refused.message);
    }
}
