#![cfg(unix)]

use super::*;

fn parent(app: &App, id: u64) -> Option<u64> {
    pane(app, id).parent.as_ref().map(|p| p.pane)
}

fn link(app: &App, id: u64) -> Option<Parent> {
    pane(app, id).parent.clone()
}

fn reported_parent(app: &App, id: u64) -> Option<u64> {
    let report = app.report(None);
    let mut panes = report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).flat_map(|t| &t.panes);
    panes.find(|pane| pane.id == id).and_then(|pane| pane.parent)
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
        let what = wire::What::Opened {
            kind: wire::Kind::Pane,
            parent: Some(caller),
            accounts: std::collections::BTreeMap::new(),
        };
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

        let (p, w) =
            app.projects.tab_of_pane(made).map(|at| (at.project(), at.workspace().index())).expect("the agent's tab");
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

        assert_eq!(App::live_parent(pane(&app, made), &app.running_panes()), Some(caller));
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

        assert_eq!(parent(&app, made), Some(caller));
        assert_eq!(App::live_parent(pane(&app, made), &app.running_panes()), None);
    }
}

mod holding {
    use super::*;

    fn caller_and_child() -> (App, Receiver<AppEvent>, u64, u64) {
        let (mut app, rx) = app();
        let caller = first(&app);
        let made = made_pane(now(&mut app, Some(caller), new_tab(None)));
        (app, rx, caller, made)
    }

    fn lives(app: &App, child: u64) -> Option<u64> {
        App::live_parent(pane(app, child), &app.running_panes())
    }

    #[test]
    fn a_child_made_from_a_shell_keeps_its_parent_while_no_agent_runs_there() {
        let (mut app, _rx, caller, made) = caller_and_child();
        let shell = pane(&app, caller).shell_pid().expect("a shell");
        assert_eq!(link(&app, made).map(|link| link.by), Some(By::Shell(shell)));

        converse(&mut app, caller, Some("c1"));
        assert_eq!(lives(&app, made), None);

        converse(&mut app, caller, None);
        assert_eq!(lives(&app, made), Some(caller));
    }

    #[test]
    fn another_conversation_in_the_parent_pane_leaves_the_child_without_one() {
        let (mut app, _rx, caller, made) = caller_and_child();
        app.link(made, Some(by_conversation(caller, "c1")));

        let steps = [Some("c1"), Some("c2"), None, Some("c1")];
        let seen: Vec<(Option<u64>, Option<u64>)> = steps
            .into_iter()
            .map(|step| {
                converse(&mut app, caller, step);
                (lives(&app, made), reported_parent(&app, made))
            })
            .collect();

        let (here, gone) = ((Some(caller), Some(caller)), (None, None));
        assert_eq!(seen, [here, gone, gone, here]);
    }

    #[test]
    fn an_agent_restarted_on_another_account_keeps_its_children() {
        let (mut app, _rx, caller, made) = caller_and_child();
        let (kept, lost) = (made_pane(now(&mut app, Some(caller), new_tab(None))), made);
        app.link(kept, Some(by_conversation(caller, "c1")));
        assert_eq!(lives(&app, lost), Some(caller));
        converse(&mut app, caller, Some("c1"));
        let cwd = pane(&app, caller).cwd().expect("a folder");

        assert!(app.respawn_in(caller, cwd, crate::accounts::Accounts::default()).expect("respawned"));
        assert_eq!((lives(&app, kept), lives(&app, lost)), (None, None));
        converse(&mut app, caller, Some("c1"));

        assert_eq!((lives(&app, kept), lives(&app, lost)), (Some(caller), None));
    }

    #[test]
    fn a_process_link_takes_the_conversation_once_read() {
        let (mut app, _rx, caller, made) = caller_and_child();
        app.link(made, Some(Parent { pane: caller, by: By::Agent(4242) }));
        let term = app.projects.pane_by_mut(caller).expect("the pane");
        term.context = context::Pane::watching(4242, crate::agents::Known::Claude, SystemTime::now());
        converse(&mut app, caller, Some("c1"));

        app.learn_conversations();

        assert_eq!(link(&app, made), Some(by_conversation(caller, "c1")));
    }
}

mod an_agent_asking {
    use super::*;

    #[test]
    fn a_pane_started_by_an_agent_belongs_to_its_conversation() {
        let (mut app, rx, _dirs) = app_with(1);
        let fake = FakeClaude::running(FakeClaude::ANSWERING);
        app.config.agent_commands.insert("claude".into(), fake.script().display().to_string());
        app.config.agent_args.insert("claude".into(), vec![fake.dir().display().to_string()]);
        app.agents.set_claude_dir(fake.dir());
        type_line(&mut app, &fake.command_line(""));
        let caller = first(&app);
        refreshing(&mut app, &rx, "the conversation is known", |a| pane(a, caller).resume.is_some());

        let made = made_pane(now(&mut app, Some(caller), new_tab(None)));

        let by = link(&app, made).map(|link| link.by);
        assert_eq!(by, Some(By::Conversation(Conversation { kind: "claude".into(), conversation: "s1".into() })));
        fake.signal("quit");
        refreshing(&mut app, &rx, "the shell is back", |a| pane(a, caller).resume.is_none());
        assert_eq!(App::live_parent(pane(&app, made), &app.running_panes()), None);
        assert_eq!(reported_parent(&app, made), None);
    }
}
