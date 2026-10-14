use super::*;
use crate::queue::{self, Place, State as QueueState, Task};
use crate::time;

fn queued_app() -> (App, Receiver<AppEvent>, TempDir, TempDir, TempDir) {
    let repo = git_repo(&[("README", "hello")]);
    let (config, worktrees) = (TempDir::new(), TempDir::new());
    let agent = config.path().join("fake-agent");
    crate::test_util::write_executable(&agent, issue_list::FAKE_AGENT);
    let settings = Config {
        agent: "fake".into(),
        agent_commands: [("fake".into(), agent.display().to_string())].into(),
        worktrees_dir: worktrees.path().display().to_string(),
        accept_trust_prompts: false,
        ..Config::default()
    };
    let config_path = write_config(config.path(), &settings);
    let (mut app, rx) = app_in(repo.path(), config_path);
    app.issues.env_tokens.clear();
    render(&mut app, AREA);
    (app, rx, repo, config, worktrees)
}

fn task(app: &App, at: i64, branch: Option<&str>) -> Task {
    Task {
        at,
        zone: "UTC".into(),
        label: "fix the login".into(),
        prompt: "fix the login".into(),
        agent: "fake".into(),
        command: agents::command_line(&app.config, "fake"),
        program: agents::command(&app.config, "fake"),
        submit: true,
        place: Place {
            project: app.projects[0].path.clone(),
            workspace: branch.is_none().then(|| app.projects[0].workspaces[0].path.clone()),
            branch: branch.map(str::to_string),
            workspace_name: None,
            workspace_id: None,
            server: String::new(),
        },
        ..Task::default()
    }
}

mod starts {
    use super::*;

    #[test]
    fn two_entries_due_together_start_two_agents_in_two_worktrees_and_accept_trust() {
        let (mut app, rx, repo, _config, _worktrees) = queued_app();
        let at = time::now() + 3600;
        let first = app.queued.queue.add(task(&app, at, Some("first"))).expect("first");
        let second = app.queued.queue.add(task(&app, at, Some("second"))).expect("second");
        let before = app.focus();
        app.refresh_at(Instant::now(), at);
        refreshing(&mut app, &rx, "both worktrees launch", |app| {
            [first, second]
                .iter()
                .all(|id| matches!(app.queued.queue.entry(*id).expect("entry").state, QueueState::Started { .. }))
        });
        let mut panes = Vec::new();
        for branch in ["first", "second"] {
            let workspace = app.projects[0]
                .workspaces
                .iter()
                .find(|workspace| git::branch(&workspace.path).as_deref() == Some(branch))
                .expect("worktree workspace");
            wait_until("the fake gets the prompt", || workspace.path.join("got").exists());
            assert_eq!(std::fs::read_to_string(workspace.path.join("got")).expect("prompt"), "fix the login");
            assert_eq!(std::fs::read_to_string(workspace.path.join("answered")).expect("trust"), "");
            panes.push(workspace.tabs[0].panes[0].id);
        }
        assert_ne!(panes[0], panes[1]);
        assert_eq!(app.focus(), before);
        assert!(!repo.path().join("got").exists());
    }

    #[test]
    fn does_not_start_early_and_wakes_at_the_scheduled_time() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let at = time::now() + 3600;
        app.queued.queue.add(task(&app, at, None)).expect("add");
        assert_eq!(app.tick_at(Instant::now(), at - 10), Some(Duration::from_secs(10)));
        app.refresh_at(Instant::now(), at - 1);
        assert!(app.agents.launches.is_empty());
        assert_eq!(app.queued.queue.pending(), 1);
        app.refresh_at(Instant::now(), at);
        assert_eq!(app.agents.launches.len(), 1);
        assert_eq!(app.agents.launches[0].trust, Some(agents::launch::Trust::Accepted));
    }

    #[test]
    fn reports_a_missing_agent_and_does_not_retry_it_every_tick() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let at = time::now() + 3600;
        let mut task = task(&app, at, None);
        task.program = "/nonexistent/queued-agent".into();
        let id = app.queued.queue.add(task).expect("add");
        app.refresh_at(Instant::now(), at);
        assert!(
            matches!(&app.queued.queue.entry(id).expect("entry").state, QueueState::Failed { reason } if reason.contains("not installed"))
        );
        app.refresh_at(Instant::now(), at + 1);
        assert!(app.agents.launches.is_empty());
    }

    #[test]
    fn a_restored_missed_entry_waits_for_start_now() {
        let (mut app, rx, repo, _config, _worktrees) = queued_app();
        let id = app.queued.queue.add(task(&app, 100, None)).expect("add");
        let path = repo.path().join("queue.json");
        crate::persist::save(&path, &app.queue_saved()).expect("save");
        app.set_queue(queue::load(&path, 100 + queue::GRACE + 1));
        app.refresh_at(Instant::now(), 1000);
        assert!(app.agents.launches.is_empty());
        assert_eq!(app.queued.queue.entry(id).expect("entry").state, QueueState::Missed);
        app.run_queued(id, Instant::now()).expect("start now");
        refreshing(&mut app, &rx, "started now", |app| {
            matches!(app.queued.queue.entry(id).expect("entry").state, QueueState::Started { .. })
        });
    }
}

mod forms {
    use super::*;

    #[rstest::rstest]
    #[case::repository(true)]
    #[case::plain_folder(false)]
    fn issue_lookup_keeps_the_name_and_creates_a_worktree_only_in_a_repository(#[case] repository: bool) {
        let (mut app, rx, _repo, config, _worktrees) = queued_app();
        let folder = TempDir::new();
        if !repository {
            app.projects[0].path = folder.path().to_path_buf();
            app.projects[0].workspaces[0].path = folder.path().to_path_buf();
        }
        let gh = crate::test_util::fake_gh(
            config.path(),
            "cat <<'JSON'\n{\"number\":7,\"title\":\"Fix login\",\"state\":\"OPEN\",\"labels\":[],\"assignees\":[],\"author\":{\"login\":\"ana\"},\"updatedAt\":\"\",\"url\":\"https://github.com/acme/shop/issues/7\"}\nJSON",
        );
        app.config.gh = gh.display().to_string();
        let add = crate::control::QueueAdd {
            at: "in 2h".into(),
            agent: Some("fake".into()),
            project: Some(app.projects[0].id),
            issue: Some("#7".into()),
            label: Some("login fix".into()),
            ..crate::control::QueueAdd::default()
        };
        assert_eq!(app.add_queue_request(None, None, None, &add).expect("prepare issue"), None);
        pump_until(&mut app, &rx, "the issue is queued", |app| app.queued.queue.pending() == 1);
        let entry = &app.queued.queue.entries[0];
        assert_eq!(entry.task.label, "login fix");
        assert_eq!(entry.task.place.branch.is_some(), repository);
        assert!(entry.task.submit);
        if !repository {
            let id = entry.id;
            let prompt = crate::text::one_line(&entry.task.prompt);
            app.run_queued(id, Instant::now()).expect("start a tab in the plain folder");
            refreshing(&mut app, &rx, "the queued tab starts", |app| {
                matches!(app.queued.queue.entry(id).expect("entry").state, QueueState::Started { .. })
            });
            wait_until("the fake receives the issue prompt", || folder.path().join("got").exists());
            assert_eq!(std::fs::read_to_string(folder.path().join("got")).expect("prompt"), prompt);
            assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);
        }
    }

    #[test]
    fn queues_each_trackers_issue_with_its_prompt_and_branch() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        app.config.prompt = "{key} {title} {url}".into();
        for source in Source::ALL {
            let issue = issues::issue(source, 7, "Fix login");
            let place = issues::browser::Place {
                project: app.projects[0].id,
                workspace: None,
                label: "root".into(),
                worktree: true,
            };
            app.schedule_issue(&issue, "fake", Some(place));
            let Some(Overlay::Queue(form)) = &app.overlay else { panic!("queue form") };
            assert!(form.prompt.text().contains("Fix login"));
            assert_eq!(form.branch.text(), issues::branch(&issue));
            assert!(form.worktree);
            app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            assert!(app.overlay.is_none());
            assert_eq!(app.side, Some(Side::Queue));
        }
        assert_eq!(app.queued.queue.pending(), Source::ALL.len());
        assert!(app.queued.queue.entries.iter().all(|entry| entry.task.submit));
    }

    #[test]
    fn edits_and_cancels_a_pending_prompt_with_mouse_and_enter() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let id = app.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        app.toggle_queue();
        let panel = app.layout(AREA).changes;
        let row = ui::queue::rows(panel, &app.queue_view()).item(0);
        click(&mut app, Position::new(row.x + 1, row.y + 1));
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("edit form") };
        form.prompt = todo::editor::Editor::new("fix logout instead");
        form.at = todo::editor::Editor::new("in 3h");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.queued.queue.entry(id).expect("edited").task.prompt, "fix logout instead");
        let row = ui::queue::rows(panel, &app.queue_view()).item(0);
        click(&mut app, Position::new(row.right() - 2, row.y + 1));
        assert_eq!(app.queued.queue.entries, []);
    }
}

mod resets {
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::claude(agents::Known::Claude, 2)]
    #[case::codex(agents::Known::Codex, 3)]
    fn captures_the_reset_from_a_fake_plan_probe(#[case] agent: agents::Known, #[case] reads: usize) {
        let (mut app, rx, _repo, config, _worktrees) = queued_app();
        let reset = time::now() + 3600;
        let answer = if agent == agents::Known::Claude {
            json!({"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"rate_limits_available":true,"rate_limits":{"limits":[{"kind":"session","percent":10,"resets_at":jiff::Timestamp::from_second(reset).expect("stamp").to_string()}]}}}})
        } else {
            json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":10,"windowDurationMins":300,"resetsAt":reset}}}})
        };
        let script = config.path().join(agent.name());
        let input = "read -r line\n".repeat(reads);
        crate::test_util::write_executable(&script, &format!("#!/bin/sh\n{input}printf '%s\\n' '{answer}'\n"));
        app.config.agent_commands.insert(agent.name().into(), script.display().to_string());
        let add = crate::control::QueueAdd {
            at: format!("{} reset", agent.name()),
            zone: "UTC".into(),
            agent: Some("fake".into()),
            prompt: Some("fix the login".into()),
            ..crate::control::QueueAdd::default()
        };
        assert!(app.add_queue_request(None, None, None, &add).expect("prepare").is_none());
        pump_until(&mut app, &rx, "the reset is scheduled", |app| app.queued.queue.pending() == 1);
        assert_eq!(app.queued.queue.entries[0].task.at, reset);
    }

    #[test]
    fn a_failed_probe_keeps_the_form_and_does_not_add_an_entry() {
        let (mut app, rx, _repo, _config, _worktrees) = queued_app();
        app.config.agent_commands.insert("claude".into(), "/nonexistent/claude-probe".into());
        app.new_queue_form(None);
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new("fix login");
        form.at = todo::editor::Editor::new("claude reset");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        pump_until(
            &mut app,
            &rx,
            "the failed probe is visible",
            |app| matches!(&app.overlay, Some(Overlay::Queue(form)) if form.error.is_some()),
        );
        assert_eq!(app.queued.queue.entries, []);
    }
}

mod guarantees {
    use super::*;

    #[test]
    fn editing_keeps_an_agent_and_mode_removed_from_settings() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        app.config.agent_modes.insert("fake".into(), [("careful".into(), vec!["--careful".into()])].into());
        app.config.agent_args.insert("fake".into(), vec!["--careful".into()]);
        let mut original = task(&app, time::now() + 3600, None);
        original.mode = Some("careful".into());
        let id = app.queued.queue.add(original.clone()).expect("add");
        app.config.agent_commands.remove("fake");
        app.config.agent_modes.remove("fake");
        app.add_queue_request(None, None, Some(id), &crate::control::QueueAdd::default()).expect("keep captured agent");
        let edited = &app.queued.queue.entry(id).expect("entry").task;
        assert_eq!(
            (&edited.agent, &edited.mode, &edited.command),
            (&original.agent, &original.mode, &original.command)
        );
        app.new_queue_form(Some(id));
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        assert_eq!(form.kinds[form.agent], original.agent);
        assert_eq!(form.modes[form.mode], "careful");
        form.prompt = todo::editor::Editor::new("fix logout instead");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.overlay.is_none());
        let edited = &app.queued.queue.entry(id).expect("edited").task;
        assert_eq!(
            (&edited.agent, &edited.mode, &edited.command),
            (&original.agent, &original.mode, &original.command)
        );
    }

    #[test]
    fn editing_keeps_no_mode_when_the_default_mode_changes() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let original = task(&app, time::now() + 3600, None);
        let id = app.queued.queue.add(original.clone()).expect("add");
        app.config.agent_modes.insert("fake".into(), [("careful".into(), vec!["--careful".into()])].into());
        app.config.agent_args.insert("fake".into(), vec!["--careful".into()]);
        app.config.submit = false;
        app.add_queue_request(None, None, Some(id), &crate::control::QueueAdd::default()).expect("keep captured mode");
        let edited = &app.queued.queue.entry(id).expect("entry").task;
        assert_eq!((&edited.mode, &edited.command), (&original.mode, &original.command));
        app.new_queue_form(Some(id));
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new("fix logout instead");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.overlay.is_none());
        let edited = &app.queued.queue.entry(id).expect("edited").task;
        assert_eq!((&edited.mode, &edited.command), (&original.mode, &original.command));
        assert_eq!(edited.submit, original.submit);
    }

    #[test]
    fn records_an_interrupted_start_before_creating_a_terminal() {
        let (mut app, _rx, repo, _config, _worktrees) = queued_app();
        let path = repo.path().join("queue.json");
        app.set_queue_path(path.clone());
        let id = app.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        app.run_queued(id, Instant::now()).expect("start");
        let restored = queue::load(&path, time::now());
        assert!(
            matches!(&restored.entry(id).expect("persisted entry").state, QueueState::Failed { reason } if reason.contains("server stopped"))
        );
        assert_eq!(restored.pending(), 0);
    }

    #[test]
    fn refuses_to_start_when_the_interrupted_marker_cannot_be_saved() {
        let (mut app, _rx, repo, _config, _worktrees) = queued_app();
        let path = repo.path().join("queue.json");
        std::fs::create_dir(&path).expect("block the file with a directory");
        app.set_queue_path(path);
        let id = app.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        assert!(app.run_queued(id, Instant::now()).expect_err("save must fail").contains("cannot save"));
        assert_eq!(app.agents.launches.len(), 0);
        assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
        assert!(
            matches!(&app.queued.queue.entry(id).expect("entry").state, QueueState::Failed { reason } if reason.contains("cannot save"))
        );
    }

    #[test]
    fn editing_a_prompt_preserves_the_exact_instant_and_captured_command() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let original = task(&app, time::now() + 3679, None);
        let id = app.queued.queue.add(original.clone()).expect("add");
        app.config.agent_args.insert("fake".into(), vec!["changed".into()]);
        let add = crate::control::QueueAdd { prompt: Some("fix logout".into()), ..crate::control::QueueAdd::default() };
        app.add_queue_request(None, None, Some(id), &add).expect("edit");
        let edited = &app.queued.queue.entry(id).expect("edited").task;
        assert_eq!((edited.at, &edited.command, &edited.place), (original.at, &original.command, &original.place));
    }
}

mod destinations {
    use super::*;

    #[rstest::rstest]
    #[case::project(true)]
    #[case::workspace(false)]
    fn closing_a_place_while_its_form_is_open_does_not_redirect_the_entry(#[case] project: bool) {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        app.new_queue_form(None);
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new("fix login");
        if project {
            app.projects[0].closing = true;
        } else {
            app.projects[0].workspaces.clear();
        }
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(&app.overlay, Some(Overlay::Queue(form)) if form.error.is_some()));
        assert_eq!(app.queued.queue.entries, []);
    }

    #[test]
    fn a_named_workspace_sharing_the_root_folder_keeps_its_own_tab_destination() {
        let (mut app, _rx, repo, _config, _worktrees) = queued_app();
        let workspace =
            app.new_workspace(AREA, repo.path().to_path_buf(), Some("notes".into()), false).expect("workspace");
        let workspace_id = workspace.id;
        app.projects[0].workspaces.push(workspace);
        app.projects[0].active = 1;
        app.new_queue_form(None);
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        assert_eq!(form.places[form.place].place.workspace_id, Some(workspace_id));
        form.prompt = todo::editor::Editor::new("fix login");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let id = app.queued.queue.entries[0].id;
        app.projects[0].workspaces[1].name = Some("renamed notes".into());
        app.run_queued(id, Instant::now()).expect("start in the same workspace");
        assert_eq!((app.projects[0].workspaces[0].tabs.len(), app.projects[0].workspaces[1].tabs.len()), (1, 2));
    }

    #[test]
    fn editing_an_entry_whose_project_closed_requires_an_explicit_place() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let mut task = task(&app, time::now() + 3600, None);
        task.place.project = PathBuf::from("/nonexistent/queued-project");
        let id = app.queued.queue.add(task).expect("add");
        app.new_queue_form(Some(id));
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(&app.overlay, Some(Overlay::Queue(form)) if !form.place_confirmed && form.error.is_some()));
        assert_eq!(
            app.queued.queue.entry(id).expect("entry").task.place.project,
            PathBuf::from("/nonexistent/queued-project")
        );
    }
}
