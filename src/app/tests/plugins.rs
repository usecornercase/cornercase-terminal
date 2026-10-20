use super::*;
use crate::plugins::Registry;
use crate::plugins::manifest::FILE;
use crate::test_util::write_executable;

pub(super) const MANIFEST: &str = r#"id = "acme.notes"
name = "Notes"
version = "1.0"
permissions = ["read"]

[[actions]]
id = "note"
title = "take a note"
contexts = ["pane", "tab"]
command = ["./act"]

[[actions]]
id = "fail"
title = "break"
contexts = ["pane"]
command = ["./fail"]

[[events]]
on = "opened"
command = ["./hook"]
"#;

pub(super) struct Plugged {
    pub(super) app: App,
    pub(super) rx: Receiver<AppEvent>,
    pub(super) root: TempDir,
    _config: TempDir,
    _dirs: Vec<TempDir>,
}

pub(super) fn plugged(enabled: bool) -> Plugged {
    let (config, root, project) = (TempDir::new(), TempDir::new(), TempDir::new());
    std::fs::write(root.path().join(FILE), MANIFEST).expect("manifest");
    let out = root.path().join("out");
    let line = "ctx=$CORNERCASE_CONTEXT action=$CORNERCASE_PLUGIN_ACTION pane=$CORNERCASE_PANE tab=$CORNERCASE_TAB \
                workspace=$CORNERCASE_WORKSPACE project=$CORNERCASE_PROJECT nested=$CORNERCASE";
    write_executable(&root.path().join("act"), &format!("#!/bin/sh\necho \"{line}\" >> '{}'\n", out.display()));
    write_executable(&root.path().join("fail"), "#!/bin/sh\nexit 3\n");
    let events = root.path().join("events");
    write_executable(
        &root.path().join("hook"),
        &format!("#!/bin/sh\necho \"$CORNERCASE_EVENT\" >> '{}'\n", events.display()),
    );
    let config_path = config.path().join("config.json");
    let mut registry = Registry::load(&paths::plugins(&config_path));
    registry.link(root.path()).expect("link");
    registry.set_enabled("acme.notes", enabled).expect("switch");
    registry.save().expect("save");
    let (mut app, rx) = new_app(config_path);
    app.open_project(project.path().to_path_buf(), AREA).expect("open project");
    app.trace();
    app.refresh(Instant::now());
    Plugged { app, rx, root, _config: config, _dirs: vec![project] }
}

impl Plugged {
    pub(super) fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.root.path().join(name)).unwrap_or_default()
    }
}

fn pane_pos() -> Position {
    let pane = areas().pane;
    Position::new(pane.x + pane.width / 2, pane.y + pane.height / 2)
}

mod menus {
    use super::*;

    #[test]
    fn a_pane_lists_the_actions_for_panes_after_its_own() {
        let mut p = plugged(true);
        right_click(&mut p.app, pane_pos());
        let labels = menu_labels(&p.app);
        assert_eq!(labels[labels.len() - 2..], ["take a note".to_string(), "break".to_string()]);
    }

    #[test]
    fn a_tab_lists_only_the_actions_for_tabs() {
        let mut p = plugged(true);
        right_click_row(&mut p.app, WorkspaceRow::Tab(0, 0));
        assert_eq!(menu_labels(&p.app), ["rename tab", "take a note"]);
    }

    #[test]
    fn a_project_lists_none_when_no_action_is_for_projects() {
        let mut p = plugged(true);
        right_click(&mut p.app, entry_pos());
        assert_eq!(menu_labels(&p.app), ["rename project", "copy path"]);
    }

    #[test]
    fn a_plugin_that_is_off_adds_nothing() {
        let mut p = plugged(false);
        right_click_row(&mut p.app, WorkspaceRow::Tab(0, 0));
        assert_eq!(menu_labels(&p.app), ["rename tab"]);
    }

    #[test]
    fn an_action_runs_with_the_ids_of_what_was_clicked() {
        let mut p = plugged(true);
        right_click(&mut p.app, pane_pos());
        pick(&mut p.app, "take a note");
        wait_until("the action ran", || !p.read("out").is_empty());
        let project = &p.app.projects[0];
        let (workspace, tab) = (&project.workspaces[0], &project.workspaces[0].tabs[0]);
        let pane = tab.panes[0].id;
        let expected = format!(
            "ctx=pane action=note pane={pane} tab={} workspace={} project={} nested=1\n",
            tab.id, workspace.id, project.id
        );
        assert_eq!(p.read("out"), expected);
    }

    #[test]
    fn an_action_that_fails_says_so() {
        let mut p = plugged(true);
        right_click(&mut p.app, pane_pos());
        pick(&mut p.app, "break");
        let (app, rx) = (&mut p.app, &p.rx);
        wait_until("the failure comes back", || {
            drain(app, rx);
            toast(app).is_some()
        });
        assert_eq!(
            toast(&p.app),
            Some("Notes: break failed: exit status: 3; `cornercase plugin logs acme.notes` says more")
        );
    }
}

mod hooks {
    use super::*;

    #[test]
    fn opening_a_tab_runs_the_hook_with_the_event_and_nothing_older() {
        let mut p = plugged(true);
        p.app.add_tab(0, 0, AREA).expect("new tab");
        p.app.trace();
        wait_until("both hooks ran", || p.read("events").lines().count() == 2);
        let events: Vec<serde_json::Value> =
            p.read("events").lines().map(|line| serde_json::from_str(line).expect("json")).collect();
        let mut kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap_or_default()).collect();
        kinds.sort_unstable();
        assert_eq!(kinds, ["pane", "tab"]);
        assert!(events.iter().all(|e| e["event"] == "opened"));
    }

    #[test]
    fn a_plugin_that_is_off_hears_nothing() {
        let mut p = plugged(false);
        p.app.add_tab(0, 0, AREA).expect("new tab");
        p.app.trace();
        assert!(!p.app.following());
    }
}

mod settings_page {
    use super::*;

    fn open_plugins(app: &mut App) {
        app.open_settings_on(&crate::settings::Row::Plugin("acme.notes".into()));
    }

    #[test]
    fn it_lists_the_plugin_with_what_it_asks_for() {
        let mut p = plugged(false);
        open_plugins(&mut p.app);
        let Some(Overlay::Settings(s)) = &p.app.overlay else { panic!("settings are open") };
        let text = s.row_text(&crate::settings::Row::Plugin("acme.notes".into()));
        assert_eq!(
            (text.label, text.value, text.note),
            ("Notes 1.0".into(), "[ ] off".into(), "asks for read · runs as you".into())
        );
    }

    #[test]
    fn enter_turns_it_on_and_saves_it() {
        let mut p = plugged(false);
        open_plugins(&mut p.app);
        send_key(&mut p.app, KeyCode::Enter, KeyModifiers::NONE);
        let registry = Registry::load(&paths::plugins(p.app.config_file.path()));
        assert!(registry.entries[0].enabled);
        let Some(Overlay::Settings(s)) = &p.app.overlay else { panic!("settings stay open") };
        assert_eq!(s.notice.as_deref(), Some("Notes is on: it runs as you"));
    }
}

mod services {
    use super::*;

    #[test]
    fn they_start_with_the_server_and_stop_with_it() {
        let p = plugged(true);
        let manifest = format!("{MANIFEST}\n[[service]]\nid = \"sync\"\ncommand = [\"./serve\"]\n");
        std::fs::write(p.root.path().join(FILE), manifest).expect("manifest");
        let pids = p.root.path().join("pids");
        write_executable(
            &p.root.path().join("serve"),
            &format!("#!/bin/sh\necho $$ >> '{}'\nexec sleep 30\n", pids.display()),
        );
        let mut app = p.app;
        app.start_plugins();
        wait_until("the service runs", || std::fs::read_to_string(&pids).is_ok_and(|s| s.ends_with('\n')));
        let pid: i32 = std::fs::read_to_string(&pids).expect("pid").trim().parse().expect("a pid");
        app.stop_plugins();
        assert!(!process::alive(pid));
    }
}
