use std::collections::BTreeMap;

use super::*;
use crate::accounts;
use crate::agents::Known;
use crate::settings::{Page, Row};
use crate::test_util::write_executable;

const WIDE: Rect = Rect { x: 0, y: 0, width: 140, height: 40 };

const STATUS: &str = r#"#!/bin/sh
case "$1 $2" in
"auth status")
  case "$CLAUDE_CONFIG_DIR" in
  */work) echo '{"loggedIn": true, "email": "me@work.dev", "subscriptionType": "max"}' ;;
  *) echo '{"loggedIn": false}'; exit 1 ;;
  esac ;;
*) printf '%s|%s\n' "$*" "$CLAUDE_CONFIG_DIR" > OUT.tmp && mv OUT.tmp OUT ;;
esac
"#;

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    bin: TempDir,
    _folder: TempDir,
}

fn setup() -> Setup {
    with_project(true)
}

fn with_project(project: bool) -> Setup {
    let (config, folder, bin) = (TempDir::new(), TempDir::new(), TempDir::new());
    let (mut app, rx) = if project {
        app_in(folder.path(), config.path().join("config.json"))
    } else {
        new_app(config.path().join("config.json"))
    };
    app.set_accounts_dir(&config.path().join("accounts"));
    app.paths.home = Some(config.path().to_path_buf());
    let fake = bin.path().join("claude");
    write_executable(&fake, &STATUS.replace("OUT", &bin.path().join("out").display().to_string()));
    app.config.agent_commands.insert("claude".into(), fake.display().to_string());
    app.config.agent_commands.insert("codex".into(), "/nonexistent/codex".into());
    Setup { app, rx, config, bin, _folder: folder }
}

impl Setup {
    fn add(&self, kind: Known, name: &str) {
        accounts::add(&self.config.path().join("accounts"), kind, name, None).expect("add the account");
    }

    fn open(&mut self) {
        self.app.relist_accounts();
        self.app.settings_page = Page::Accounts;
        self.app.open_settings();
        self.app.watch_accounts_page(Instant::now());
    }

    fn settle(&mut self) {
        wait_until("the logins are checked", || {
            drain(&mut self.app, &self.rx);
            self.app.checking_logins().is_empty()
        });
    }

    fn go(&mut self, row: &Row) {
        let Some(Overlay::Settings(s)) = &mut self.app.overlay else { panic!("the settings are not open") };
        let i = s.rows().iter().position(|r| r == row).expect("the row is there");
        s.select(i);
    }

    fn key(&mut self, code: KeyCode) {
        send_key(&mut self.app, code, KeyModifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        text.chars().for_each(|c| self.key(KeyCode::Char(c)));
    }

    fn value(&self, row: &Row) -> String {
        let Some(Overlay::Settings(s)) = &self.app.overlay else { panic!("the settings are not open") };
        s.row_text(row).value
    }

    fn rename_system(&mut self, name: &str) {
        self.go(&Row::RenameAccount(Known::Claude, String::new()));
        self.key(KeyCode::Enter);
        self.type_text(name);
        self.key(KeyCode::Enter);
    }

    fn label(&self, row: &Row) -> String {
        let Some(Overlay::Settings(s)) = &self.app.overlay else { panic!("the settings are not open") };
        s.row_text(row).label
    }

    fn notice(&self) -> Option<String> {
        let Some(Overlay::Settings(s)) = &self.app.overlay else { return None };
        s.notice.clone()
    }

    fn profile(&mut self, name: &str, account: &str) {
        let chosen = BTreeMap::from([("claude".to_string(), account.to_string())]);
        self.app.config.profiles.insert(name.to_string(), chosen);
    }
}

#[test]
fn opening_the_page_checks_each_login() {
    let mut s = setup();
    s.add(Known::Claude, "home");
    s.add(Known::Claude, "work");
    s.open();
    assert_eq!(s.value(&Row::Account(Known::Claude, "work".into())), "checking…");
    s.settle();
    assert_eq!(s.value(&Row::Account(Known::Claude, "work".into())), "signed in as me@work.dev (max)");
    assert_eq!(s.value(&Row::Account(Known::Claude, "home".into())), "signed out");
    assert_eq!(s.value(&Row::System(Known::Claude)), "signed out");
}

#[test]
fn the_system_login_is_checked_only_once_its_agent_has_accounts() {
    let mut s = setup();
    s.open();
    assert_eq!(s.app.checking_logins(), Vec::new());
    assert_eq!(s.value(&Row::System(Known::Claude)), "");
}

#[test]
fn the_page_with_two_accounts_and_one_in_use() {
    let mut s = with_project(false);
    s.add(Known::Claude, "home");
    s.add(Known::Claude, "work");
    s.app.config.active_accounts = BTreeMap::from([("claude".to_string(), "work".to_string())]);
    s.app.config.account_names =
        BTreeMap::from([("claude".to_string(), BTreeMap::from([(String::new(), "Office".to_string())]))]);
    s.app.set_default_homes(&s.config.path().join("homes"));
    s.open();
    s.settle();
    insta::assert_snapshot!(format!("{}", render(&mut s.app, WIDE).backend()));
}

#[test]
fn a_new_account_opens_its_login_tab_and_closes_the_settings() {
    let mut s = setup();
    s.open();
    s.go(&Row::AddAccount(Known::Claude));
    s.key(KeyCode::Enter);
    s.type_text("work");
    s.key(KeyCode::Enter);
    assert!(s.app.overlay.is_none());
    assert_eq!(toast(&s.app), Some("log in to claude in the tab work login"));
    let out = s.bin.path().join("out");
    refreshing(&mut s.app, &s.rx, "the login command runs", |_| out.exists());
    let work = s.config.path().join("accounts/claude/work").canonicalize().expect("the account");
    assert_eq!(std::fs::read_to_string(&out).expect("the log").trim_end(), format!("auth login|{}", work.display()));
}

#[test]
fn removing_an_account_asks_first_and_forgets_its_name() {
    let mut s = with_project(false);
    s.add(Known::Claude, "work");
    s.profile("Work", "work");
    s.app.config.account_names =
        BTreeMap::from([("claude".to_string(), BTreeMap::from([("work".to_string(), "Job".to_string())]))]);
    s.open();
    s.go(&Row::Account(Known::Claude, "work".into()));
    s.key(KeyCode::Backspace);
    assert!(matches!(&s.app.overlay, Some(Overlay::RemoveAccount { kind: Known::Claude, name }) if name == "work"));
    insta::assert_snapshot!(format!("{}", render(&mut s.app, AREA).backend()));
    assert!(s.config.path().join("accounts/claude/work").is_dir(), "nothing is removed before the answer");

    s.key(KeyCode::Enter);

    assert!(!s.config.path().join("accounts/claude/work").exists());
    assert_eq!(s.notice().as_deref(), Some("the claude account work was removed"));
    assert_eq!(s.app.config.account_names, BTreeMap::new());
    assert_eq!(s.app.config.profiles["Work"], BTreeMap::new());
}

#[test]
fn cancelling_the_removal_keeps_the_account_and_goes_back() {
    let mut s = setup();
    s.add(Known::Claude, "work");
    s.open();
    s.go(&Row::Account(Known::Claude, "work".into()));
    s.key(KeyCode::Backspace);
    s.key(KeyCode::Esc);
    assert!(matches!(&s.app.overlay, Some(Overlay::Settings(settings)) if settings.page == Page::Accounts));
    assert!(s.config.path().join("accounts/claude/work").is_dir());
}

#[test]
fn using_an_account_saves_it_leaves_the_profile_and_raises_the_revision() {
    let mut s = setup();
    s.add(Known::Claude, "home");
    s.profile("Work", "home");
    s.app.config.profile = "Work".into();
    s.open();
    let before = s.app.config_file.revision;
    s.go(&Row::System(Known::Claude));
    s.key(KeyCode::Enter);
    assert_eq!(s.app.config.profile, "");
    assert_eq!(s.app.config.active_accounts, BTreeMap::new());
    assert!(s.app.config_file.revision > before);
    assert_eq!(s.label(&Row::System(Known::Claude)), "● System");
    assert_eq!(s.notice().as_deref(), Some("new claude tabs use System · open tabs keep theirs"));

    s.go(&Row::Account(Known::Claude, "home".into()));
    s.key(KeyCode::Enter);
    assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("home"));
    assert_eq!(s.label(&Row::Account(Known::Claude, "home".into())), "● home");
}

#[test]
fn the_system_login_is_renamed_and_the_name_cleared() {
    let mut s = setup();
    s.add(Known::Claude, "home");
    s.open();
    s.rename_system("work");
    assert_eq!(s.app.config.account_label("claude", ""), "work");
    assert_eq!(s.label(&Row::System(Known::Claude)), "● work");
    assert_eq!(s.notice().as_deref(), Some("claude: System is now called work"));
    assert!(s.config.path().join("accounts/claude/home").is_dir(), "no folder moves");

    s.go(&Row::RenameAccount(Known::Claude, String::new()));
    s.key(KeyCode::Enter);
    (0..4).for_each(|_| s.key(KeyCode::Backspace));
    s.key(KeyCode::Enter);
    assert_eq!(s.app.config.account_names, BTreeMap::new());
    assert_eq!(s.notice().as_deref(), Some("claude: work is now called System"));
}

#[test]
fn a_name_taken_by_another_account_shows_the_server_s_reason() {
    let mut s = setup();
    s.add(Known::Claude, "home");
    s.open();
    s.rename_system("Home");
    assert_eq!(s.app.config.account_names, BTreeMap::new());
    assert!(s.notice().is_some_and(|notice| notice.contains("home")), "{:?}", s.notice());
}

#[test]
fn an_account_chosen_outside_shows_on_the_page() {
    let mut s = setup();
    s.add(Known::Claude, "work");
    s.open();
    let chosen = BTreeMap::from([("claude".to_string(), "work".to_string())]);
    let config = Config { active_accounts: chosen, ..s.app.config.clone() };
    s.app.set_config(config);
    assert_eq!(s.label(&Row::Account(Known::Claude, "work".into())), "● work");
}
