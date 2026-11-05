use super::*;
use crate::machines::run_on::{NEEDS_REMOTE, Target};
use crate::machines::{NOT_SEEN, Status};

fn targets(app: &App, project: u64) -> Vec<Target> {
    app.run_on_for(project).expect("a run on row").targets
}

fn whys(app: &App) -> Vec<Option<String>> {
    targets(app, 1).into_iter().map(|target| target.why).collect()
}

fn served(seeing: &mut Seeing, projects: &[(&str, Option<&str>)], wanted: &[&str]) {
    let mut sent = report(projects);
    sent.home = Some("/home/ana".into());
    serve(&mut seeing.theirs, 2, sent);
    seeing.rig.until("the new snapshot arrives", |app| {
        let seen = app.machines.held()[0].seen.as_ref();
        seen.is_some_and(|seen| {
            seen.home.is_some() && seen.projects.iter().map(|p| p.name.as_str()).eq(wanted.iter().copied())
        })
    });
}

mod the_row {
    use super::*;

    #[test]
    fn is_absent_without_machines() {
        let rig = Rig::new(Vec::new(), There::Missing);

        assert_eq!(rig.app.run_on_for(1), None);
    }

    #[test]
    fn lists_each_linked_machine_with_its_icon_and_no_reason() {
        let Seeing { rig, ours, .. } = seeing_two();

        let run_on = rig.app.run_on_for(ours).expect("a run on row");

        let link = rig.app.machines.held()[0].link.id();
        assert_eq!(run_on.targets, [Target { name: "vps".into(), icon: '☁', link, why: None }]);
        assert_eq!((run_on.chosen, run_on.blocked), (None, None));
    }

    #[test]
    fn dims_a_fallen_machine_with_its_reason() {
        let rig = seeing_fall();

        assert_eq!(whys(&rig.app), [Some("unreachable".to_string())]);
    }

    #[test]
    fn dims_a_machine_whose_snapshot_has_not_arrived() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Server("vps-1".into()));

        rig.until("the machine links", |app| app.machines.held()[0].status.state == State::Linked);

        assert_eq!(whys(&rig.app), [Some(NOT_SEEN.to_string())]);
    }

    #[test]
    fn says_why_an_older_machine_cannot_run_it_without_its_destination() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Older);
        rig.until("the machine is older", |app| app.machines.held()[0].status.state == State::Older);
        let failed = "ssh to vps.lan refused the update".to_string();
        let status = rig.app.machines.held()[0].status.clone();
        rig.app.machines.set_status(0, Status { update_failed: Some(failed), ..status });

        assert_eq!(whys(&rig.app), [Some("ssh to vps refused the update".to_string())]);
    }

    #[test]
    fn is_blocked_for_a_project_without_a_git_remote() {
        let Seeing { mut rig, ours, .. } = seeing_two();
        rig.app.projects[0].repo.identity = None;

        let mut run_on = rig.app.run_on_for(ours).expect("a run on row");

        assert_eq!(run_on.blocked, Some(NEEDS_REMOTE));
        assert!(!run_on.choose(Some(0)));
    }
}

mod pairing {
    use super::*;

    #[test]
    fn picks_the_first_project_there_with_the_same_identity() {
        let mut seeing = seeing_two();
        let shouted = SHOP.to_uppercase();
        served(
            &mut seeing,
            &[("blog", None), ("shop-a", Some(&shouted)), ("shop-b", Some(SHOP))],
            &["blog", "shop-a", "shop-b"],
        );
        let link = seeing.rig.app.machines.held()[0].link.id();

        let there = seeing.rig.app.project_there(link, seeing.ours);

        assert_eq!(there.map(|project| project.name.as_str()), Some("shop-a"));
    }

    #[test]
    fn finds_nothing_for_a_project_without_a_git_remote() {
        let Seeing { mut rig, ours, .. } = seeing_two();
        rig.app.projects[0].repo.identity = None;
        let link = rig.app.machines.held()[0].link.id();

        assert_eq!(rig.app.project_there(link, ours), None);
    }
}

mod where_it_starts {
    use super::*;

    #[test]
    fn names_the_machine_and_the_project_s_folder_there() {
        let mut seeing = seeing_two();
        served(&mut seeing, &[("web-shop", Some(SHOP))], &["web-shop"]);
        let (app, ours) = (&seeing.rig.app, seeing.ours);
        let link = app.machines.held()[0].link.id();

        assert_eq!(app.where_there(link, ours, true), "in its own worktree on vps, in ~/web-shop");
        assert_eq!(app.where_there(link, ours, false), "in a tab on vps, in ~/web-shop");
    }

    #[test]
    fn offers_a_clone_when_the_project_is_not_there() {
        let mut seeing = seeing_two();
        served(&mut seeing, &[("blog", None)], &["blog"]);
        let link = seeing.rig.app.machines.held()[0].link.id();

        assert_eq!(seeing.rig.app.where_there(link, seeing.ours, true), "not on vps · clone…");
    }

    #[test]
    fn says_nothing_for_a_machine_that_is_gone() {
        let Seeing { rig, ours, .. } = seeing_two();

        assert_eq!(rig.app.where_there(0, ours, true), "");
    }
}
