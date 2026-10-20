use super::*;
use crate::issues::browser::Tab as IssueTab;
use crate::queue::form::When;
use crate::queue::{self, Place, State as QueueState, Task};
use crate::time;

fn queued_app() -> (App, Receiver<AppEvent>, TempDir, TempDir, TempDir) {
    let (mut app, rx, repo, config, worktrees) = undrawn_queued_app();
    render(&mut app, AREA);
    (app, rx, repo, config, worktrees)
}

fn undrawn_queued_app() -> (App, Receiver<AppEvent>, TempDir, TempDir, TempDir) {
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

fn queue_tab(app: &mut App) {
    app.open_issues(AREA).expect("open issues");
    let Some(Overlay::Issues(b)) = &mut app.overlay else { panic!("issues") };
    b.tab_to(IssueTab::Queue);
    app.sync_queued();
}

fn queue_row(i: u16) -> Position {
    let list = ui::issues_list(ui::issues_area(AREA));
    Position::new(list.x + 2, list.y + i)
}

fn press_button(app: &mut App, label: &str) {
    app.sync_queued();
    let Some(Overlay::Issues(b)) = &app.overlay else { panic!("issues") };
    let labels = b.labels(issues_dialog::room(AREA).buttons);
    let i = labels.iter().position(|l| *l == label).unwrap_or_else(|| panic!("no {label} in {labels:?}"));
    let button = ui::issue_buttons(ui::issues_area(AREA), &labels)[i];
    click(app, button.as_position());
}

mod starts {
    use super::*;

    #[test]
    fn two_entries_due_together_start_two_agents_in_two_worktrees_and_accept_trust() {
        let (mut app, rx, repo, _config, _worktrees) = queued_app();
        let at = time::now() + 3600;
        let first = app.panels.queued.queue.add(task(&app, at, Some("first"))).expect("first");
        let second = app.panels.queued.queue.add(task(&app, at, Some("second"))).expect("second");
        let before = app.focus();
        app.refresh_at(Instant::now(), at);
        refreshing(&mut app, &rx, "both worktrees launch", |app| {
            [first, second].iter().all(|id| {
                matches!(app.panels.queued.queue.entry(*id).expect("entry").state, QueueState::Started { .. })
            })
        });
        let mut panes = Vec::new();
        for branch in ["first", "second"] {
            let workspace = app.projects[0]
                .workspaces
                .iter()
                .find(|workspace| git::branch(&workspace.path).as_deref() == Some(branch))
                .expect("worktree workspace");
            wait_until("the fake gets the prompt", || {
                std::fs::read_to_string(workspace.path.join("got")).is_ok_and(|text| !text.is_empty())
            });
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
        app.panels.queued.queue.add(task(&app, at, None)).expect("add");
        assert_eq!(app.tick_at(Instant::now(), at - 10), Some(Duration::from_secs(10)));
        app.refresh_at(Instant::now(), at - 1);
        assert!(app.agents.launches.is_empty());
        assert_eq!(app.panels.queued.queue.pending(), 1);
        app.refresh_at(Instant::now(), at);
        assert_eq!(app.agents.launches.len(), 1);
        assert_eq!(app.agents.launches[0].trust, Some(agents::launch::Trust::Accepted));
    }

    #[test]
    fn a_server_given_a_size_without_a_window_starts_what_is_due() {
        let (mut app, _rx, _repo, _config, _worktrees) = undrawn_queued_app();
        let at = time::now() + 3600;
        app.panels.queued.queue.add(task(&app, at, None)).expect("add");
        app.refresh_at(Instant::now(), at);
        assert!(app.agents.launches.is_empty());

        app.set_queue_area(AREA);
        app.refresh_at(Instant::now(), at);

        assert_eq!(app.agents.launches.len(), 1);
        assert_eq!(app.panels.queued.queue.pending(), 0);
    }

    #[test]
    fn reports_a_missing_agent_and_does_not_retry_it_every_tick() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let at = time::now() + 3600;
        let mut task = task(&app, at, None);
        task.program = "/nonexistent/queued-agent".into();
        let id = app.panels.queued.queue.add(task).expect("add");
        app.refresh_at(Instant::now(), at);
        assert!(
            matches!(&app.panels.queued.queue.entry(id).expect("entry").state, QueueState::Failed { reason } if reason.contains("not installed"))
        );
        app.refresh_at(Instant::now(), at + 1);
        assert!(app.agents.launches.is_empty());
    }

    #[test]
    fn a_restored_missed_entry_waits_for_start_now() {
        let (mut app, rx, repo, _config, _worktrees) = queued_app();
        let id = app.panels.queued.queue.add(task(&app, 100, None)).expect("add");
        let path = repo.path().join("queue.json");
        crate::persist::save(&path, &app.queue_saved()).expect("save");
        app.set_queue(queue::load(&path, 100 + queue::GRACE + 1));
        app.refresh_at(Instant::now(), 1000);
        assert!(app.agents.launches.is_empty());
        assert_eq!(app.panels.queued.queue.entry(id).expect("entry").state, QueueState::Missed);
        app.run_queued(id, Instant::now()).expect("start now");
        refreshing(&mut app, &rx, "started now", |app| {
            matches!(app.panels.queued.queue.entry(id).expect("entry").state, QueueState::Started { .. })
        });
    }
}

mod forms {
    use super::*;

    fn issue_gh(app: &mut App, config: &TempDir) {
        let gh = crate::test_util::fake_gh(
            config.path(),
            "cat <<JSON\n{\"number\":$3,\"title\":\"Fix login $3\",\"state\":\"OPEN\",\"labels\":[],\"assignees\":[],\"author\":{\"login\":\"ana\"},\"updatedAt\":\"\",\"url\":\"https://github.com/acme/shop/issues/$3\"}\nJSON",
        );
        app.config.gh = gh.display().to_string();
    }

    fn issue_add(app: &App, issue: &str) -> crate::control::QueueAdd {
        crate::control::QueueAdd {
            at: "in 2h".into(),
            agent: Some("fake".into()),
            project: Some(app.projects[0].id),
            issue: Some(issue.into()),
            ..crate::control::QueueAdd::default()
        }
    }

    #[test]
    fn editing_the_issue_moves_the_entry_to_the_new_issues_branch() {
        let (mut app, rx, _repo, config, _worktrees) = queued_app();
        issue_gh(&mut app, &config);
        app.add_queue_request(None, None, None, &issue_add(&app, "#7")).expect("prepare issue 7");
        pump_until(&mut app, &rx, "issue 7 is queued", |app| app.panels.queued.queue.pending() == 1);
        let id = app.panels.queued.queue.entries[0].id;
        let edit = crate::control::QueueAdd { at: String::new(), project: None, ..issue_add(&app, "#9") };
        app.add_queue_request(None, None, Some(id), &edit).expect("prepare issue 9");
        pump_until(&mut app, &rx, "issue 9 replaces it", |app| {
            app.panels.queued.queue.entries[0].task.label.starts_with("#9")
        });
        let entry = &app.panels.queued.queue.entries[0];
        assert_eq!(entry.task.place.branch.as_deref(), Some("issue-9-fix-login-9"));
        assert!(entry.task.prompt.ends_with("/issues/9"), "{}", entry.task.prompt);
    }

    #[test]
    fn an_issue_url_of_another_repository_is_refused() {
        let (mut app, rx, _repo, config, _worktrees) = queued_app();
        issue_gh(&mut app, &config);
        let add = issue_add(&app, "https://github.com/other/shop/issues/7");
        app.add_queue_request(None, None, None, &add).expect("prepare the issue");
        pump_until(&mut app, &rx, "the lookup answers", |app| app.messages.toast.is_some());
        let toast = app.messages.toast.as_ref().expect("toast");
        assert!(toast.message.contains("not an issue of this project's GitHub"), "{}", toast.message);
        assert_eq!(app.panels.queued.queue.entries, []);
    }

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
        issue_gh(&mut app, &config);
        let add = crate::control::QueueAdd {
            label: Some("login fix".into()),
            source: Some("github".into()),
            ..issue_add(&app, "#7")
        };
        assert_eq!(app.add_queue_request(None, None, None, &add).expect("prepare issue"), None);
        pump_until(&mut app, &rx, "the issue is queued", |app| app.panels.queued.queue.pending() == 1);
        let entry = &app.panels.queued.queue.entries[0];
        assert_eq!(entry.task.label, "login fix");
        assert_eq!(entry.task.place.branch.is_some(), repository);
        assert!(entry.task.submit);
        if !repository {
            let id = entry.id;
            let prompt = crate::text::one_line(&entry.task.prompt);
            app.run_queued(id, Instant::now()).expect("start a tab in the plain folder");
            refreshing(&mut app, &rx, "the queued tab starts", |app| {
                matches!(app.panels.queued.queue.entry(id).expect("entry").state, QueueState::Started { .. })
            });
            wait_until("the fake receives the issue prompt", || {
                std::fs::read_to_string(folder.path().join("got")).is_ok_and(|text| !text.is_empty())
            });
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
            assert!(form.worktree());
            app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
            assert!(app.overlay.is_none());
        }
        assert_eq!(app.panels.queued.queue.pending(), Source::ALL.len());
        assert!(app.panels.queued.queue.entries.iter().all(|entry| entry.task.submit));
    }

    #[test]
    fn edits_and_cancels_a_pending_prompt_with_mouse_and_enter() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let id = app.panels.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        queue_tab(&mut app);
        click(&mut app, queue_row(0));
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("edit form") };
        form.prompt = todo::editor::Editor::new("fix logout instead");
        form.when = When::In.combo();
        form.amount = todo::editor::Editor::new("3");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        assert_eq!(app.panels.queued.queue.entry(id).expect("edited").task.prompt, "fix logout instead");
        assert!(matches!(&app.overlay, Some(Overlay::Issues(b)) if b.current() == IssueTab::Queue));
        press_button(&mut app, "remove");
        assert_eq!(app.panels.queued.queue.entries, []);
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
        pump_until(&mut app, &rx, "the reset is scheduled", |app| app.panels.queued.queue.pending() == 1);
        assert_eq!(app.panels.queued.queue.entries[0].task.at, reset);
    }

    #[test]
    fn a_failed_probe_keeps_the_form_and_does_not_add_an_entry() {
        let (mut app, rx, _repo, _config, _worktrees) = queued_app();
        app.config.agent_commands.insert("claude".into(), "/nonexistent/claude-probe".into());
        app.new_queue_form(None);
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new("fix login");
        form.when = When::ClaudeReset.combo();
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        pump_until(
            &mut app,
            &rx,
            "the failed probe is visible",
            |app| matches!(&app.overlay, Some(Overlay::Queue(form)) if form.error.is_some()),
        );
        assert_eq!(app.panels.queued.queue.entries, []);
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
        let id = app.panels.queued.queue.add(original.clone()).expect("add");
        app.config.agent_commands.remove("fake");
        app.config.agent_modes.remove("fake");
        app.add_queue_request(None, None, Some(id), &crate::control::QueueAdd::default()).expect("keep captured agent");
        let edited = &app.panels.queued.queue.entry(id).expect("entry").task;
        assert_eq!(
            (&edited.agent, &edited.mode, &edited.command),
            (&original.agent, &original.mode, &original.command)
        );
        app.new_queue_form(Some(id));
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        assert_eq!(form.agent.label(), Some(original.agent.as_str()));
        assert_eq!(form.mode.label(), Some("careful"));
        form.prompt = todo::editor::Editor::new("fix logout instead");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        assert!(app.overlay.is_none());
        let edited = &app.panels.queued.queue.entry(id).expect("edited").task;
        assert_eq!(
            (&edited.agent, &edited.mode, &edited.command),
            (&original.agent, &original.mode, &original.command)
        );
    }

    #[test]
    fn editing_keeps_no_mode_when_the_default_mode_changes() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let original = task(&app, time::now() + 3600, None);
        let id = app.panels.queued.queue.add(original.clone()).expect("add");
        app.config.agent_modes.insert("fake".into(), [("careful".into(), vec!["--careful".into()])].into());
        app.config.agent_args.insert("fake".into(), vec!["--careful".into()]);
        app.config.submit = false;
        app.add_queue_request(None, None, Some(id), &crate::control::QueueAdd::default()).expect("keep captured mode");
        let edited = &app.panels.queued.queue.entry(id).expect("entry").task;
        assert_eq!((&edited.mode, &edited.command), (&original.mode, &original.command));
        app.new_queue_form(Some(id));
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new("fix logout instead");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        assert!(app.overlay.is_none());
        let edited = &app.panels.queued.queue.entry(id).expect("edited").task;
        assert_eq!((&edited.mode, &edited.command), (&original.mode, &original.command));
        assert_eq!(edited.submit, original.submit);
    }

    #[test]
    fn records_an_interrupted_start_before_creating_a_terminal() {
        let (mut app, _rx, repo, _config, _worktrees) = queued_app();
        let path = repo.path().join("queue.json");
        app.set_queue_path(path.clone());
        let id = app.panels.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
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
        let id = app.panels.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        assert!(app.run_queued(id, Instant::now()).expect_err("save must fail").contains("cannot save"));
        assert_eq!(app.agents.launches.len(), 0);
        assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
        assert!(
            matches!(&app.panels.queued.queue.entry(id).expect("entry").state, QueueState::Failed { reason } if reason.contains("cannot save"))
        );
    }

    #[test]
    fn editing_a_prompt_preserves_the_exact_instant_and_captured_command() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let original = task(&app, time::now() + 3679, None);
        let id = app.panels.queued.queue.add(original.clone()).expect("add");
        app.config.agent_args.insert("fake".into(), vec!["changed".into()]);
        let add = crate::control::QueueAdd { prompt: Some("fix logout".into()), ..crate::control::QueueAdd::default() };
        app.add_queue_request(None, None, Some(id), &add).expect("edit");
        let edited = &app.panels.queued.queue.entry(id).expect("edited").task;
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
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        assert!(matches!(&app.overlay, Some(Overlay::Queue(form)) if form.error.is_some()));
        assert_eq!(app.panels.queued.queue.entries, []);
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
        assert_eq!(form.choice().expect("a place").place.workspace_id, Some(workspace_id));
        form.prompt = todo::editor::Editor::new("fix login");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        let id = app.panels.queued.queue.entries[0].id;
        app.projects[0].workspaces[1].name = Some("renamed notes".into());
        app.run_queued(id, Instant::now()).expect("start in the same workspace");
        assert_eq!((app.projects[0].workspaces[0].tabs.len(), app.projects[0].workspaces[1].tabs.len()), (1, 2));
    }

    #[test]
    fn editing_an_entry_whose_project_closed_requires_an_explicit_place() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let mut task = task(&app, time::now() + 3600, None);
        task.place.project = PathBuf::from("/nonexistent/queued-project");
        let id = app.panels.queued.queue.add(task).expect("add");
        app.new_queue_form(Some(id));
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        assert!(
            matches!(&app.overlay, Some(Overlay::Queue(form)) if form.place.chosen().is_none() && form.error.is_some())
        );
        assert_eq!(
            app.panels.queued.queue.entry(id).expect("entry").task.place.project,
            PathBuf::from("/nonexistent/queued-project")
        );
    }
}

mod panel {
    use super::*;
    use crate::app::queue_panel::CLIENTS;
    use crate::queue::form::Control;

    fn opened() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
        let (mut app, rx, repo, config, worktrees) = queued_app();
        queue_tab(&mut app);
        (app, rx, vec![repo, config, worktrees])
    }

    fn undo(app: &mut App) {
        let button = ui::toast_button(AREA, app.messages.toast.as_ref().expect("an undo toast").view());
        click(app, button.as_position());
    }

    fn ids(app: &App) -> Vec<u64> {
        app.panels.queued.queue.entries.iter().map(|entry| entry.id).collect()
    }

    #[test]
    fn removing_an_entry_and_clearing_started_ones_can_be_undone() {
        let (mut app, _rx, _dirs) = opened();
        let pending = app.panels.queued.queue.add(task(&app, time::now() + 3600, None)).expect("pending");
        let started = app.panels.queued.queue.add(task(&app, time::now() + 7200, None)).expect("started");
        app.panels.queued.queue.entry_mut(started).expect("entry").state =
            QueueState::Started { pane: 1, server: crate::control::server_token().into() };
        let before = app.panels.queued.queue.entries.clone();

        press_button(&mut app, "remove");
        assert_eq!(ids(&app), [started]);
        undo(&mut app);
        assert_eq!(app.panels.queued.queue.entries, before);

        press_button(&mut app, "clear started");
        assert_eq!(ids(&app), [pending]);
        undo(&mut app);
        assert_eq!(app.panels.queued.queue.entries, before);
    }

    #[test]
    fn a_starting_entry_opens_no_form_and_cannot_be_removed() {
        let (mut app, _rx, _dirs) = opened();
        let id = app.panels.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        app.panels.queued.queue.entry_mut(id).expect("entry").state = QueueState::Starting;
        app.panels.queued.running.insert(CLIENTS + 1, id);
        click(&mut app, queue_row(0));
        assert!(matches!(app.overlay, Some(Overlay::Issues(_))));
        let Some(Overlay::Issues(b)) = &app.overlay else { panic!("issues") };
        assert_eq!(b.labels(usize::MAX), ["new", "cancel"]);
        assert_eq!(app.outbox.notifications.len(), 0);
    }

    #[test]
    fn a_new_entry_comes_back_to_the_queue_tab_and_escape_too() {
        let (mut app, _rx, _dirs) = opened();
        press_button(&mut app, "new");
        assert!(matches!(app.overlay, Some(Overlay::Queue(_))));
        app.queue_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), AREA);
        assert!(matches!(&app.overlay, Some(Overlay::Issues(b)) if b.current() == IssueTab::Queue));
        press_button(&mut app, "new");
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new("fix the login");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        assert_eq!(app.panels.queued.queue.entries.len(), 1);
        let Some(Overlay::Issues(b)) = &app.overlay else { panic!("back in issues") };
        assert_eq!((b.current(), b.queued.len()), (IssueTab::Queue, 1));
    }

    #[test]
    fn a_started_entry_opens_its_pane() {
        let (mut app, _rx, _dirs) = opened();
        let pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
        app.add_tab(0, 0, AREA).expect("a second tab");
        let id = app.panels.queued.queue.add(task(&app, time::now() + 3600, None)).expect("add");
        app.panels.queued.queue.entry_mut(id).expect("entry").state =
            QueueState::Started { pane, server: crate::control::server_token().into() };
        click(&mut app, queue_row(0));
        assert!(app.overlay.is_none());
        assert_eq!(app.term().map(|term| term.id), Some(pane));
    }

    #[test]
    fn shift_tab_goes_back_through_the_fields() {
        let (mut app, _rx, _dirs) = opened();
        app.new_queue_form(None);
        let field = |app: &App| match &app.overlay {
            Some(Overlay::Queue(form)) => form.focus,
            _ => panic!("form"),
        };
        app.queue_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT), AREA);
        assert_eq!(field(&app), Control::Mode);
        app.queue_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), AREA);
        assert_eq!(field(&app), Control::Prompt);
    }

    #[test]
    fn a_click_in_a_scrolled_prompt_lands_on_the_line_it_shows() {
        let (mut app, _rx, _dirs) = opened();
        app.new_queue_form(None);
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.prompt = todo::editor::Editor::new(&"word ".repeat(80));
        let field = ui::queue_form::text(AREA, form, Control::Prompt);
        let start = ui::queue_form::first_line(&form.prompt, field);
        assert!(start > 0);
        click(&mut app, Position::new(field.x, field.y));
        let Some(Overlay::Queue(form)) = &app.overlay else { panic!("form") };
        assert_eq!(form.prompt.position(usize::from(field.width)), (start, 0));
    }

    #[test]
    fn a_late_answer_for_a_form_that_is_gone_leaves_other_dialogs_and_says_why() {
        let (mut app, _rx, _dirs) = opened();
        app.overlay = Some(Overlay::Usage);
        app.queue_form_done(Ok(7));
        assert!(matches!(app.overlay, Some(Overlay::Usage)));
        app.queue_form_done(Err("the plan probe failed".into()));
        assert!(matches!(app.overlay, Some(Overlay::Usage)));
        assert!(app.messages.toast.as_ref().is_some_and(|t| t.message.contains("the plan probe failed")));
    }
}

mod lists {
    use super::*;
    use crate::queue::form::{Control, Form};

    fn form(app: &App) -> Form {
        match &app.overlay {
            Some(Overlay::Queue(form)) => (**form).clone(),
            _ => panic!("the schedule form"),
        }
    }

    #[test]
    fn a_click_opens_a_list_and_another_click_picks_from_it() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        app.new_queue_form(None);
        let when = ui::queue_form::rect(AREA, &form(&app), Control::When);
        click(&mut app, when.as_position());
        assert_eq!(form(&app).opened(), Some(Control::When));
        let list = ui::queue_form::list(AREA, &form(&app)).expect("an open list");
        click(&mut app, Position::new(list.x + 2, list.y + 3));
        let f = form(&app);
        assert_eq!((f.opened(), f.kind(), f.controls()[2]), (None, When::On, Control::Date));
    }

    #[test]
    fn choosing_another_agent_offers_its_modes() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        app.config.agent_modes.insert("fake".into(), [("careful".into(), vec!["--careful".into()])].into());
        app.new_queue_form(None);
        let Some(Overlay::Queue(f)) = &mut app.overlay else { panic!("form") };
        f.agent.choose(0);
        f.focus = Control::Agent;
        for c in "fak".chars() {
            app.queue_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), AREA);
        }
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        let f = form(&app);
        assert_eq!(f.agent.label(), Some("fake"));
        assert_eq!(f.mode.options(), ["default", "careful"]);
    }
}

mod accounts;
