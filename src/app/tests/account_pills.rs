use std::collections::BTreeMap;

use rstest::rstest;

use super::*;
use crate::accounts;
use crate::agents::Known;
use crate::app::account_menu::AccountItem;
use crate::settings::{Page, Row};

const COMPACT: Rect = Rect { x: 0, y: 0, width: 60, height: 30 };

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    dir: TempDir,
    _folder: TempDir,
}

fn setup() -> Setup {
    let (dir, folder) = (TempDir::new(), TempDir::new());
    let (mut app, rx) = new_app(dir.path().join("config.json"));
    let root = dir.path().join("accounts");
    app.set_accounts_dir(&root);
    app.set_default_homes(&dir.path().join("homes"));
    for name in ["home", "work"] {
        accounts::add(&root, Known::Claude, name, None).expect("add the account");
    }
    app.relist_accounts();
    app.config.active_accounts.insert("claude".into(), "work".into());
    app.open_project(folder.path().to_path_buf(), AREA).expect("open the project");
    wait_until("the shell is at its prompt", || app.term().is_some_and(Term::shell_in_foreground));
    Setup { app, rx, dir, _folder: folder }
}

impl Setup {
    fn pane(&self) -> u64 {
        self.app.term().expect("a pane").id
    }

    fn open(&mut self) {
        let pane = self.pane();
        click_pill(&mut self.app, pane, 0);
    }

    fn generation(&self, pane: u64) -> u32 {
        self.app.projects.pane_by(pane).map_or(u32::MAX, Term::generation)
    }

    fn claude_on(&self, pane: u64) -> Option<&str> {
        self.app.projects.pane_by(pane).and_then(|term| term.accounts().name(Known::Claude))
    }

    fn add(&self, kind: Known, name: &str) {
        accounts::add(&self.dir.path().join("accounts"), kind, name, None).expect("add the account");
    }

    fn split_right(&mut self) -> u64 {
        let pane = self.app.layout(AREA).pane;
        right_click(&mut self.app, Position::new(pane.x + 2, pane.y + 2));
        pick(&mut self.app, "split right");
        self.app.resize(AREA);
        wait_until("the new shell is at its prompt", || self.app.term().is_some_and(Term::shell_in_foreground));
        self.pane()
    }

    fn open_pane_menu(&mut self) {
        let pane = self.app.layout(AREA).pane;
        right_click(&mut self.app, Position::new(pane.x + 2, pane.y + 2));
    }
}

#[test]
fn the_row_s_button_lists_the_accounts_with_the_pane_s_own_checked() {
    let mut s = setup();
    s.app.config.active_accounts.insert("claude".into(), "home".into());

    s.open();

    assert_eq!(menu_labels(&s.app), ["  System", "  home", "✓ work", "manage accounts…"]);
    assert_eq!(menu_disabled(&s.app), [false; 4]);
}

#[test]
fn picking_another_account_starts_the_shell_again_on_it_and_keeps_the_default() {
    let mut s = setup();
    let (pane, old) = (s.pane(), s.app.term().and_then(Term::shell_pid));
    s.open();

    pick(&mut s.app, "  home");

    assert_eq!((s.generation(pane), s.claude_on(pane)), (1, Some("home")));
    assert_ne!(s.app.projects.pane_by(pane).and_then(Term::shell_pid), old);
    assert_eq!(toast(&s.app), Some("this pane now uses home for claude"));
    assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("work"));
}

#[test]
fn picking_the_account_the_pane_uses_leaves_it_as_it_is() {
    let mut s = setup();
    let pane = s.pane();
    s.open();

    pick(&mut s.app, "✓ work");

    assert_eq!((s.generation(pane), s.claude_on(pane)), (0, Some("work")));
    assert_eq!(toast(&s.app), None);
}

#[test]
fn a_click_on_an_inactive_pane_s_row_acts_on_that_pane_without_focusing_it() {
    let mut s = setup();
    let first = s.pane();
    let second = s.split_right();

    click_pill(&mut s.app, first, 0);
    pick(&mut s.app, "  home");

    assert_eq!((s.generation(first), s.claude_on(first)), (1, Some("home")));
    assert_eq!((s.generation(second), s.claude_on(second)), (0, Some("work")));
    assert_eq!(s.pane(), second);
}

#[test]
fn behind_a_program_the_accounts_are_disabled_and_say_why() {
    let mut s = setup();
    let pane = s.pane();
    type_line(&mut s.app, "sleep 30");
    pump_until(&mut s.app, &s.rx, "sleep runs", |app| app.term().is_some_and(|t| !t.shell_in_foreground()));
    let old = s.app.term().and_then(Term::shell_pid);
    s.open();

    assert_eq!(menu_labels(&s.app), ["  System", "  home", "✓ work", "stop sleep first", "manage accounts…"]);
    assert_eq!(menu_disabled(&s.app), [true, true, true, true, false]);
    pick(&mut s.app, "  home");

    assert_eq!((s.generation(pane), s.claude_on(pane)), (0, Some("work")));
    assert_eq!(s.app.term().and_then(Term::shell_pid), old);
}

#[derive(Debug, Clone, Copy)]
enum Change {
    Settings,
    Accounts,
    Pane,
}

#[rstest]
#[case::settings(Change::Settings)]
#[case::accounts(Change::Accounts)]
#[case::pane(Change::Pane)]
fn a_menu_opened_before_a_change_does_nothing(#[case] change: Change) {
    let mut s = setup();
    let pane = s.pane();
    s.open();
    let home = menu_labels(&s.app).iter().position(|l| l == "  System").expect("System is listed");
    let at = menu_item_at(&s.app, home);
    match change {
        Change::Settings => s.app.settings_changed(),
        Change::Accounts => {
            s.add(Known::Claude, "aaa");
            s.app.relist_accounts();
        }
        Change::Pane => {
            s.app.respawn_shell(pane, Known::Claude, "home").expect("the shell starts again");
            wait_until("the shell is back", || s.app.term().is_some_and(Term::shell_in_foreground));
        }
    }
    let before = (s.generation(pane), s.claude_on(pane).map(str::to_string));

    click(&mut s.app, at);

    assert_eq!((s.generation(pane), s.claude_on(pane).map(str::to_string)), before);
}

#[test]
fn the_pane_and_tab_menus_offer_the_accounts_also_with_the_switcher_off() {
    let mut s = setup();
    s.app.config.account_switcher = false;
    let pane = s.pane();
    let tab = s.app.tab_target(tab_at(&s.app, 0, 0, 0));
    assert!(s.app.target_menu(tab).contains(&MenuAction::AccountMenu(pane)));

    s.open_pane_menu();
    pick(&mut s.app, "account ›");
    assert_eq!(menu_labels(&s.app), ["  System", "  home", "✓ work", "manage accounts…"]);
    pick(&mut s.app, "  home");

    assert_eq!((s.generation(pane), s.claude_on(pane)), (1, Some("home")));
}

#[test]
fn without_accounts_the_menus_have_no_account_entry() {
    let mut s = setup();
    for name in ["home", "work"] {
        accounts::remove(&s.dir.path().join("accounts"), Known::Claude, name).expect("remove the account");
    }
    s.app.relist_accounts();
    let tab = s.app.tab_target(tab_at(&s.app, 0, 0, 0));
    assert!(!s.app.target_menu(tab).iter().any(|action| matches!(action, MenuAction::AccountMenu(_))));

    s.open_pane_menu();

    assert!(!menu_labels(&s.app).contains(&"account ›".to_string()));
}

#[test]
fn a_shell_with_two_agents_asks_which_agent_first() {
    let mut s = setup();
    s.add(Known::Codex, "work");
    s.app.relist_accounts();
    s.open_pane_menu();
    pick(&mut s.app, "account ›");

    assert_eq!(menu_labels(&s.app), ["claude: work ›", "codex: System ›"]);
    pick(&mut s.app, "codex: System ›");

    assert_eq!(menu_labels(&s.app), ["✓ System", "  work", "manage accounts…"]);
    let Some(Overlay::Menu { actions, .. }) = &s.app.overlay else { panic!("the menu is not open") };
    assert!(matches!(actions[0], MenuAction::Account(AccountItem { kind: Known::Codex, .. }, _)));
}

#[test]
fn accounts_are_listed_by_their_names() {
    let mut s = setup();
    let names = BTreeMap::from([(String::new(), "Office".to_string()), ("home".into(), "Personal".into())]);
    s.app.config.account_names.insert("claude".into(), names);
    s.open();
    assert_eq!(menu_labels(&s.app), ["  Office", "  Personal", "✓ work", "manage accounts…"]);

    pick(&mut s.app, "  Personal");

    assert_eq!(toast(&s.app), Some("this pane now uses Personal for claude"));
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
fn the_row_shows_only_with_accounts_and_the_switcher_on() {
    let mut s = setup();
    assert!(s.app.pane_footer());
    s.app.config.account_switcher = false;
    assert!(!s.app.pane_footer());

    s.app.config.account_switcher = true;
    for name in ["home", "work"] {
        accounts::remove(&s.dir.path().join("accounts"), Known::Claude, name).expect("remove the account");
    }
    s.app.relist_accounts();
    assert!(!s.app.pane_footer());
}

#[rstest]
#[case::wide(AREA)]
#[case::compact(COMPACT)]
fn the_window_has_no_bar_of_accounts(#[case] area: Rect) {
    let s = setup();
    let (panel, sidebar) = (s.app.panel_shown(), s.app.sidebar());
    let (agents, icons) = (s.app.config.agents_section, s.app.config.icon_bar);
    let whole = ui::full_layout(
        area,
        s.app.screen.widths,
        ui::Screen { changes: panel, sidebar, agents, icon_bar: icons, ..ui::Screen::default() },
    );
    assert_eq!(s.app.layout(area), whole);
    assert_eq!(s.app.layout(area).pane.bottom(), area.bottom());
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
    app.screen.nav = (area.width < ui::COMPACT_WIDTH).then_some(ui::Nav::Projects);
    assert!(app.config.account_switcher);

    let (panel, agents) = (app.panel_shown(), app.config.agents_section);
    assert_eq!(
        app.layout(area),
        ui::full_layout(
            area,
            app.screen.widths,
            ui::Screen { changes: panel, sidebar, agents, icon_bar: icons, ..ui::Screen::default() }
        )
    );
    let with_switcher = screen(&mut app, area);
    app.config.account_switcher = false;
    assert_eq!(with_switcher, screen(&mut app, area));
}
