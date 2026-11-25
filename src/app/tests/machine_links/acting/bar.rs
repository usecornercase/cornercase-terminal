use super::menus::{Place, opened};
use super::*;
use crate::app::machine_links::RemoteItem;
use crate::config;
use crate::ui::tab_bar::Strip;

const FIRST: u64 = 100;

fn on_top(remote: Remote) -> Remote {
    let mut remote = remote.shown_in(Place::Column);
    remote.rig.app.config.tabs = config::Tabs::Top.id().into();
    render(&mut remote.rig.app, AREA);
    remote
}

fn strip(app: &App) -> Strip {
    app.tab_strip(&app.layout(AREA))
}

fn shown_tab(app: &App) -> Option<u64> {
    app.shown_remote().and_then(|shown| shown.tab)
}

fn twelve() -> Remote {
    let tabs = (0..12).map(|i| TabInfo { id: FIRST + i, name: format!("tab {i}"), ..TabInfo::default() }).collect();
    on_top(seeing_in(Reply::Agree, holding(BOOT, WorkspaceInfo { tabs, ..workspace() })))
}

fn with_models() -> Remote {
    let mut workspace = tabbed();
    workspace.tabs[1].panes[0].model = Some("Opus 5.5".into());
    workspace.tabs[1].panes[0].context = Some(40);
    on_top(seeing_in(Reply::Agree, holding(BOOT, workspace)))
}

fn bar_row(app: &mut App) -> String {
    let r = app.layout(AREA).tab_bar;
    let t = render(app, AREA);
    (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

mod the_bar {
    use super::*;

    #[test]
    fn lists_the_shown_machine_workspace_s_tabs() {
        let mut remote = on_top(with_tabs(Reply::Agree));
        let app = &mut remote.rig.app;

        let bar = app.tab_bar_view();
        let names: Vec<&str> = bar.tabs.iter().map(|tab| tab.name.as_str()).collect();
        assert_eq!((names, bar.active), (vec!["zsh", "review"], Some(0)));
        assert_eq!(bar.machine.map(|machine| machine.name), Some("vps".into()));

        app.leave_remote();
        let bar = app.tab_bar_view();
        assert_eq!((bar.tabs.len(), bar.machine), (0, None));
    }

    #[test]
    fn is_drawn_without_a_project_here() {
        let mut remote = on_top(with_tabs(Reply::Agree));

        let row = bar_row(&mut remote.rig.app);

        assert!(row.contains("zsh") && row.contains("review"), "{row:?}");
    }

    #[test]
    fn a_click_on_a_tab_shows_it_there() {
        let mut remote = on_top(with_tabs(Reply::Agree));
        let pos = strip(&remote.rig.app).item(1).as_position();

        click(&mut remote.rig.app, pos);

        assert_eq!(shown_tab(&remote.rig.app), Some(AGENT));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn brings_the_shown_tab_into_view_and_starts_over_when_it_is_ours_again() {
        let mut remote = twelve();
        let link = remote.link;
        let app = &mut remote.rig.app;
        assert!(strip(app).item(11).is_empty());

        assert!(app.select_on(link, RemoteItem::Tab(FIRST + 11)));
        render(app, AREA);
        assert!(!strip(app).item(11).is_empty());
        assert!(app.scroll.of(List::TabBar) > 0);

        app.leave_remote();
        render(app, AREA);
        assert_eq!(app.scroll.of(List::TabBar), 0);
    }

    #[test]
    fn the_wheel_scrolls_the_machine_s_tabs() {
        let mut remote = twelve();
        let at = remote.rig.app.layout(AREA).tab_bar.as_position();

        mouse(&mut remote.rig.app, MouseEventKind::ScrollDown, at);

        assert_eq!(remote.rig.app.scroll.of(List::TabBar), 1);
    }

    #[test]
    fn shows_the_details_of_the_pane_in_use() {
        let mut remote = with_models();
        let link = remote.link;
        let app = &mut remote.rig.app;
        (app.config.model, app.config.context) = (true, true);
        assert_eq!(app.tab_bar_view().details, ui::Details::default());

        assert!(app.select_on(link, RemoteItem::Tab(AGENT)));

        let details = app.tab_bar_view().details;
        assert_eq!((details.model.as_deref(), details.percent), (Some("Opus 5.5"), Some(40)));
    }
}

mod its_plus {
    use super::*;

    fn press_plus(remote: &mut Remote) {
        let pos = strip(&remote.rig.app).new_button().as_position();
        click(&mut remote.rig.app, pos);
    }

    #[test]
    fn opens_a_tab_there() {
        let mut remote = on_top(with_tabs(Reply::Made(made_tab)));

        press_plus(&mut remote);
        remote.heard();

        let asked = remote.asked();
        let [asked] = &asked[..] else { panic!("one question, not {asked:?}") };
        assert_eq!((asked["command"].as_str(), asked["boot"].as_str()), (Some("new-tab"), Some(BOOT)));
        assert_eq!(asked["args"]["workspace"], WORKSPACE);
    }

    #[test]
    fn picks_the_new_tab_once_the_machine_lists_it() {
        let mut remote = on_top(with_tabs(Reply::Made(made_tab)));

        press_plus(&mut remote);
        remote.heard();
        serve(&mut remote.theirs, 2, listing_the_made_tab());
        remote.rig.until("the new tab shows", |app| shown_tab(app) == Some(MADE));
        render(&mut remote.rig.app, AREA);

        assert_eq!(remote.rig.app.tab_bar_view().active, Some(2));
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_sends_nothing() {
        let mut remote = on_top(with_tabs(Reply::Agree));
        remote.falls();

        press_plus(&mut remote);

        remote.sent_nothing_while_down();
    }

    #[test]
    fn opens_no_menu_on_a_right_click() {
        let mut remote = on_top(with_tabs(Reply::Agree));
        let pos = strip(&remote.rig.app).new_button().as_position();

        right_click(&mut remote.rig.app, pos);

        assert_eq!((remote.rig.app.overlay.is_none(), remote.rig.seen("ask")), (true, 0));
    }
}

mod a_tab_s_close_button {
    use super::*;

    fn close(remote: &mut Remote, t: usize) {
        let pos = strip(&remote.rig.app).close(t).as_position();
        click(&mut remote.rig.app, pos);
    }

    #[test]
    fn closes_a_shell_at_once_and_reads_closing_until_the_machine_drops_it() {
        let mut remote = on_top(with_tabs(Reply::Agree));

        close(&mut remote, 0);
        assert!(remote.rig.app.overlay.is_none());
        assert!(remote.rig.app.tab_bar_view().tabs[0].closing);
        remote.heard();

        let asked = remote.asked();
        assert_eq!(
            (asked[0]["command"].as_str(), &asked[0]["args"]["item"]),
            (Some("close"), &json!({ "tab": SHELL }))
        );
        assert!(remote.rig.app.tab_bar_view().tabs[0].closing);
    }

    #[test]
    fn asks_first_for_a_tab_with_an_agent() {
        let mut remote = on_top(with_tabs(Reply::Agree));

        close(&mut remote, 1);

        assert_eq!(opened(&remote.rig.app), Some("close machine tab"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_sends_nothing() {
        let mut remote = on_top(with_tabs(Reply::Agree));
        remote.falls();

        close(&mut remote, 0);

        remote.sent_nothing_while_down();
    }

    #[test]
    fn and_the_tab_take_no_click_while_it_closes() {
        let mut remote = on_top(with_tabs(Reply::Agree));
        let agent = strip(&remote.rig.app).item(1).as_position();
        click(&mut remote.rig.app, agent);
        close(&mut remote, 0);
        remote.heard();

        let shell = strip(&remote.rig.app).item(0).as_position();
        click(&mut remote.rig.app, shell);
        close(&mut remote, 0);
        right_click(&mut remote.rig.app, shell);

        assert_eq!((remote.rig.app.overlay.is_none(), remote.rig.seen("ask")), (true, 1));
        assert_eq!(shown_tab(&remote.rig.app), Some(AGENT));
    }
}

mod a_tab_s_menu {
    use super::*;

    fn open_menu(remote: &mut Remote, t: usize, right: bool) {
        let strip = strip(&remote.rig.app);
        if right {
            right_click(&mut remote.rig.app, strip.item(t).as_position());
        } else {
            click(&mut remote.rig.app, strip.menu(t).as_position());
        }
    }

    #[rstest]
    #[case::button(false)]
    #[case::right_click(true)]
    fn offers_to_rename_and_close_it(#[case] right: bool) {
        let mut remote = on_top(with_tabs(Reply::Agree));

        open_menu(&mut remote, 1, right);

        let labels: Vec<String> = vec!["rename tab".into(), "close tab".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![false, false]));
        let pos = menu_item_at(&remote.rig.app, 0);
        click(&mut remote.rig.app, pos);
        assert_eq!(opened(&remote.rig.app), Some("rename machine tab"));
    }

    #[rstest]
    #[case::button(false)]
    #[case::right_click(true)]
    fn of_a_fallen_machine_says_why_and_greys_both(#[case] right: bool) {
        let mut remote = on_top(with_tabs(Reply::Agree));
        remote.falls();

        open_menu(&mut remote, 0, right);

        let labels: Vec<String> = vec!["vps unreachable".into(), "rename tab".into(), "close tab".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![true, true, true]));
    }
}
