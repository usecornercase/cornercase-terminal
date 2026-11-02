use super::*;
use crate::control::{PaneInfo, ProjectInfo, TabInfo, WorkspaceInfo};
use crate::machines::link::fake::{request, serve};
use crate::project::group::GroupEntry;

const BOOT: &str = "vps-1";
const SHOP: &str = "github.com/acme/web-shop";

fn watched() -> (Rig, Ends) {
    let ends = Ends::default();
    let rig = Rig::new(vec![machine("vps", "vps.lan")], There::Watched(Arc::clone(&ends)));
    (rig, ends)
}

fn linked(ends: &Ends) -> UnixStream {
    wait_until("the link connects", || !ends.lock().is_empty());
    let mut theirs = ends.lock().remove(0);
    answer(&mut theirs, BOOT, version::CURRENT);
    request(&mut theirs);
    theirs
}

fn report(projects: &[(&str, Option<&str>)]) -> control::Report {
    let info = |(id, (name, repo)): (usize, &(&str, Option<&str>))| ProjectInfo {
        id: u64::try_from(id).expect("an id") + 1,
        name: (*name).into(),
        path: format!("/home/ana/{name}").into(),
        repo: repo.map(str::to_string),
        ..ProjectInfo::default()
    };
    control::Report {
        boot: BOOT.into(),
        projects: projects.iter().enumerate().map(info).collect(),
        ..Default::default()
    }
}

fn worked(mut report: control::Report) -> control::Report {
    for project in &mut report.projects {
        let id = project.id * 10;
        let pane = PaneInfo { id: id + 2, ..PaneInfo::default() };
        let tabs = vec![
            TabInfo { id: id + 1, panes: vec![pane], ..TabInfo::default() },
            TabInfo { id: id + 3, active: true, ..TabInfo::default() },
        ];
        let workspace =
            WorkspaceInfo { id, name: "main".into(), path: project.path.clone(), tabs, ..Default::default() };
        project.workspaces = vec![workspace];
    }
    report
}

fn names(app: &App) -> Option<Vec<String>> {
    let seen = app.machines.held().first()?.seen.as_ref()?;
    Some(seen.projects.iter().map(|p| p.name.clone()).collect())
}

fn seeing(rig: &mut Rig, wanted: &[&str]) {
    rig.until("the snapshot arrives", |app| names(app).is_some_and(|names| names == wanted));
}

struct Seeing {
    rig: Rig,
    theirs: UnixStream,
    ours: u64,
    ends: Ends,
    _dir: TempDir,
}

fn seeing_two() -> Seeing {
    let (mut rig, ends) = watched();
    let dir = TempDir::new();
    rig.app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
    rig.app.projects[0].repo.identity = Some(SHOP.into());
    let ours = rig.app.projects[0].id;
    let mut theirs = linked(&ends);
    serve(&mut theirs, 1, worked(report(&[("web-shop", Some(SHOP)), ("blog", None)])));
    seeing(&mut rig, &["web-shop", "blog"]);
    Seeing { rig, theirs, ours, ends, _dir: dir }
}

fn theirs_at(app: &App) -> usize {
    app.project().map_or(0, |p| p.workspaces.len())
}

fn picked(app: &App) -> Option<(u64, Option<u64>)> {
    app.shown_remote().map(|shown| (shown.workspace, shown.tab))
}

fn with_agent(mut report: control::Report) -> control::Report {
    report.home = Some("/home/ana".into());
    let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[0];
    pane.agent = Some("claude".into());
    pane.status = Some("working".into());
    report.projects[0].workspaces[0].tabs[0].status = Some("working".into());
    report
}

fn seeing_claude() -> Seeing {
    let mut seeing = seeing_two();
    serve(&mut seeing.theirs, 2, with_agent(worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]))));
    seeing.rig.until("claude works there", |app| {
        let seen = app.machines.held()[0].seen.as_ref();
        seen.is_some_and(|seen| seen.home.is_some())
    });
    seeing
}

fn seeing_fall() -> Rig {
    let Seeing { mut rig, theirs, .. } = seeing_two();
    drop(theirs);
    rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);
    rig
}

mod snapshots {
    use super::*;

    #[test]
    fn only_the_latest_one_is_kept() {
        let (mut rig, ends) = watched();
        let mut theirs = linked(&ends);

        serve(&mut theirs, 1, report(&[("web-shop", Some(SHOP))]));
        seeing(&mut rig, &["web-shop"]);
        serve(&mut theirs, 2, report(&[("web-shop", Some(SHOP)), ("blog", None)]));

        seeing(&mut rig, &["web-shop", "blog"]);
        assert_eq!(rig.app.machines.held()[0].seen.as_ref().map(|seen| seen.boot.as_str()), Some(BOOT));
    }

    #[test]
    fn the_last_one_stays_once_the_machine_falls_and_until_it_sends_another() {
        let (mut rig, ends) = watched();
        let mut theirs = linked(&ends);
        serve(&mut theirs, 1, report(&[("web-shop", Some(SHOP))]));
        seeing(&mut rig, &["web-shop"]);

        drop(theirs);
        rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);

        let held = &rig.app.machines.held()[0];
        assert_eq!(held.down_at.map(|(_, failed)| failed), Some(1));
        assert_eq!(names(&rig.app), Some(vec!["web-shop".to_string()]));

        let mut theirs = linked(&ends);
        rig.until("the machine is back", |app| app.machines.held()[0].status.state == State::Linked);
        assert_eq!((rig.app.machines.held()[0].down_at, names(&rig.app)), (None, Some(vec!["web-shop".into()])));

        serve(&mut theirs, 1, report(&[("blog", None)]));
        seeing(&mut rig, &["blog"]);
    }

    #[test]
    fn a_machine_taken_out_of_the_config_forgets_what_it_saw() {
        let (mut rig, ends) = watched();
        let mut theirs = linked(&ends);
        serve(&mut theirs, 1, report(&[("web-shop", Some(SHOP))]));
        seeing(&mut rig, &["web-shop"]);

        rig.edit(Vec::new(), Duration::from_secs(2));
        assert!(rig.app.machines.held().is_empty());
        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(4));

        assert_eq!(rig.app.machines.held().len(), 1);
        assert_eq!(names(&rig.app), None);
    }

    #[test]
    fn a_machine_given_another_destination_forgets_what_it_saw_and_when_it_fell() {
        let (mut rig, ends) = watched();
        let mut theirs = linked(&ends);
        serve(&mut theirs, 1, report(&[("web-shop", Some(SHOP))]));
        seeing(&mut rig, &["web-shop"]);
        drop(theirs);
        rig.until("the machine falls", |app| app.machines.held()[0].down_at.is_some());
        let before = rig.links();

        rig.edit(vec![machine("vps", "vps.example.com")], Duration::from_secs(2));

        let held = &rig.app.machines.held()[0];
        assert_ne!(rig.links(), before);
        assert_eq!((held.destination.as_str(), held.down_at, names(&rig.app)), ("vps.example.com", None, None));
    }
}

mod pairing {
    use super::*;

    fn identities(app: &mut App, repos: &[Option<&str>]) {
        for (p, repo) in repos.iter().enumerate() {
            app.projects[p].repo.identity = repo.map(str::to_string);
        }
    }

    fn ids(app: &App) -> Vec<u64> {
        app.projects.iter().map(|p| p.id).collect()
    }

    #[test]
    fn follows_the_sidebar_s_order_with_the_ungrouped_projects_first() {
        let (mut app, _rx, _dirs) = app_with(3);
        identities(&mut app, &[Some(SHOP), Some(SHOP), None]);
        let id = app.take_id();
        app.groups
            .push(Group { id, entry: GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false } });
        app.projects[0].group = Some(id);
        let ids = ids(&app);

        let paired = app.paired(&report(&[("web-shop", Some(SHOP)), ("blog", None), ("shop-2", Some(SHOP))]));

        assert_eq!(paired, [Some(ids[1]), None, Some(ids[1])]);
    }

    #[test]
    fn leaves_out_a_project_that_is_closing() {
        let (mut app, _rx, _dirs) = app_with(2);
        identities(&mut app, &[Some(SHOP), Some(SHOP)]);
        app.projects[0].closing = true;
        let ids = ids(&app);

        assert_eq!(app.paired(&report(&[("web-shop", Some(SHOP))])), [Some(ids[1])]);
    }
}

mod card;
mod clicks;
mod input;
mod notices;
mod report;
mod rows;
mod selection;
mod stream;
mod tree;
