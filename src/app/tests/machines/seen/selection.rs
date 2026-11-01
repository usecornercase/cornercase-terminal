use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::app::machines::RemoteItem;
use crate::control::ShownRemote;
use crate::shortcuts::Action;

const RESTARTED: &str = "vps-2";

type Shown = (String, u64, u64, Option<u64>, PathBuf);

fn shown(app: &App) -> Option<Shown> {
    let shown = app.shown_remote()?;
    Some((shown.boot, shown.project, shown.workspace, shown.tab, shown.path))
}

fn at(boot: &str, project: u64, tab: u64, name: &str) -> Shown {
    (boot.into(), project, project * 10, Some(tab), format!("/home/ana/{name}").into())
}

fn seen_by() -> (Rig, Ends, UnixStream) {
    let (mut rig, ends) = watched();
    let mut theirs = linked(&ends);
    serve(&mut theirs, 1, worked(report(&[("web-shop", Some(SHOP)), ("blog", None)])));
    seeing(&mut rig, &["web-shop", "blog"]);
    (rig, ends, theirs)
}

fn restarted(projects: &[(&str, Option<&str>)]) -> control::Report {
    control::Report { boot: RESTARTED.into(), ..worked(report(projects)) }
}

fn booted(app: &App, boot: &str) -> bool {
    app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.boot == boot)
}

mod picking {
    use super::*;

    #[test]
    fn a_workspace_there_shows_its_active_tab() {
        let (mut rig, _ends, _theirs) = seen_by();

        assert!(rig.app.select_remote("VPS", RemoteItem::Workspace(20)));

        let picked = rig.app.shown_remote().expect("a machine's workspace");
        assert_eq!((picked.machine.as_str(), picked.link), ("vps", rig.links()[0]));
        assert_eq!(shown(&rig.app), Some(at(BOOT, 2, 23, "blog")));
    }

    #[test]
    fn a_tab_there_shows_that_tab() {
        let (mut rig, _ends, _theirs) = seen_by();

        assert!(rig.app.select_remote("vps", RemoteItem::Tab(11)));

        assert_eq!(shown(&rig.app), Some(at(BOOT, 1, 11, "web-shop")));
    }

    #[rstest]
    #[case::another_machine("pi", RemoteItem::Workspace(10))]
    #[case::a_workspace_it_does_not_have("vps", RemoteItem::Workspace(99))]
    #[case::a_tab_it_does_not_have("vps", RemoteItem::Tab(99))]
    fn nothing_the_machine_does_not_have(#[case] machine: &str, #[case] item: RemoteItem) {
        let (mut rig, _ends, _theirs) = seen_by();

        assert!(!rig.app.select_remote(machine, item));

        assert_eq!(shown(&rig.app), None);
    }

    #[test]
    fn is_in_status_while_the_local_ids_stay_put() {
        let (mut rig, _ends, _theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Tab(11));

        let status = rig.app.report(None);

        let expected = ShownRemote {
            machine: "vps".into(),
            boot: BOOT.into(),
            project: 1,
            workspace: 10,
            tab: Some(11),
            path: "/home/ana/web-shop".into(),
        };
        assert_eq!((status.shown_remote, status.shown), (Some(expected), control::Ids::default()));
    }

    #[rstest]
    #[case::a_place(|app: &mut App, id| app.goto(Goto::Place { project: id, workspace: None, tab: None }))]
    #[case::a_project_s_row(|app: &mut App, id| app.click_row(Target::Project(id)))]
    #[case::the_next_tab_by_key(|app: &mut App, _| { app.go_shortcut(Action::NextTab, AREA); })]
    #[case::a_tab_by_key(|app: &mut App, _| { app.go_shortcut(Action::Tab(0), AREA); })]
    #[case::a_split_by_key(|app: &mut App, _| {
        app.config.prefix_key = "ctrl+]".into();
        send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(app, KeyCode::Char('|'), KeyModifiers::NONE);
    })]
    fn something_here_goes_back_to_it(#[case] pick: fn(&mut App, u64)) {
        let (mut rig, _ends, _theirs) = seen_by();
        let dir = TempDir::new();
        rig.app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
        rig.app.select_remote("vps", RemoteItem::Workspace(10));
        let ours = rig.app.projects[0].id;

        pick(&mut rig.app, ours);

        assert_eq!(shown(&rig.app), None);
    }
}

mod following {
    use super::*;

    #[test]
    fn it_stays_while_the_machine_is_down() {
        let (mut rig, _ends, theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Workspace(20));

        drop(theirs);
        rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);

        assert_eq!(shown(&rig.app), Some(at(BOOT, 2, 23, "blog")));
    }

    #[test]
    fn a_restarted_machine_keeps_it_by_the_workspace_s_folder() {
        let (mut rig, _ends, mut theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Tab(21));

        serve(&mut theirs, 1, restarted(&[("blog", None), ("web-shop", Some(SHOP))]));
        rig.until("the machine restarts", |app| booted(app, RESTARTED));

        assert_eq!(shown(&rig.app), Some(at(RESTARTED, 1, 13, "blog")));
    }

    #[test]
    fn a_restarted_machine_without_that_folder_lets_it_go() {
        let (mut rig, _ends, mut theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Workspace(20));

        serve(&mut theirs, 1, restarted(&[("web-shop", Some(SHOP))]));
        rig.until("the machine restarts", |app| booted(app, RESTARTED));

        assert_eq!(shown(&rig.app), None);
    }

    #[test]
    fn a_workspace_closed_there_lets_it_go() {
        let (mut rig, _ends, mut theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Workspace(20));

        serve(&mut theirs, 2, worked(report(&[("web-shop", Some(SHOP))])));
        seeing(&mut rig, &["web-shop"]);

        assert_eq!(shown(&rig.app), None);
    }

    #[test]
    fn a_tab_closed_there_falls_back_to_the_workspace_s_active_one() {
        let (mut rig, _ends, mut theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Tab(21));
        let mut fewer = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
        fewer.projects[1].workspaces[0].tabs.remove(0);
        fewer.projects[1].name = "blog-2".into();

        serve(&mut theirs, 2, fewer);
        seeing(&mut rig, &["web-shop", "blog-2"]);

        assert_eq!(shown(&rig.app), Some(at(BOOT, 2, 23, "blog")));
    }

    #[test]
    fn giving_the_machine_another_destination_goes_back_to_what_is_here() {
        let (mut rig, _ends, _theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Workspace(20));

        rig.edit(vec![machine("vps", "vps.example.com")], Duration::from_secs(2));

        assert_eq!(shown(&rig.app), None);
    }

    #[test]
    fn taking_the_machine_out_goes_back_to_what_is_here() {
        let (mut rig, ends, _theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Workspace(20));

        rig.edit(Vec::new(), Duration::from_secs(2));
        assert_eq!(shown(&rig.app), None);
        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(4));
        let mut theirs = linked(&ends);
        serve(&mut theirs, 1, worked(report(&[("web-shop", Some(SHOP)), ("blog", None)])));
        seeing(&mut rig, &["web-shop", "blog"]);

        assert_eq!(shown(&rig.app), None);
    }
}

mod requested {
    use serde_json::{Value, json};

    use super::*;

    fn focused(app: &mut App, machine: &str, item: &Value) -> std::result::Result<Value, Value> {
        let args = json!({"machine": machine, "item": item});
        let text = json!({"contract": 1, "command": "focus-machine", "args": args}).to_string();
        app.request(7, &text, Some(AREA), Instant::now());
        let answers = app.take_answers();
        assert_eq!(answers.len(), 1, "an answer at once");
        let reply: Value = serde_json::from_str(&answers[0].1).expect("a reply");
        if reply["error"].is_null() { Ok(reply["ok"].clone()) } else { Err(reply["error"].clone()) }
    }

    #[test]
    fn picks_it_as_a_click_does_and_answers_what_is_shown() {
        let (mut rig, _ends, _theirs) = seen_by();

        rig.app.screen.nav = Some(ui::Nav::Workspaces);

        let answer = focused(&mut rig.app, " VPS ", &json!({"tab": 21})).expect("shown");

        let expected = ShownRemote {
            machine: "vps".into(),
            boot: BOOT.into(),
            project: 2,
            workspace: 20,
            tab: Some(21),
            path: "/home/ana/blog".into(),
        };
        assert_eq!(serde_json::from_value::<ShownRemote>(answer).expect("a shown remote"), expected);
        assert_eq!((shown(&rig.app), rig.app.screen.nav), (Some(at(BOOT, 2, 21, "blog")), None));
    }

    #[rstest]
    #[case::an_unknown_machine("pi", json!({"workspace": 10}), "not-found", "no machine is called `pi`")]
    #[case::a_workspace_it_does_not_have("vps", json!({"workspace": 99}), "not-found", "`vps` has no workspace 99")]
    #[case::a_tab_it_does_not_have("vps", json!({"tab": 99}), "not-found", "`vps` has no tab 99")]
    #[case::a_pane("vps", json!({"pane": 12}), "malformed", "`pane-12` is not a machine's workspace or tab")]
    fn refuses_what_the_machine_does_not_show(
        #[case] machine: &str,
        #[case] item: Value,
        #[case] code: &str,
        #[case] words: &str,
    ) {
        let (mut rig, _ends, _theirs) = seen_by();
        rig.app.select_remote("vps", RemoteItem::Workspace(10));

        let error = focused(&mut rig.app, machine, &item).expect_err("refused");

        assert_eq!(error["code"], code);
        assert!(error["message"].as_str().is_some_and(|message| message.contains(words)), "{error}");
        assert_eq!(shown(&rig.app), Some(at(BOOT, 1, 13, "web-shop")));
    }

    #[test]
    fn refuses_a_machine_that_has_sent_nothing_yet() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);

        let error = focused(&mut rig.app, "vps", &json!({"workspace": 10})).expect_err("refused");

        assert_eq!(error["code"], "unavailable");
        assert_eq!(rig.app.shown_remote(), None);
    }
}
