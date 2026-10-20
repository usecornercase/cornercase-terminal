use std::collections::BTreeMap;

use rstest::rstest;

use super::*;
use crate::accounts;
use crate::agents::Known;
use crate::settings::{Page, Row};

const COMPACT: Rect = Rect { x: 0, y: 0, width: 60, height: 30 };

struct Setup {
    app: App,
    _rx: Receiver<AppEvent>,
    dir: TempDir,
    _folder: TempDir,
}

fn setup() -> Setup {
    let (dir, folder) = (TempDir::new(), TempDir::new());
    let (mut app, rx) = app_in(folder.path(), dir.path().join("config.json"));
    let root = dir.path().join("accounts");
    app.set_accounts_dir(&root);
    for name in ["home", "work"] {
        accounts::add(&root, Known::Claude, name, None).expect("add the account");
    }
    app.relist_accounts();
    app.config.active_accounts.insert("claude".into(), "work".into());
    Setup { app, _rx: rx, dir, _folder: folder }
}

impl Setup {
    fn pill(&self, area: Rect) -> Rect {
        let areas = self.app.layout(area).shown(self.app.nav);
        ui::pill_rects(&areas, &self.app.account_pills()).first().map(|(_, r, _)| *r).expect("the pill shows")
    }

    fn open(&mut self) {
        let at = self.pill(AREA).as_position();
        click(&mut self.app, at);
    }
}

#[test]
fn the_pill_names_the_chosen_account_and_opens_its_accounts() {
    let mut s = setup();
    let r = s.pill(AREA);
    let t = render(&mut s.app, AREA);
    let drawn: String = (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect();
    assert_eq!(drawn, " claude: work ▾ ");
    assert_eq!(r.y, AREA.bottom() - 1);

    s.open();

    assert_eq!(menu_labels(&s.app), ["  System", "  home", "✓ work", "manage accounts…"]);
    let Some(Overlay::Menu { at, .. }) = &s.app.overlay else { panic!("the menu is not open") };
    assert_eq!(ui::menu_area(AREA, *at, &menu_labels(&s.app)).bottom(), r.y, "the menu sits on the bar");
}

#[test]
fn picking_an_account_uses_it_for_new_tabs_only() {
    let mut s = setup();
    s.open();
    pick(&mut s.app, "  home");

    assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("home"));
    let state = s.app.app_state();
    assert_eq!((state.profile, state.accounts.get("claude").map(String::as_str)), (None, Some("home")));
    assert_eq!(toast(&s.app), Some("new claude tabs use home · open tabs keep theirs"));
    assert_eq!(s.app.term().map(|t| t.accounts().name(Known::Claude)), Some(None));
    s.app.add_tab(0, 0, AREA).expect("a new tab");
    assert_eq!(s.app.term().and_then(|t| t.accounts().name(Known::Claude)), Some("home"));
}

#[test]
fn picking_system_goes_back_to_the_agent_s_own_login() {
    let mut s = setup();
    s.open();
    pick(&mut s.app, "  System");
    assert_eq!(s.app.config.active_accounts.get("claude"), None);
    assert_eq!(toast(&s.app), Some("new claude tabs use System · open tabs keep theirs"));
}

#[test]
fn a_pill_shows_the_active_profile_s_account_and_picking_turns_the_profile_off() {
    let mut s = setup();
    let chosen = BTreeMap::from([("claude".to_string(), "home".to_string())]);
    s.app.config.profiles.insert("Home".into(), chosen);
    s.app.config.profile = "Home".into();
    assert_eq!(s.app.account_pills()[0].account, "home");

    s.open();
    pick(&mut s.app, "  work");

    assert_eq!((s.app.config.profile.as_str(), s.app.config.profiles.len()), ("", 1));
    assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("work"));
}

#[test]
fn accounts_are_listed_by_their_names() {
    let mut s = setup();
    let names = BTreeMap::from([(String::new(), "Office".to_string()), ("home".into(), "Personal".into())]);
    s.app.config.account_names.insert("claude".into(), names);
    s.open();
    assert_eq!(menu_labels(&s.app), ["  Office", "  Personal", "✓ work", "manage accounts…"]);

    pick(&mut s.app, "  Personal");

    assert_eq!(s.app.account_pills()[0].account, "Personal");
    assert_eq!(toast(&s.app), Some("new claude tabs use Personal · open tabs keep theirs"));
}

#[test]
fn a_chosen_account_that_is_gone_shows_the_system_login() {
    let mut s = setup();
    s.app.config.active_accounts.insert("claude".into(), "gone".into());
    assert_eq!(s.app.account_pills()[0].account, "System");
}

#[test]
fn manage_accounts_opens_the_accounts_page() {
    let mut s = setup();
    s.open();
    pick(&mut s.app, "manage accounts…");
    let Some(Overlay::Settings(settings)) = &s.app.overlay else { panic!("the settings are not open") };
    assert_eq!(
        (settings.page, settings.rows().get(settings.cursor)),
        (Page::Accounts, Some(&Row::System(Known::Claude)))
    );
}

#[test]
fn a_menu_opened_before_the_settings_changed_does_nothing() {
    let mut s = setup();
    s.open();
    s.app.config_file.revision += 1;
    pick(&mut s.app, "  home");
    assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("work"));
}

#[test]
fn a_menu_opened_before_the_accounts_changed_does_nothing() {
    let mut s = setup();
    s.open();
    let home = menu_labels(&s.app).iter().position(|l| l == "  home").expect("home is listed");
    let at = menu_item_at(&s.app, home);
    accounts::add(&s.dir.path().join("accounts"), Known::Claude, "aaa", None).expect("add the account");
    s.app.relist_accounts();

    click(&mut s.app, at);

    assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("work"));
}

#[test]
fn the_pills_hide_without_accounts_or_when_turned_off() {
    let mut s = setup();
    assert_eq!(s.app.account_pills().len(), 1);
    s.app.config.account_switcher = false;
    assert_eq!(s.app.account_pills(), Vec::<ui::AccountPill>::new());
    assert!(s.app.layout(AREA).accounts.is_empty());

    s.app.config.account_switcher = true;
    for name in ["home", "work"] {
        accounts::remove(&s.dir.path().join("accounts"), Known::Claude, name).expect("remove the account");
    }
    s.app.relist_accounts();
    assert_eq!(s.app.account_pills(), Vec::<ui::AccountPill>::new());
}

#[test]
fn the_terminals_are_two_rows_shorter_while_the_bar_shows() {
    let mut s = setup();
    s.app.resize(AREA);
    let rows = |app: &App| app.term().and_then(|t| t.emulator.size().ok()).map(|(rows, _)| rows);
    let pane = s.app.layout(AREA).pane;
    assert_eq!(pane.bottom(), AREA.bottom() - 2);
    assert_eq!(rows(&s.app), Some(pane.height));

    s.app.config.account_switcher = false;
    s.app.resize(AREA);
    assert_eq!(rows(&s.app), Some(pane.height + 2));
}

#[test]
fn the_bar_shows_and_opens_in_compact_mode_too() {
    let mut s = setup();
    for nav in [None, Some(ui::Nav::Projects)] {
        s.app.nav = nav;
        s.app.overlay = None;
        let at = s.pill(COMPACT).as_position();
        assert_eq!(at.y, COMPACT.bottom() - 1);
        click_in(&mut s.app, at, COMPACT);
        let Some(Overlay::Menu { actions, .. }) = &s.app.overlay else { panic!("the menu is not open") };
        assert_eq!(s.app.menu_labels(actions), ["  System", "  home", "✓ work", "manage accounts…"]);
    }
}

fn screen(app: &mut App, area: Rect) -> String {
    format!("{}", render(app, area).backend())
}

#[rstest]
#[case::side_by_side(config::Sidebar::SideBySide, false, AREA)]
#[case::projects_on_top(config::Sidebar::ProjectsOnTop, false, AREA)]
#[case::workspaces_on_top(config::Sidebar::WorkspacesOnTop, false, AREA)]
#[case::tree(config::Sidebar::Tree, false, AREA)]
#[case::icon_bar(config::Sidebar::SideBySide, true, AREA)]
#[case::compact(config::Sidebar::SideBySide, false, COMPACT)]
fn without_accounts_the_window_is_drawn_as_before(
    #[case] sidebar: config::Sidebar,
    #[case] icons: bool,
    #[case] area: Rect,
) {
    let (dir, folder) = (TempDir::new(), TempDir::new());
    let (mut app, _rx) = app_in(folder.path(), dir.path().join("config.json"));
    app.set_accounts_dir(&dir.path().join("accounts"));
    app.relist_accounts();
    app.config.sidebar = sidebar.id().into();
    app.config.icon_bar = icons;
    app.tab_mut().expect("a tab").name = Some("shell".into());
    app.nav = (area.width < ui::COMPACT_WIDTH).then_some(ui::Nav::Projects);
    assert!(app.config.account_switcher);

    let (panel, agents) = (app.panel_shown(), app.config.agents_section);
    assert!(!app.status_bar());
    assert_eq!(
        app.layout(area),
        ui::full_layout(area, app.widths, ui::Screen { changes: panel, sidebar, agents, icons })
    );
    assert!(app.layout(area).accounts.is_empty());
    let with_switcher = screen(&mut app, area);
    app.config.account_switcher = false;
    assert_eq!(with_switcher, screen(&mut app, area));
}
