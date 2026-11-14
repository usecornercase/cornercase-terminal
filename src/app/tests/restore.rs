use super::*;
#[cfg(unix)]
use crate::project::group;
use crate::state;

#[cfg(unix)]
fn locked_restore() -> (TempDir, Locked, App, Receiver<AppEvent>, WorkspaceState) {
    let (dir, locked) = (TempDir::new(), Locked::new());
    let (app, rx) = empty_app();
    let ws = with_a_locked_pane(dir.path(), &locked);
    (dir, locked, app, rx, ws)
}

fn project(path: &Path, workspaces: Vec<WorkspaceState>) -> ProjectState {
    ProjectState { path: path.to_path_buf(), name: None, group: None, workspaces, active: 0, collapsed: false }
}

fn saved(projects: Vec<ProjectState>, active: usize) -> State {
    State {
        version: state::VERSION,
        groups: Vec::new(),
        projects,
        active,
        widths: None,
        issues: None,
        changes: None,
        todo: false,
        files: false,
        flow: false,
    }
}

#[rstest::rstest]
#[case::changes_and_todo(true, true, false, (false, true, false))]
#[case::changes_and_files(true, false, true, (false, false, true))]
#[case::todo_and_files(false, true, true, (false, false, true))]
#[case::all_three(true, true, true, (false, false, true))]
fn restoring_multiple_panels_prefers_files_then_todo_then_changes(
    #[case] changes: bool,
    #[case] todo: bool,
    #[case] files: bool,
    #[case] expected: (bool, bool, bool),
) {
    let mut saved = saved(Vec::new(), 0);
    saved.changes = Some(ChangesState { open: changes, mode: changes::Mode::Uncommitted, scope: None });
    saved.todo = todo;
    saved.files = files;
    let (mut app, _rx) = empty_app();

    assert!(app.restore(&saved, AREA));

    assert_eq!(
        (
            app.screen.side == Some(Side::Changes),
            app.screen.side == Some(Side::Todo),
            app.screen.side == Some(Side::Files)
        ),
        expected
    );
    let again = app.state();
    assert_eq!((again.changes.is_some_and(|changes| changes.open), again.todo, again.files), expected);
}

#[cfg(unix)]
#[rstest::rstest]
#[case::changes(true, false, false)]
#[case::todo(false, true, false)]
#[case::files(false, false, true)]
fn restoring_preserves_the_saved_state(#[case] changes: bool, #[case] todo: bool, #[case] files: bool) {
    let dirs: Vec<TempDir> = (0..4).map(|_| TempDir::new()).collect();
    let paths: Vec<PathBuf> = dirs.iter().map(canonical).collect();
    let mut loose = WorkspaceState::plain(paths[0].clone(), vec![Some(paths[0].clone())], 0);
    loose.name = Some("planning".into());
    loose.base = Some("main".into());
    loose.collapsed = true;
    loose.tabs[0].name = Some("notes".into());
    let split = TabState {
        name: Some("split shell".into()),
        panes: vec![
            PaneState {
                cwd: Some(paths[2].clone()),
                parent_conversation: None,
                right_clicks: false,
                agent: None,
                done: false,
                pending: false,
                id: None,
                parent: None,
                collapsed: false,
                accounts: std::collections::BTreeMap::new(),
                closed: None,
                exited_agent: None,
            },
            PaneState {
                cwd: Some(paths[3].clone()),
                parent_conversation: None,
                right_clicks: true,
                agent: None,
                done: false,
                pending: false,
                id: None,
                parent: None,
                collapsed: false,
                accounts: std::collections::BTreeMap::new(),
                closed: None,
                exited_agent: None,
            },
        ],
        active: 1,
        layout: Some(split::Node::Split {
            dir: Dir::Down,
            ratio: 0.375,
            first: Box::new(split::Node::Leaf(0)),
            second: Box::new(split::Node::Leaf(1)),
        }),
    };
    let saved = State {
        version: state::VERSION,
        groups: vec![
            group::GroupEntry { name: "work".into(), icon: group::GROUP_ICONS[0], colour: 99, collapsed: false },
            group::GroupEntry { name: "archive".into(), icon: group::GROUP_ICONS[3], colour: 33, collapsed: true },
        ],
        projects: vec![
            ProjectState { name: Some("Notes".into()), collapsed: true, ..project(&paths[0], vec![loose]) },
            ProjectState {
                name: Some("Shop".into()),
                group: Some(1),
                active: 1,
                collapsed: true,
                ..project(
                    &paths[1],
                    vec![
                        WorkspaceState::plain(paths[1].clone(), vec![Some(paths[1].clone())], 0),
                        WorkspaceState {
                            path: paths[2].clone(),
                            name: Some("returns".into()),
                            worktree: true,
                            tabs: vec![
                                TabState {
                                    name: Some("build".into()),
                                    panes: vec![PaneState {
                                        cwd: Some(paths[2].clone()),
                                        parent_conversation: None,
                                        right_clicks: false,
                                        agent: None,
                                        done: false,
                                        pending: false,
                                        id: None,
                                        parent: None,
                                        collapsed: false,
                                        accounts: std::collections::BTreeMap::new(),
                                        closed: None,
                                        exited_agent: None,
                                    }],
                                    active: 0,
                                    layout: None,
                                },
                                split,
                            ],
                            active: 1,
                            base: Some("release".into()),
                            collapsed: true,
                            run: None,
                            role_worktrees: Vec::new(),
                            run_dirs: Vec::new(),
                            closed: None,
                            discord_post: None,
                        },
                    ],
                )
            },
        ],
        active: 1,
        widths: Some(state::Widths {
            projects: 35,
            workspaces: 24,
            changes: Some(42),
            stack: Some(11),
            agents: Some(7),
        }),
        issues: Some(IssuesState {
            tab: Some("github".into()),
            closed: true,
            people: People { github: [issues::Who::Me, issues::Who::Person("ana".into())], ..People::default() },
        }),
        changes: Some(ChangesState { open: changes, mode: changes::Mode::Commits, scope: Some(changes::Scope::Every) }),
        todo,
        files,
        flow: false,
    };
    let (mut app, _rx) = empty_app();

    assert!(app.restore(&saved, AREA), "every tab comes back");
    wait_until("every shell reports its saved folder", || {
        app.projects.iter().zip(&saved.projects).all(|(project, saved)| {
            project.workspaces.iter().zip(&saved.workspaces).all(|(workspace, saved)| {
                workspace
                    .tabs
                    .iter()
                    .zip(&saved.tabs)
                    .all(|(tab, saved)| tab.panes.iter().zip(&saved.panes).all(|(pane, saved)| pane.cwd() == saved.cwd))
            })
        })
    });

    assert_eq!(app.state(), saved);
}

#[cfg(unix)]
#[test]
fn reopens_projects_workspaces_and_tabs() {
    let (a, b) = (TempDir::new(), TempDir::new());
    let (a_path, b_path) = (canonical(&a), canonical(&b));
    std::fs::create_dir(a_path.join("sub")).expect("create folder");
    let (mut app, _rx) = empty_app();
    let mut second = WorkspaceState::plain(a_path.clone(), vec![Some(a_path.clone()), Some(a_path.join("sub"))], 0);
    second.name = Some("other".into());
    second.active = 1;
    let mut first_project = project(&a_path, vec![WorkspaceState::plain(a_path.clone(), vec![None], 0), second]);
    first_project.active = 1;
    let state =
        saved(vec![first_project, project(&b_path, vec![WorkspaceState::plain(b_path.clone(), vec![None], 0)])], 0);

    assert!(app.restore(&state, AREA), "every tab comes back");

    assert_eq!((app.projects.len(), app.active, app.projects[0].active), (2, 0, 1));
    assert_eq!(workspace_labels(&app), ["default", "other"]);
    wait_until("the active tab starts in its folder", || term(&app, 0).cwd() == Some(a_path.join("sub")));
}

#[test]
fn what_was_folded_stays_folded() {
    let (a, b) = (TempDir::new(), TempDir::new());
    let (a_path, b_path) = (canonical(&a), canonical(&b));
    let (mut app, _rx) = empty_app();
    let mut folded = WorkspaceState::plain(a_path.clone(), vec![None], 0);
    folded.collapsed = true;
    let mut other = project(&b_path, vec![WorkspaceState::plain(b_path.clone(), vec![None], 0)]);
    other.collapsed = true;
    let state =
        saved(vec![project(&a_path, vec![folded, WorkspaceState::plain(a_path.clone(), vec![None], 0)]), other], 0);

    app.restore(&state, AREA);
    let again = app.state();

    let folds = |s: &State| -> Vec<(bool, Vec<bool>)> {
        s.projects.iter().map(|p| (p.collapsed, p.workspaces.iter().map(|w| w.collapsed).collect())).collect()
    };
    assert_eq!(folds(&again), folds(&state));
}

#[test]
fn the_first_draw_keeps_a_folded_active_project_folded() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();
    let mut folded = project(&canonical(&dir), vec![WorkspaceState::plain(canonical(&dir), vec![None], 0)]);
    folded.collapsed = true;
    app.restore(&saved(vec![folded], 0), AREA);

    app.follow(AREA);

    assert!(app.projects[0].collapsed);
}

#[cfg(unix)]
fn with_a_locked_pane(dir: &Path, locked: &Locked) -> WorkspaceState {
    let mut ws = WorkspaceState::plain(dir.to_path_buf(), vec![None, None], 0);
    ws.tabs[0].panes.push(PaneState {
        cwd: Some(locked.path().to_path_buf()),
        parent_conversation: None,
        right_clicks: false,
        agent: None,
        done: false,
        pending: false,
        id: None,
        parent: None,
        collapsed: false,
        accounts: std::collections::BTreeMap::new(),
        closed: None,
        exited_agent: None,
    });
    ws
}

#[cfg(unix)]
#[test]
fn a_tab_whose_shell_cannot_start_is_left_out_and_the_rest_comes_back() {
    let (first, last, locked) = (TempDir::new(), TempDir::new(), Locked::new());
    let (mut app, _rx) = empty_app();
    let one_tab = |path: &Path| project(path, vec![WorkspaceState::plain(path.to_path_buf(), vec![None], 0)]);
    let state = saved(vec![one_tab(first.path()), one_tab(locked.path()), one_tab(last.path())], 0);

    assert!(!app.restore(&state, AREA), "the restore says a tab is missing");

    let tabs: Vec<usize> = app.projects.iter().map(|p| p.workspaces.iter().map(|w| w.tabs.len()).sum()).collect();
    assert_eq!(tabs, [1, 0, 1]);
    assert_eq!(toast(&app), Some("could not restore 1 tab (locked), see server.log"));
}

#[cfg(unix)]
#[test]
fn a_split_tab_with_one_pane_that_cannot_start_is_left_out_whole() {
    let (dir, _locked, mut app, _rx, ws) = locked_restore();

    assert!(!app.restore(&saved(vec![project(dir.path(), vec![ws])], 0), AREA), "the restore says a tab is missing");

    let label = app.project_label(&app.projects[0]);
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
    assert_eq!(toast(&app), Some(format!("could not restore 1 tab ({label}), see server.log").as_str()));
}

#[cfg(unix)]
#[test]
fn a_tab_that_cannot_start_names_the_folder_and_why() {
    let (dir, locked, mut app, _rx, ws) = locked_restore();

    let Err(e) = app.restore_tab(
        &ws.tabs[0],
        session::Restoring { dir: dir.path(), area: AREA, run: None, reopen: false },
        |_| None,
        &mut session::Parents::default(),
    ) else {
        panic!("the second pane cannot start");
    };

    let message = e.to_string();
    assert!(message.contains(&locked.path().display().to_string()), "{message}");
    assert!(message.contains("Permission denied"), "{message}");
}

#[test]
fn skips_projects_whose_folder_is_gone() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();
    let gone = project(Path::new("/nonexistent/folder"), vec![]);

    app.restore(&saved(vec![gone, project(dir.path(), vec![])], 1), AREA);

    assert_eq!((app.projects.len(), app.active), (1, 0));
}

#[test]
fn skips_workspaces_whose_folder_is_gone() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();
    let workspaces = vec![
        WorkspaceState::plain(Path::new("/nonexistent/folder").to_path_buf(), vec![None], 0),
        WorkspaceState::plain(dir.path().to_path_buf(), vec![None], 0),
    ];

    app.restore(&saved(vec![project(dir.path(), workspaces)], 0), AREA);

    assert_eq!(app.projects[0].workspaces.len(), 1);
}

#[cfg(unix)]
#[test]
fn a_pane_whose_folder_is_gone_opens_in_the_workspace() {
    let dir = TempDir::new();
    let path = canonical(&dir);
    let (mut app, _rx) = empty_app();
    let ws = WorkspaceState::plain(path.clone(), vec![Some(PathBuf::from("/nonexistent/folder"))], 0);

    app.restore(&saved(vec![project(&path, vec![ws])], 0), AREA);

    wait_until("the pane starts in the workspace", || term(&app, 0).cwd() == Some(path.clone()));
}

#[cfg(unix)]
#[test]
fn keeps_the_names() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();
    let mut ws = WorkspaceState::plain(dir.path().to_path_buf(), vec![None], 0);
    ws.name = Some("main line".into());
    ws.tabs[0].name = Some("server".into());
    let mut p = project(dir.path(), vec![ws]);
    p.name = Some("api".into());

    app.restore(&saved(vec![p], 0), AREA);

    let labels = (
        app.project_label(&app.projects[0]),
        workspace_labels(&app),
        app.projects[0].workspaces[0].tabs[0].label(&app.config),
    );
    assert_eq!(labels, ("api".into(), vec!["main line".to_string()], "server".into()));
}

#[test]
fn clamps_the_active_project() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();

    app.restore(&saved(vec![project(dir.path(), vec![])], 5), AREA);

    assert_eq!(app.active, 0);
}

#[cfg(unix)]
#[test]
fn state_lists_projects_workspaces_and_tabs() {
    let (mut app, rx, dirs) = app_with(2);
    click_row(&mut app, WorkspaceRow::NewTab(0));
    type_line(&mut app, "cd /");
    pump_until(&mut app, &rx, "the tab moves", |a| tab_term(a, 0, 1).cwd().as_deref() == Some(Path::new("/")));

    let state = app.state();

    let second = &state.projects[1];
    let tabs = &second.workspaces[0].tabs;
    assert_eq!(
        (state.projects.len(), &second.path, tabs.len(), tabs[1].panes[0].cwd.as_deref(), state.active),
        (2, &canonical(&dirs[1]), 2, Some(Path::new("/")), 1)
    );
}

#[cfg(unix)]
#[test]
fn a_window_that_comes_later_gives_its_colours_to_the_panes_already_open() {
    let (mut app, _rx, _dirs) = app_with(2);
    let background = libghostty_vt::style::RgbColor { r: 0x1d, g: 0x20, b: 0x22 };
    assert_eq!(app.projects.terms().count(), 2);

    app.retheme(HostTheme { background: Some(background), ..HostTheme::default() });

    assert!(app.projects.terms().all(|term| term.emulator.default_background() == Some(background)));
    assert_eq!(app.themes.host().background, Some(background));
}

mod parents {
    use super::*;

    fn conversation(id: &str) -> Conversation {
        Conversation { kind: "claude".into(), conversation: id.into() }
    }

    fn pane(id: Option<u64>, parent: Option<u64>) -> PaneState {
        let parent_conversation = parent.map(|_| conversation("s1"));
        PaneState { id, parent, parent_conversation, ..PaneState::at(None) }
    }

    fn workspace(dir: &Path, panes: Vec<PaneState>) -> WorkspaceState {
        let mut ws = WorkspaceState::plain(dir.to_path_buf(), vec![None; panes.len()], 0);
        for (tab, pane) in ws.tabs.iter_mut().zip(panes) {
            tab.panes = vec![pane];
        }
        ws
    }

    fn parents(app: &App) -> Vec<(u64, Option<u64>)> {
        app.projects.terms().map(|term| (term.id, term.parent.as_ref().map(|p| p.pane))).collect()
    }

    #[cfg(unix)]
    #[test]
    fn children_come_back_under_their_parents_with_new_ids() {
        let (a, b) = (TempDir::new(), TempDir::new());
        let (mut app, _rx) = empty_app();
        let first = project(a.path(), vec![workspace(a.path(), vec![pane(Some(50), Some(40))])]);
        let second = project(b.path(), vec![workspace(b.path(), vec![pane(Some(40), None), pane(None, Some(50))])]);

        assert!(app.restore(&saved(vec![first, second], 0), AREA), "every tab comes back");

        let found = parents(&app);
        let [(child, under_parent), (parent, None), (grandchild, under_child)] = found[..] else {
            panic!("three panes, only the root without a parent: {found:?}");
        };
        assert_eq!((under_parent, under_child), (Some(parent), Some(child)));
        assert!(![40, 50].contains(&parent) && grandchild != child, "the new ids are used");
        let saved: Vec<PaneState> = app
            .state()
            .projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .flat_map(|w| &w.tabs)
            .flat_map(|t| t.panes.clone())
            .collect();
        let ids: Vec<_> = saved.iter().map(|pane| (pane.id, pane.parent)).collect();
        assert_eq!(ids, [(Some(child), Some(parent)), (Some(parent), None), (None, Some(child))]);
    }

    #[cfg(unix)]
    fn fake_claude(app: &mut App, resume: bool) -> TempDir {
        let bin = TempDir::new();
        let fake = bin.path().join("claude");
        crate::test_util::write_executable(&fake, "#!/bin/sh\nsleep 30\n");
        app.config.agent_commands.insert("claude".into(), fake.display().to_string());
        app.config.resume_agents = resume;
        bin
    }

    #[cfg(unix)]
    fn written_folds(app: &App) -> Vec<bool> {
        let workspaces = app.state().projects.into_iter().flat_map(|p| p.workspaces);
        workspaces.flat_map(|w| w.tabs).flat_map(|t| t.panes).map(|pane| pane.collapsed).collect()
    }

    #[cfg(unix)]
    #[rstest::rstest]
    #[case::resuming(true)]
    #[case::without_resume(false)]
    fn a_fold_comes_back_with_its_conversation(#[case] resume: bool) {
        let dir = TempDir::new();
        let (mut app, _rx) = empty_app();
        let _bin = fake_claude(&mut app, resume);
        let parent = PaneState { collapsed: true, agent: Some(resume_state("s1")), ..pane(Some(40), None) };
        let ws = workspace(dir.path(), vec![parent, pane(None, Some(40))]);

        assert!(app.restore(&saved(vec![project(dir.path(), vec![ws])], 0), AREA), "every tab comes back");

        let first = app.projects.terms().next().expect("the folded pane");
        assert_eq!(first.fold, Some(By::Conversation(Conversation::of(&resume_state("s1")))));
        let folds: Vec<bool> = app.projects.terms().map(Term::collapsed).collect();
        assert_eq!(folds, [resume, false]);
        assert_eq!(written_folds(&app), [resume, false]);
    }

    #[cfg(unix)]
    #[test]
    fn a_fold_saved_without_a_conversation_is_dropped() {
        let dir = TempDir::new();
        let (mut app, _rx) = empty_app();
        let ws = workspace(dir.path(), vec![PaneState { collapsed: true, ..pane(Some(40), None) }]);

        assert!(app.restore(&saved(vec![project(dir.path(), vec![ws])], 0), AREA));

        let first = app.projects.terms().next().expect("the pane");
        assert_eq!((first.fold.clone(), written_folds(&app)), (None, vec![false]));
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_session_writes_only_folds_that_hold_with_their_conversation() {
        let (mut app, _rx, _dirs) = app_with(1);
        for _ in 0..3 {
            click_row(&mut app, WorkspaceRow::NewTab(0));
        }
        let ids: Vec<u64> = (0..4).map(|t| tab_term(&app, 0, t).id).collect();
        agent_in_front(&mut app, ids[0], Some(4242), Some("c1"));
        agent_in_front(&mut app, ids[1], Some(4243), Some("c1"));
        agent_in_front(&mut app, ids[2], Some(4244), None);
        for id in &ids {
            assert!(app.set_agent_fold(*id, true));
        }
        converse(&mut app, ids[1], Some("c2"));

        assert_eq!(written_folds(&app), [true, false, false, false]);
    }

    #[cfg(unix)]
    fn saved_parents(state: &State) -> Vec<(Option<u64>, Option<u64>)> {
        let workspaces = state.projects.iter().flat_map(|p| &p.workspaces);
        workspaces.flat_map(|w| &w.tabs).flat_map(|t| &t.panes).map(|pane| (pane.id, pane.parent)).collect()
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_session_names_open_parents_and_forgets_closed_ones() {
        let (mut app, rx, _dirs) = app_with(1);
        click_row(&mut app, WorkspaceRow::NewTab(0));
        click_row(&mut app, WorkspaceRow::NewTab(0));
        let ids: Vec<u64> = (0..3).map(|t| tab_term(&app, 0, t).id).collect();
        let tabs = &mut app.projects[0].workspaces[0].tabs;
        tabs[1].panes[0].parent = Some(by_conversation(ids[0], "s1"));
        tabs[2].panes[0].parent = Some(by_conversation(ids[1], "s1"));

        let state = app.state();
        assert_eq!(saved_parents(&state), [(Some(ids[0]), None), (Some(ids[1]), Some(ids[0])), (None, Some(ids[1]))]);
        let (mut restored, _rx) = empty_app();
        assert!(restored.restore(&state, AREA), "every tab comes back");
        let found = parents(&restored);
        let [(root, None), (child, under_root), (_, under_child)] = found[..] else {
            panic!("three panes, only the root without a parent: {found:?}");
        };
        assert_eq!((under_root, under_child), (Some(root), Some(child)));

        app.close_tab(tab_at(&app, 0, 0, 0));
        pump_until(&mut app, &rx, "the parent's tab closes", |a| a.projects.pane_by(ids[0]).is_none());
        assert_eq!(saved_parents(&app.state()), [(Some(ids[1]), None), (None, Some(ids[1]))]);
    }

    #[test]
    fn a_parent_in_a_skipped_project_is_dropped() {
        let dir = TempDir::new();
        let (mut app, _rx) = empty_app();
        let gone = Path::new("/nonexistent/folder");
        let skipped = project(gone, vec![workspace(gone, vec![pane(Some(40), None)])]);
        let kept = project(dir.path(), vec![workspace(dir.path(), vec![pane(None, Some(40))])]);

        app.restore(&saved(vec![skipped, kept], 1), AREA);

        assert!(parents(&app).iter().all(|(_, parent)| parent.is_none()));
    }

    #[cfg(unix)]
    #[test]
    fn a_parent_not_older_than_its_child_is_dropped() {
        let dir = TempDir::new();
        let (mut app, _rx) = empty_app();
        let panes = vec![pane(Some(5), Some(7)), pane(Some(7), Some(5)), pane(Some(9), Some(9))];

        app.restore(&saved(vec![project(dir.path(), vec![workspace(dir.path(), panes)])], 0), AREA);

        let found = parents(&app);
        assert_eq!(found.iter().map(|(_, parent)| *parent).collect::<Vec<_>>(), [None, Some(found[0].0), None]);
    }

    #[cfg(unix)]
    #[test]
    fn a_parent_saved_without_its_conversation_is_dropped() {
        let dir = TempDir::new();
        let (mut app, _rx) = empty_app();
        let older = PaneState { parent: Some(40), ..pane(None, None) };
        let panes = vec![pane(Some(40), None), older];

        assert!(app.restore(&saved(vec![project(dir.path(), vec![workspace(dir.path(), panes)])], 0), AREA));

        assert!(parents(&app).iter().all(|(_, parent)| parent.is_none()));
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_session_writes_only_parents_that_outlive_the_server() {
        let (mut app, _rx, _dirs) = app_with(1);
        for _ in 0..4 {
            click_row(&mut app, WorkspaceRow::NewTab(0));
        }
        let ids: Vec<u64> = (0..5).map(|t| tab_term(&app, 0, t).id).collect();
        let shell = tab_term(&app, 0, 0).shell_pid().expect("a shell");
        let tabs = &mut app.projects[0].workspaces[0].tabs;
        tabs[1].panes[0].parent = Some(by_conversation(ids[0], "s1"));
        tabs[2].panes[0].parent = Some(Parent { pane: ids[0], by: By::Agent(shell) });
        tabs[3].panes[0].parent = Some(Parent { pane: ids[0], by: By::Shell(shell) });
        tabs[4].panes[0].parent = Some(Parent::pane(ids[0]));

        let state = app.state();

        let panes: Vec<&PaneState> =
            state.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).flat_map(|t| &t.panes).collect();
        let written: Vec<(Option<u64>, bool)> =
            panes.iter().map(|pane| (pane.parent, pane.parent_conversation.is_some())).collect();
        let first = Some(ids[0]);
        assert_eq!(written, [(None, false), (first, true), (None, false), (None, false), (first, false)]);
        assert_eq!(panes[0].id, first);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_comes_back_under_its_parent_when_that_conversation_resumes() {
        let dir = TempDir::new();
        let (mut app, _rx) = empty_app();
        let _bin = fake_claude(&mut app, true);
        let parent = PaneState { agent: Some(resume_state("s1")), ..pane(Some(40), None) };
        let (mine, other) =
            (pane(None, Some(40)), PaneState { parent_conversation: Some(conversation("s0")), ..pane(None, Some(40)) });
        let ws = workspace(dir.path(), vec![parent, mine, other]);

        assert!(app.restore(&saved(vec![project(dir.path(), vec![ws])], 0), AREA));

        let live = app.running_panes();
        let found: Vec<Option<u64>> = app.projects.terms().map(|term| App::live_parent(term, &live)).collect();
        let root = app.projects.terms().next().expect("the parent").id;
        assert_eq!(found, [None, Some(root), None]);
    }
}
