use std::path::{Path, PathBuf};

use rstest::rstest;

use super::*;
use crate::project::Phase;
use crate::project::group::GroupEntry;
use crate::project::split::{Dir, Node};
use crate::test_util::spawn_term;
use crate::ui::{self, SidebarRow};

fn hierarchy() -> Vec<Project> {
    let mut first = Project::new(10, PathBuf::from("/first"), None);
    let mut second = Project::new(20, PathBuf::from("/second"), None);
    for (project, id, tabs) in [(&mut first, 11, vec![(12, vec![13])]), (&mut second, 21, vec![(22, vec![23])])] {
        let mut workspace = Workspace::new(id, project.path.join(id.to_string()), None, false);
        for (id, panes) in tabs {
            let panes = panes.into_iter().map(|id| spawn_term(id, &[], None).0).collect();
            workspace.tabs.push(Tab::restored(id, None, panes, None));
        }
        project.workspaces.push(workspace);
    }
    let mut later = Workspace::new(14, first.path.join("later"), None, true);
    later.tabs.push(Tab::new(15, None, spawn_term(16, &[], None).0));
    let mut split = Tab::new(17, None, spawn_term(18, &[], None).0);
    split.split(18, Dir::Down, spawn_term(19, &[], None).0, true);
    later.tabs.push(split);
    first.workspaces.push(later);
    vec![first, second]
}

fn workspace_at(project: usize, workspace: usize) -> WorkspaceAt {
    WorkspaceAt { project, workspace }
}

fn tab_at(project: usize, workspace: usize, tab: usize) -> TabAt {
    TabAt { workspace: workspace_at(project, workspace), tab }
}

mod lookups {
    use super::*;

    #[test]
    fn find_children_in_later_projects_workspaces_and_tabs() {
        let projects = hierarchy();
        for (project, workspace, tab, pane, (p, w, t)) in [(10, 14, 17, 19, (0, 1, 1)), (20, 21, 22, 23, (1, 0, 0))] {
            let (place, position) = (workspace_at(p, w), tab_at(p, w, t));
            assert_eq!(
                (
                    projects.project_index(project),
                    projects.workspace_index(workspace),
                    projects.workspace_in(project, workspace),
                    projects.tab_index(tab),
                    projects.tab_in(project, workspace, tab),
                    projects.tab_of_pane(pane),
                ),
                (Some(p), Some(place), Some(place), Some(position), Some(position), Some(position))
            );
            let (path, found) = projects.tab_with_pane(pane).expect("tab and workspace folder");
            assert_eq!(
                (
                    projects.pane_by(pane).map(|p| p.id),
                    path.to_path_buf(),
                    found.id,
                    projects[p].index_of(workspace),
                    projects[p].workspaces[w].index_of(tab),
                    found.index_of(pane),
                    found.pane_by(pane).map(|p| p.id),
                ),
                (
                    Some(pane),
                    projects[p].workspaces[w].path.clone(),
                    tab,
                    Some(w),
                    Some(t),
                    Some(found.panes.len() - 1),
                    Some(pane)
                )
            );
        }
    }

    #[test]
    fn reject_unknown_ids_and_ids_from_another_level() {
        let mut projects = hierarchy();
        assert_eq!([999, 14, 17, 19].map(|id| projects.project_index(id)), [None; 4]);
        assert_eq!(
            [999, 10, 17, 19].map(|id| (
                projects.workspace_index(id),
                projects.workspace_in(10, id),
                projects[0].index_of(id)
            )),
            [(None, None, None); 4]
        );
        assert_eq!(
            [999, 10, 14, 19].map(|id| (
                projects.tab_index(id),
                projects.tab_in(10, 14, id),
                projects[0].workspaces[1].index_of(id)
            )),
            [(None, None, None); 4]
        );
        for id in [999, 10, 14, 17] {
            assert_eq!(
                (
                    projects.tab_of_pane(id),
                    projects.tab_with_pane(id).is_none(),
                    projects.tab_with_pane_mut(id).is_none(),
                    projects.pane_by(id).is_none(),
                    projects.pane_by_mut(id).is_none(),
                ),
                (None, true, true, true, true)
            );
            let tab = &mut projects[0].workspaces[1].tabs[1];
            assert_eq!(
                (tab.index_of(id), tab.pane_by(id).is_none(), tab.pane_by_mut(id).is_none()),
                (None, true, true)
            );
        }
        assert_eq!(
            (projects.workspace_in(999, 14), projects.tab_in(999, 14, 17), projects.tab_in(10, 999, 17)),
            (None, None, None)
        );
    }

    #[test]
    fn require_the_named_parent_to_own_the_child() {
        let projects = hierarchy();
        assert_eq!(projects.workspace_in(10, 21), None);
        assert_eq!(projects.workspace_in(20, 14), None);
        assert_eq!(projects.tab_in(10, 21, 22), None);
        assert_eq!(projects.tab_in(20, 14, 17), None);
        assert_eq!(projects.tab_in(10, 11, 17), None);
    }

    #[test]
    fn find_closing_and_removing_items_without_filtering() {
        let mut projects = hierarchy();
        projects[0].closing = true;
        projects[0].workspaces[1].phase = Phase::Removing("later".into());
        projects[1].workspaces[0].phase = Phase::Closing;
        assert_eq!(projects.project_index(10), Some(0));
        for (p, w, t, workspace, tab, pane) in [(0, 1, 1, 14, 17, 19), (1, 0, 0, 21, 22, 23)] {
            let id = projects[p].id;
            assert_eq!(
                (
                    projects.workspace_index(workspace),
                    projects.workspace_in(id, workspace),
                    projects.tab_index(tab),
                    projects.tab_in(id, workspace, tab),
                    projects.tab_of_pane(pane),
                ),
                (
                    Some(workspace_at(p, w)),
                    Some(workspace_at(p, w)),
                    Some(tab_at(p, w, t)),
                    Some(tab_at(p, w, t)),
                    Some(tab_at(p, w, t))
                )
            );
            assert_eq!(
                (
                    projects.tab_with_pane(pane).map(|(_, t)| t.id),
                    projects.pane_by(pane).map(|p| p.id),
                    projects.pane_by_mut(pane).map(|p| p.id),
                    projects.tab_with_pane_mut(pane).map(|(_, t)| t.id),
                ),
                (Some(tab), Some(pane), Some(pane), Some(tab))
            );
        }
    }

    #[test]
    fn mutable_lookups_change_the_same_panes_and_tabs() {
        let mut projects = hierarchy();
        projects.pane_by_mut(19).expect("later split pane").feed(b"later pane");
        let (path, tab) = projects.tab_with_pane_mut(23).expect("later project tab");
        assert_eq!(path, Path::new("/second/21"));
        tab.name = Some("renamed".into());
        tab.pane_by_mut(23).expect("pane by id").feed(b"second project");
        let text = |term: &mut Term| term.emulator.snapshot().expect("snapshot").contents();
        assert!(text(projects.pane_by_mut(19).expect("same pane")).contains("later pane"));
        assert!(text(projects.pane_by_mut(23).expect("same pane")).contains("second project"));
        assert_eq!(projects[1].workspaces[0].tabs[0].name.as_deref(), Some("renamed"));
        assert!(!text(projects.pane_by_mut(18).expect("other split pane")).contains("later pane"));
    }
}

mod places {
    use super::*;

    #[test]
    fn name_their_project_workspace_and_tab() {
        let place = tab_at(0, 1, 1);
        assert_eq!(
            (place.project(), place.workspace(), place.workspace().project(), place.workspace().index(), place.index()),
            (0, workspace_at(0, 1), 0, 1, 1)
        );
    }

    #[test]
    fn reach_the_workspace_and_tab_they_name() {
        let mut projects = hierarchy();
        let (place, tab) = (workspace_at(0, 1), tab_at(1, 0, 0));
        assert_eq!((projects.workspace(place).id, projects.tab(tab).id), (14, 22));
        projects.workspace_mut(place).name = Some("later".into());
        projects.tab_mut(tab).name = Some("second".into());
        assert_eq!(projects[0].workspaces[1].name.as_deref(), Some("later"));
        assert_eq!(projects[1].workspaces[0].tabs[0].name.as_deref(), Some("second"));
    }

    #[test]
    fn come_from_positions_only_when_something_is_there() {
        let projects = hierarchy();
        assert_eq!(
            [(0, 1), (1, 0), (0, 2), (2, 0)].map(|(p, w)| projects.workspace_at(p, w)),
            [Some(workspace_at(0, 1)), Some(workspace_at(1, 0)), None, None]
        );
        assert_eq!(
            [(0, 1, 1), (1, 0, 0), (0, 1, 2), (1, 1, 0), (2, 0, 0)].map(|(p, w, t)| projects.tab_at(p, w, t)),
            [Some(tab_at(0, 1, 1)), Some(tab_at(1, 0, 0)), None, None, None]
        );
    }

    #[test]
    fn follow_the_active_workspace_and_tab() {
        let mut projects = hierarchy();
        projects[0].active = 1;
        projects[0].workspaces[1].active = 1;
        assert_eq!(projects.active_workspace(0), Some(workspace_at(0, 1)));
        assert_eq!(projects.active_tab(workspace_at(0, 1)), Some(tab_at(0, 1, 1)));
        projects[1].active = 5;
        projects[0].workspaces[0].tabs.clear();
        assert_eq!(
            (projects.active_workspace(1), projects.active_workspace(2), projects.active_tab(workspace_at(0, 0))),
            (None, None, None)
        );
    }

    #[test]
    fn find_workspaces_by_id_or_by_what_they_hold() {
        let mut projects = hierarchy();
        assert_eq!(projects.workspace_by(21).map(|w| w.id), Some(21));
        assert!(projects.workspace_by(22).is_none() && projects.workspace_by_mut(999).is_none());
        projects.workspace_by_mut(14).expect("later workspace").name = Some("found".into());
        assert_eq!(projects[0].workspaces[1].name.as_deref(), Some("found"));
        assert_eq!(projects.find_workspace(0, |w| w.path == Path::new("/first/later")), Some(workspace_at(0, 1)));
        assert_eq!(projects.find_workspace(1, |w| w.path == Path::new("/first/later")), None);
        assert_eq!(projects.find_workspace(2, |_| true), None);
    }

    #[test]
    fn walk_a_projects_tabs_with_their_places() {
        let projects = hierarchy();
        let walked: Vec<_> = projects.project_tabs(0).map(|(at, tab)| (at, tab.id)).collect();
        assert_eq!(walked, [(tab_at(0, 0, 0), 12), (tab_at(0, 1, 0), 15), (tab_at(0, 1, 1), 17)]);
    }

    #[test]
    fn name_what_was_just_pushed() {
        let mut projects = hierarchy();
        let place = projects.push_workspace(1, Workspace::new(30, PathBuf::from("/second/new"), None, false));
        let tab = projects.push_tab(place, Tab::new(31, None, spawn_term(32, &[], None).0));
        assert_eq!((place, tab), (workspace_at(1, 1), tab_at(1, 1, 0)));
        assert_eq!((projects.workspace_index(30), projects.tab_of_pane(32)), (Some(place), Some(tab)));
    }
}

mod iterators {
    use super::*;

    #[test]
    fn visit_every_level_in_storage_order_including_closing_items() {
        let mut projects = hierarchy();
        projects[0].closing = true;
        projects[0].workspaces[1].phase = Phase::Removing("later".into());
        let workspaces = [11, 14, 21];
        let tabs = [12, 15, 17, 22];
        let terms = [13, 16, 18, 19, 23];
        assert_eq!(
            (
                projects.workspaces().map(|w| w.id).collect::<Vec<_>>(),
                projects.tabs().map(|t| t.id).collect::<Vec<_>>(),
                projects.terms().map(|t| t.id).collect::<Vec<_>>(),
            ),
            (workspaces.to_vec(), tabs.to_vec(), terms.to_vec())
        );
        assert_eq!(
            (
                projects.workspaces_mut().map(|w| w.id).collect::<Vec<_>>(),
                projects.tabs_mut().map(|t| t.id).collect::<Vec<_>>(),
                projects.terms_mut().map(|t| t.id).collect::<Vec<_>>(),
            ),
            (workspaces.to_vec(), tabs.to_vec(), terms.to_vec())
        );
        for workspace in projects.workspaces_mut() {
            workspace.name = Some("workspace".into());
        }
        for tab in projects.tabs_mut() {
            tab.name = Some("tab".into());
        }
        for term in projects.terms_mut() {
            term.feed(b"all panes");
        }
        assert!(projects.workspaces().all(|w| w.name.as_deref() == Some("workspace")));
        assert!(projects.tabs().all(|t| t.name.as_deref() == Some("tab")));
        assert!(
            projects.terms_mut().all(|t| t.emulator.snapshot().expect("snapshot").contents().contains("all panes"))
        );
    }

    #[test]
    fn an_empty_hierarchy_has_no_children() {
        let mut projects: Vec<Project> = Vec::new();
        assert_eq!(
            (projects.project_index(1), projects.workspace_index(1), projects.tab_index(1), projects.tab_of_pane(1)),
            (None, None, None, None)
        );
        assert!(projects.pane_by(1).is_none() && projects.pane_by_mut(1).is_none());
        assert_eq!(
            [
                projects.workspaces().count(),
                projects.tabs().count(),
                projects.terms().count(),
                projects.workspaces_mut().count(),
                projects.tabs_mut().count(),
                projects.terms_mut().count(),
            ],
            [0; 6]
        );
    }
}

mod layout {
    use super::*;

    #[test]
    fn uses_pane_positions_and_keeps_the_split_tree() {
        let mut projects = hierarchy();
        let tab = &mut projects[0].workspaces[1].tabs[1];
        tab.layout.set_ratio(&[], 0.3);
        tab.panes.swap(0, 1);
        assert_eq!(
            tab.layout_by_index(),
            Some(Node::Split {
                dir: Dir::Down,
                ratio: 0.3,
                first: Box::new(Node::Leaf(1)),
                second: Box::new(Node::Leaf(0)),
            })
        );
        assert_eq!(projects[1].workspaces[0].tabs[0].layout_by_index(), Some(Node::Leaf(0)));
    }

    #[test]
    fn refuses_a_layout_with_a_missing_pane() {
        let mut projects = hierarchy();
        let tab = &mut projects[0].workspaces[1].tabs[1];
        tab.panes.pop();
        assert_eq!(tab.layout_by_index(), None);
    }
}

mod labels {
    use super::*;

    #[test]
    fn use_the_custom_name_else_the_folder_with_the_home_abbreviation() {
        let mut project = Project::new(1, PathBuf::from("/home/example/work/project"), Some("custom".into()));
        assert_eq!(project.label(None), "custom");
        project.name = None;
        assert_eq!(project.label(None), "project");
        project.path = PathBuf::from("/home/example");
        assert_eq!(project.label(Some(Path::new("/home/example"))), "~");
    }
}

mod order {
    use super::*;

    #[rstest]
    #[case::interleaved(vec![Some(2), None, Some(1), Some(2), None, Some(1)], vec![1, 2], vec![1, 4, 2, 5, 0, 3])]
    #[case::missing_group(vec![Some(9), Some(1), None], vec![1], vec![0, 2, 1])]
    #[case::empty_groups(vec![Some(2), None], vec![1, 2, 3], vec![1, 0])]
    #[case::no_groups(vec![Some(9), None], vec![], vec![0, 1])]
    #[case::no_projects(vec![], vec![1, 2], vec![])]
    fn agrees_with_the_unfolded_sidebar_and_ignores_group_folds(
        #[case] memberships: Vec<Option<u64>>,
        #[case] group_ids: Vec<u64>,
        #[case] expected: Vec<usize>,
    ) {
        let projects: Vec<_> = memberships
            .into_iter()
            .enumerate()
            .map(|(p, group)| {
                let mut project = Project::new(100 + u64::try_from(p).expect("project id"), PathBuf::new(), None);
                project.group = group;
                project
            })
            .collect();
        let mut groups: Vec<_> = group_ids
            .into_iter()
            .map(|id| Group {
                id, entry: GroupEntry { name: id.to_string(), icon: '●', colour: 12, collapsed: false }
            })
            .collect();
        let indices = group_indices(&projects, &groups);
        let rows: Vec<_> = ui::sidebar_rows(&indices, &vec![false; groups.len()])
            .into_iter()
            .filter_map(|row| match row {
                SidebarRow::Project(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(sidebar_order(&projects, &groups), expected);
        assert_eq!(rows, expected);
        for group in &mut groups {
            group.entry.collapsed = true;
        }
        assert_eq!(group_indices(&projects, &groups), indices);
        assert_eq!(sidebar_order(&projects, &groups), expected);
    }
}
