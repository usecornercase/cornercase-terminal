use std::time::Instant;

use super::*;
use crate::persist::{self, SETTLE, Saver};
use crate::test_util::TempDir;

fn project(dir: &str) -> ProjectState {
    let workspace = WorkspaceState::plain(PathBuf::from(dir), vec![Some(PathBuf::from(dir))], 0);
    ProjectState {
        path: PathBuf::from(dir),
        name: None,
        group: None,
        workspaces: vec![workspace],
        active: 0,
        collapsed: false,
    }
}

fn folded(mut state: State) -> State {
    let active = state.active;
    state.projects.iter_mut().enumerate().for_each(|(i, p)| p.collapsed = i != active);
    state
}

fn state(dirs: &[&str]) -> State {
    State {
        version: VERSION,
        groups: Vec::new(),
        projects: dirs.iter().map(|d| project(d)).collect(),
        active: 0,
        widths: None,
        issues: None,
        changes: None,
        todo: false,
        files: false,
    }
}

mod file {
    use super::*;

    #[test]
    #[expect(clippy::too_many_lines, reason = "pin every version 5 field explicitly against the fixture")]
    fn version_five_matches_the_saved_format() {
        use crate::issues::Who;
        use crate::split::Dir;

        let expected = State {
            version: 5,
            groups: vec![
                GroupEntry { name: "work".into(), icon: '●', colour: 99, collapsed: false },
                GroupEntry { name: "archive".into(), icon: '◆', colour: 33, collapsed: true },
            ],
            projects: vec![
                ProjectState {
                    path: "projects/loose".into(),
                    name: Some("Notes".into()),
                    group: None,
                    workspaces: vec![WorkspaceState {
                        path: "projects/loose".into(),
                        name: Some("planning".into()),
                        worktree: false,
                        tabs: vec![TabState {
                            name: Some("notes".into()),
                            panes: vec![PaneState { cwd: None, right_clicks: false, agent: None }],
                            active: 0,
                            layout: None,
                        }],
                        active: 0,
                        base: Some("main".into()),
                        collapsed: true,
                    }],
                    active: 0,
                    collapsed: false,
                },
                ProjectState {
                    path: "projects/shop".into(),
                    name: Some("Shop".into()),
                    group: Some(0),
                    workspaces: vec![WorkspaceState {
                        path: "worktrees/returns".into(),
                        name: Some("returns".into()),
                        worktree: true,
                        tabs: vec![TabState {
                            name: Some("agent and shell".into()),
                            panes: vec![
                                PaneState {
                                    cwd: Some("worktrees/returns".into()),
                                    right_clicks: true,
                                    agent: Some(AgentState {
                                        kind: "claude".into(),
                                        conversation: "conversation-plan".into(),
                                        mode: Some("plan".into()),
                                    }),
                                },
                                PaneState {
                                    cwd: Some("worktrees/returns/src".into()),
                                    right_clicks: false,
                                    agent: Some(AgentState {
                                        kind: "codex".into(),
                                        conversation: "conversation-default".into(),
                                        mode: None,
                                    }),
                                },
                            ],
                            active: 1,
                            layout: Some(Node::Split {
                                dir: Dir::Right,
                                ratio: 0.375,
                                first: Box::new(Node::Leaf(0)),
                                second: Box::new(Node::Leaf(1)),
                            }),
                        }],
                        active: 0,
                        base: None,
                        collapsed: false,
                    }],
                    active: 0,
                    collapsed: true,
                },
            ],
            active: 1,
            widths: Some(Widths { projects: 35, workspaces: 24, changes: Some(42), stack: Some(11), agents: Some(7) }),
            issues: Some(IssuesState {
                tab: Some("shortcut".into()),
                closed: true,
                people: People {
                    github: [Who::Me, Who::Person("ana".into())],
                    shortcut: [Who::User { id: "u-1".into(), name: "Bo".into() }, Who::Anyone],
                    linear: [Who::Anyone, Who::Me],
                    jira: [Who::Person("zoe".into()), Who::Anyone],
                    plane: [Who::Anyone, Who::User { id: "u-2".into(), name: "Kai".into() }],
                },
            }),
            changes: Some(ChangesState { open: true, mode: Mode::All, scope: Some(Scope::Every) }),
            todo: true,
            files: true,
        };
        let fixture = crate::test_util::fixture!("session/v5.json");
        let dir = TempDir::new();
        let path = dir.path().join("session.json");
        std::fs::write(&path, fixture).expect("write the session fixture");

        assert_eq!(load(&path), Some(expected.clone()));

        persist::save(&path, &expected).expect("save the session");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read the saved session"),
            fixture.strip_suffix('\n').expect("fixture newline")
        );
    }

    #[test]
    fn round_trips() {
        let tmp = TempDir::new();
        let path = tmp.path().join("nested").join("session.json");

        persist::save(&path, &state(&["/a", "/b"])).expect("save");

        assert_eq!(load(&path), Some(state(&["/a", "/b"])));
    }

    #[rstest::rstest]
    #[case::column_widths(Widths { projects: 40, workspaces: 20, ..Widths::default() })]
    #[case::stacked_line(Widths { stack: Some(12), ..Widths::default() })]
    fn round_trips_the_widths(#[case] widths: Widths) {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let saved = State { widths: Some(widths), ..state(&["/a"]) };

        persist::save(&path, &saved).expect("save");

        assert_eq!(load(&path), Some(saved));
    }

    #[test]
    fn files_without_widths_load_with_none() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        std::fs::write(&path, r#"{"version":3,"projects":[],"active":0}"#).expect("write");

        assert_eq!(load(&path).expect("load").widths, None);
    }

    #[test]
    fn widths_saved_before_the_line_load_without_it() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let text = r#"{"version":4,"projects":[],"active":0,"widths":{"projects":40,"workspaces":20}}"#;
        std::fs::write(&path, text).expect("write");

        let widths = load(&path).expect("load").widths;

        assert_eq!(widths, Some(Widths { projects: 40, workspaces: 20, ..Widths::default() }));
    }

    #[test]
    fn round_trips_names_worktrees_and_tabs() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let mut saved = state(&["/a"]);
        saved.projects[0].name = Some("api".into());
        let mut workspace = WorkspaceState::plain("/wt".into(), vec![Some("/wt".into()), None], 1);
        workspace.name = Some("login".into());
        workspace.worktree = true;
        workspace.tabs[0].name = Some("server".into());
        saved.projects[0].workspaces.push(workspace);

        persist::save(&path, &saved).expect("save");

        assert_eq!(load(&path), Some(saved));
    }

    #[test]
    fn round_trips_the_conversation_of_an_agent_pane() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let mut saved = state(&["/a"]);
        let agent = AgentState { kind: "claude".into(), conversation: "4f2c-91".into(), mode: Some("plan".into()) };
        saved.projects[0].workspaces[0].tabs[0].panes[0].agent = Some(agent);

        persist::save(&path, &saved).expect("save");

        assert_eq!(load(&path), Some(saved));
    }

    #[test]
    fn round_trips_groups() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let mut saved = state(&["/a", "/b"]);
        saved.groups = vec![
            GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false },
            GroupEntry { name: "oss".into(), icon: '★', colour: 99, collapsed: true },
        ];
        saved.projects[1].group = Some(1);

        persist::save(&path, &saved).expect("save");

        assert_eq!(load(&path), Some(saved));
    }

    #[test]
    fn round_trips_what_is_folded() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let mut saved = state(&["/a", "/b"]);
        saved.projects[1].collapsed = true;
        saved.projects[0].workspaces[0].collapsed = true;

        persist::save(&path, &saved).expect("save");

        assert_eq!(load(&path), Some(saved));
    }

    #[test]
    fn version_4_files_open_with_only_the_active_project_unfolded() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let v4 = r#"{"version":4,"projects":[{"path":"/a","workspaces":[{"path":"/a","tabs":[{"panes":[{"cwd":"/a"}]}]}]},
                {"path":"/b","workspaces":[{"path":"/b","tabs":[{"panes":[{"cwd":"/b"}]}]}]}],"active":1}"#;
        std::fs::write(&path, v4).expect("write");

        let collapsed: Vec<bool> = load(&path).expect("load").projects.iter().map(|p| p.collapsed).collect();

        assert_eq!(collapsed, [true, false]);
    }

    #[test]
    fn version_3_files_load_with_every_project_ungrouped() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let v3 = r#"{"version":3,"projects":[{"path":"/a","workspaces":[{"path":"/a","tabs":[{"panes":[{"cwd":"/a"}]}]}]},
                {"path":"/b","workspaces":[{"path":"/b","tabs":[{"panes":[{"cwd":"/b"}]}]}]}],"active":1}"#;
        std::fs::write(&path, v3).expect("write");

        assert_eq!(load(&path), Some(folded(State { active: 1, ..state(&["/a", "/b"]) })));
    }

    #[test]
    fn version_2_files_become_one_project_per_workspace() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let v2 = r#"{"version":2,"workspaces":[{"path":"/a","terminals":[{"cwd":"/a"}]},
                {"path":"/b","name":"api","terminals":[{"cwd":"/b"},{"cwd":"/b/src"}],"active":1}],"active":1}"#;
        std::fs::write(&path, v2).expect("write");

        let mut expected = state(&["/a", "/b"]);
        expected.projects[1].name = Some("api".into());
        expected.projects[1].workspaces[0] =
            WorkspaceState::plain("/b".into(), vec![Some("/b".into()), Some("/b/src".into())], 1);
        expected.active = 1;
        assert_eq!(load(&path), Some(folded(expected)));
    }

    #[test]
    fn version_1_files_become_one_project_per_terminal() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let v1 = r#"{"version":1,"terminals":[{"cwd":"/a"},{"cwd":null},{"cwd":"/b","name":"api"}],"active":2}"#;
        std::fs::write(&path, v1).expect("write");

        let mut expected = state(&["/a", "/b"]);
        expected.projects[1].name = Some("api".into());
        expected.active = 1;
        assert_eq!(load(&path), Some(folded(expected)));
    }

    #[test]
    fn missing_file_loads_nothing() {
        let tmp = TempDir::new();
        assert_eq!(load(&tmp.path().join("session.json")), None);
    }

    #[test]
    fn corrupt_file_loads_nothing() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        std::fs::write(&path, "{ not json").expect("write");

        assert_eq!(load(&path), None);
    }

    #[test]
    fn other_versions_load_nothing() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        persist::save(&path, &State { version: VERSION + 1, ..state(&["/a"]) }).expect("save");

        assert_eq!(load(&path), None);
    }
}

mod saver {
    use super::*;

    fn saver(tmp: &TempDir) -> (Saver<State>, PathBuf) {
        let path = tmp.path().join("session.json");
        (Saver::new(path.clone(), None), path)
    }

    #[test]
    fn waits_for_the_state_to_settle() {
        let tmp = TempDir::new();
        let (mut saver, path) = saver(&tmp);
        let t0 = Instant::now();

        saver.observe(state(&["/a"]), t0).expect("observe");
        saver.observe(state(&["/a"]), t0 + SETTLE / 2).expect("observe");

        assert_eq!(load(&path), None);
    }

    #[test]
    fn saves_a_settled_state() {
        let tmp = TempDir::new();
        let (mut saver, path) = saver(&tmp);
        let t0 = Instant::now();

        let first = saver.observe(state(&["/a"]), t0).expect("observe");
        let settled = saver.observe(state(&["/a"]), t0 + SETTLE).expect("observe");

        assert_eq!((first, settled), (false, true));
        assert_eq!(load(&path), Some(state(&["/a"])));
    }

    #[test]
    fn a_change_restarts_the_wait() {
        let tmp = TempDir::new();
        let (mut saver, path) = saver(&tmp);
        let t0 = Instant::now();

        saver.observe(state(&["/a"]), t0).expect("observe");
        saver.observe(state(&[]), t0 + SETTLE / 2).expect("observe");
        saver.observe(state(&[]), t0 + SETTLE).expect("observe");

        assert_eq!(load(&path), None);
    }

    #[test]
    fn does_not_rewrite_an_unchanged_state() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        let mut saver = Saver::new(path.clone(), Some(state(&["/a"])));
        let t0 = Instant::now();

        saver.observe(state(&["/a"]), t0).expect("observe");
        saver.observe(state(&["/a"]), t0 + SETTLE).expect("observe");

        assert!(!path.exists());
    }

    #[test]
    fn flushing_saves_a_state_that_has_not_settled_yet() {
        let tmp = TempDir::new();
        let (mut saver, path) = saver(&tmp);

        saver.observe(state(&["/a"]), Instant::now()).expect("observe");
        saver.flush().expect("flush");

        assert_eq!(load(&path), Some(state(&["/a"])));
    }
}

mod mapping {
    use super::*;
    use crate::split::Dir;
    use crate::test_util::spawn_term;

    fn split_tab() -> Tab {
        let mut tab = Tab::new(5, Some("dev".into()), spawn_term(6, &[], None).0);
        tab.split(6, Dir::Right, spawn_term(7, &[], None).0, true);
        tab.active = 1;
        tab.right_clicks = vec![7];
        tab
    }

    fn saved_tab(panes: usize, active: usize) -> TabState {
        let pane = PaneState { cwd: None, right_clicks: false, agent: None };
        TabState { name: None, panes: vec![pane; panes], active, layout: None }
    }

    #[test]
    fn a_tab_comes_back_with_its_layout_active_pane_and_right_clicks() {
        let tab = split_tab();
        let saved = TabState::of(&tab);
        assert_eq!(saved.layout, tab.layout_by_index());
        let panes = vec![spawn_term(16, &[], None).0, spawn_term(17, &[], None).0];
        let back = saved.restore(15, panes);
        assert_eq!((back.id, back.name.as_deref(), back.active), (15, Some("dev"), 1));
        assert_eq!(
            back.layout,
            Node::Split {
                dir: Dir::Right,
                ratio: 0.5,
                first: Box::new(Node::Leaf(16)),
                second: Box::new(Node::Leaf(17))
            }
        );
        assert_eq!(back.right_clicks, [17]);
    }

    #[test]
    fn a_single_pane_tab_saves_no_layout() {
        let tab = Tab::new(5, None, spawn_term(6, &[], None).0);
        assert_eq!(TabState::of(&tab).layout, None);
    }

    #[test]
    fn a_project_keeps_its_group_position_folds_and_active_workspace() {
        let mut project = Project::new(1, "/p".into(), Some("name".into()));
        let mut first = Workspace::new(2, "/p".into(), None, false);
        first.collapsed = true;
        project.workspaces = vec![first, Workspace::new(3, "/p/w".into(), Some("w".into()), true)];
        project.active = 1;
        project.collapsed = true;
        let saved = ProjectState::of(&project, Some(2));
        assert_eq!((saved.group, saved.active, saved.collapsed), (Some(2), 1, true));
        assert_eq!(saved.name.as_deref(), Some("name"));
        assert_eq!(saved.workspaces.len(), 2);
        assert!(saved.workspaces[0].collapsed);
        assert!(saved.workspaces[1].worktree);
    }

    #[test]
    fn restoring_clamps_an_active_child_that_is_out_of_range() {
        let workspace =
            WorkspaceState { tabs: Vec::new(), active: 9, ..WorkspaceState::plain("/p".into(), Vec::new(), 9) };
        let restored =
            workspace.restore(4, "/p".into(), vec![Tab::restored(5, None, vec![spawn_term(6, &[], None).0], None)]);
        assert_eq!(restored.active, 0);
        let project = ProjectState::of(&Project::new(1, "/p".into(), None), None);
        let project = ProjectState { active: 9, ..project };
        assert_eq!(project.restore(1, "/p".into(), Some(7), vec![restored]).active, 0);
        assert_eq!(saved_tab(2, 9).restore(8, vec![spawn_term(9, &[], None).0, spawn_term(10, &[], None).0]).active, 1);
    }

    #[test]
    fn a_restored_project_keeps_its_name_group_and_fold() {
        let mut project = Project::new(1, "/p".into(), Some("name".into()));
        project.collapsed = true;
        let saved = ProjectState::of(&project, Some(0));
        let back = saved.restore(8, "/q".into(), Some(7), Vec::new());
        assert_eq!((back.id, back.group, back.collapsed), (8, Some(7), true));
        assert_eq!((back.name.as_deref(), back.path.as_path()), (Some("name"), std::path::Path::new("/q")));
    }
}
