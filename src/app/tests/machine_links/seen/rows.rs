use super::*;
use crate::agents::activity::Status;
use crate::ui::{SidebarRow, WorkspaceRow};

fn rows(app: &App) -> Vec<(String, Option<String>)> {
    let view = app.lists_view(AREA, Instant::now());
    let tagged = view.workspaces.iter().map(|w| (w.name.clone(), w.machine.as_ref().map(|m| m.name.clone())));
    tagged.collect()
}

fn seeing_waiting() -> Rig {
    let Seeing { mut rig, mut theirs, .. } = seeing_two();
    let mut sent = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
    let tab = &mut sent.projects[0].workspaces[0].tabs[0];
    tab.status = Some("waiting".into());
    tab.panes[0].model = Some("opus".into());
    tab.panes[0].context = Some(40);
    serve(&mut theirs, 2, sent);
    rig.until("the waiting tab arrives", |app| {
        let seen = app.machines.held()[0].seen.as_ref();
        seen.is_some_and(|seen| seen.projects[0].workspaces[0].tabs[0].status.is_some())
    });
    rig
}

mod entries {
    use super::*;

    #[test]
    fn a_machine_s_workspaces_follow_the_local_ones_of_their_project() {
        let Seeing { rig, .. } = seeing_two();

        let ours: Vec<_> = rig.app.projects.iter().flat_map(|p| &p.workspaces).map(|w| (w.label(), None)).collect();
        let theirs = [("main".to_string(), Some("vps".to_string()))];
        assert_eq!(rows(&rig.app), [ours, theirs.to_vec()].concat());
    }

    #[test]
    fn a_machine_s_tab_has_its_state_and_its_model_and_context() {
        let rig = seeing_waiting();

        let view = rig.app.lists_view(AREA, Instant::now());
        let tab = &view.workspaces.last().expect("the machine's workspace").tabs[0];
        let shown = (tab.status, tab.details.model.as_deref(), tab.details.percent, tab.others);
        assert_eq!(shown, (Some(Status::Waiting), Some("opus"), Some(40), 0));
    }

    #[test]
    fn a_project_only_the_machine_has_comes_last_out_of_the_groups() {
        let Seeing { mut rig, .. } = seeing_two();
        let id = rig.app.take_id();
        let entry = GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false };
        rig.app.groups.push(Group { id, entry });

        let projects = rig.app.project_entries();

        let last = projects.last().expect("the machine's project");
        let tagged = (last.name.as_str(), last.group, last.machine.as_ref().map(|m| m.name.as_str()));
        assert_eq!((projects.len(), tagged), (2, ("blog", None, Some("vps"))));
        let sidebar = rig.app.sidebar_rows();
        assert_eq!(sidebar[sidebar.len() - 2..], [SidebarRow::Gap, SidebarRow::Project(1)]);
    }

    #[test]
    fn the_local_project_counts_the_machine_s_workspaces_and_their_attention() {
        let rig = seeing_waiting();

        let ours = &rig.app.project_entries()[0];
        let local = rig.app.projects.iter().map(|p| p.workspaces.len()).sum::<usize>();
        assert_eq!((ours.workspaces, ours.status, ours.machine.is_none()), (local + 1, Some(Status::Waiting), true));
    }

    #[test]
    fn a_fallen_machine_s_rows_stay_dim_with_when_it_was_seen() {
        let rig = seeing_fall();

        let view = rig.app.lists_view(AREA, Instant::now());
        let tag = view.workspaces.last().and_then(|w| w.machine.clone()).expect("the machine's workspace");
        assert!(tag.dim && tag.seen.as_deref().is_some_and(|ago| ago.ends_with(" s")), "{tag:?}");
    }
}

mod rows_there {
    use super::*;

    #[test]
    fn have_no_new_tab_row() {
        let Seeing { rig, .. } = seeing_two();

        let rows = ui::workspace_rows(&rig.app.tab_lines());

        assert!(rows.contains(&WorkspaceRow::Tab(1, 1)) && !rows.contains(&WorkspaceRow::NewTab(1)), "{rows:?}");
        assert!(rows.contains(&WorkspaceRow::NewTab(0)));
    }

    #[test]
    fn take_no_local_click() {
        let Seeing { mut rig, .. } = seeing_two();
        let before = rig.app.project().map(|p| (p.active, p.workspaces.len()));

        click_row(&mut rig.app, WorkspaceRow::Workspace(1));
        click_close(&mut rig.app, WorkspaceRow::Tab(1, 0));
        let pos = sidebar_pos(&rig.app, SidebarRow::Project(1));
        click(&mut rig.app, pos);

        assert_eq!(rig.app.project().map(|p| (p.active, p.workspaces.len())), before);
        assert_eq!((rig.app.active, rig.app.overlay.is_none()), (0, true));
    }
}
