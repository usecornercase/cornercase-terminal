use super::*;

fn parent(app: &App, id: u64) -> Option<u64> {
    pane(app, id).parent
}

fn made_pane(response: Response) -> u64 {
    done(response).ids.pane.expect("the new pane")
}

fn split() -> Command {
    Command::Split(wire::Split { down: true, ..wire::Split::default() })
}

fn ask_as_another_server(app: &mut App, caller: u64, command: Command) {
    let text = serde_json::to_string(&wire::Request {
        caller: Some(caller),
        server: Some("another".into()),
        contract: None,
        boot: None,
        view: None,
        command,
    })
    .expect("json");
    app.request(CLIENT, &text, Some(AREA), Instant::now());
}

fn agent_in_a_repository() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let repo = git_repo(&[("README", "hi")]);
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let fake = config.path().join("claude");
    write_executable(&fake, "#!/bin/sh\nprintf 'ready> '\nread line\n");
    let settings = Config {
        worktrees_dir: worktrees.path().display().to_string(),
        agent_commands: [("claude".to_string(), fake.display().to_string())].into(),
        ..Config::default()
    };
    let (app, rx) = app_in(repo.path(), write_config(config.path(), &settings));
    (app, rx, vec![repo, worktrees, config])
}

fn start(worktree: Option<&str>) -> Command {
    Command::Start(wire::Start {
        agent: Some("claude".into()),
        worktree: worktree.map(str::to_string),
        prompt: Some("go".into()),
        ..wire::Start::default()
    })
}

mod new_tab {
    use super::*;

    #[test]
    fn a_tab_opened_from_a_pane_is_its_child() {
        let (mut app, _rx) = app();
        let caller = first(&app);

        let made = made_pane(now(&mut app, Some(caller), new_tab(None)));

        assert_eq!(parent(&app, made), Some(caller));
    }

    #[test]
    fn a_tab_opened_from_outside_has_no_parent() {
        let (mut app, _rx) = app();

        let made = made_pane(now(&mut app, None, new_tab(None)));

        assert_eq!(parent(&app, made), None);
    }

    #[test]
    fn a_pane_of_another_server_is_not_the_parent() {
        let (mut app, _rx) = app();
        let caller = first(&app);

        ask_as_another_server(&mut app, caller, new_tab(None));
        let made = made_pane(answers(&mut app).remove(0));

        assert_eq!(parent(&app, made), None);
    }
}

mod split {
    use super::*;

    #[test]
    fn a_split_made_from_a_pane_is_its_child() {
        let (mut app, _rx) = app();
        let caller = first(&app);

        let made = made_pane(now(&mut app, Some(caller), split()));

        assert_eq!(parent(&app, made), Some(caller));
    }

    #[test]
    fn a_split_made_from_outside_has_no_parent() {
        let (mut app, _rx) = app();

        let made = made_pane(now(&mut app, None, split()));

        assert_eq!(parent(&app, made), None);
    }
}

mod start {
    use super::*;

    #[test]
    fn an_agent_started_from_a_pane_is_its_child() {
        let (mut app, rx, _dirs) = agent_in_a_repository();
        let caller = first(&app);

        ask(&mut app, Some(caller), start(None));
        let made = made_pane(answered(&mut app, &rx, "the agent starts"));

        assert_eq!(parent(&app, made), Some(caller));
    }

    #[test]
    fn the_event_of_its_pane_names_the_pane_it_was_started_from() {
        let (mut app, rx, _dirs) = agent_in_a_repository();
        let caller = first(&app);
        follow(&mut app, &[]);

        ask(&mut app, Some(caller), start(None));
        let made = made_pane(answered(&mut app, &rx, "the agent starts"));

        let opened: Vec<_> = told(&streamed(&mut app))
            .into_iter()
            .filter(|(what, _)| matches!(what, wire::What::Opened { kind: wire::Kind::Pane, .. }))
            .collect();
        let what = wire::What::Opened { kind: wire::Kind::Pane, parent: Some(caller) };
        assert_eq!(opened.iter().map(|(what, ids)| (what, ids.pane)).collect::<Vec<_>>(), [(&what, Some(made))]);
    }

    #[test]
    fn an_agent_started_from_another_server_has_no_parent() {
        let (mut app, _rx, _dirs) = agent_in_a_repository();
        let caller = first(&app);

        ask_as_another_server(&mut app, caller, start(None));

        let tab = app.projects[0].workspaces[0].tabs.last().expect("the agent's tab");
        let made = tab.pane().expect("its pane").id;
        assert_ne!(made, caller);
        assert_eq!(parent(&app, made), None);
    }

    #[test]
    fn an_agent_started_in_a_new_worktree_keeps_its_parent() {
        let (mut app, rx, _dirs) = agent_in_a_repository();
        let caller = first(&app);

        ask(&mut app, Some(caller), start(Some("feat/x")));
        assert_eq!(answers(&mut app), [], "git makes the worktree first");
        let made = made_pane(answered(&mut app, &rx, "the agent starts in the worktree"));

        let (p, w, _) = app.projects.tab_of_pane(made).expect("the agent's tab");
        assert!(app.projects[p].workspaces[w].worktree, "the agent runs in the new worktree");
        assert_eq!(parent(&app, made), Some(caller));
    }

    #[test]
    fn an_agent_started_in_a_new_worktree_from_outside_has_no_parent() {
        let (mut app, rx, _dirs) = agent_in_a_repository();

        ask(&mut app, None, start(Some("feat/x")));
        let made = made_pane(answered(&mut app, &rx, "the agent starts in the worktree"));

        assert_eq!(parent(&app, made), None);
    }
}

mod live_parent {
    use super::*;

    #[test]
    fn a_pane_whose_parent_is_open_has_that_parent() {
        let (mut app, _rx) = app();
        let caller = first(&app);

        let made = made_pane(now(&mut app, Some(caller), new_tab(None)));

        assert_eq!(App::live_parent(pane(&app, made), &app.pane_ids()), Some(caller));
    }

    #[test]
    fn closing_the_parent_leaves_the_child_without_one() {
        let (mut app, rx) = app();
        let caller = first(&app);
        let made = made_pane(now(&mut app, Some(caller), new_tab(None)));

        done(now(
            &mut app,
            None,
            Command::Close(wire::Close {
                item: Item::Pane(caller),
                remove_worktree: false,
                force: false,
                unlock: false,
            }),
        ));
        refreshing(&mut app, &rx, "the parent's pane is gone", |a| a.projects.pane_by(caller).is_none());

        assert_eq!(pane(&app, made).parent, Some(caller));
        assert_eq!(App::live_parent(pane(&app, made), &app.pane_ids()), None);
    }
}
