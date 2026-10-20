use std::collections::BTreeMap;

use super::issue_list::{Setup, browser, enter, listing, open_loaded, setup, shortcut_with_token};
use super::*;
use crate::accounts;
use crate::agents::Known;

fn with_accounts(s: &mut Setup) {
    let dir = s.config.path().join("accounts");
    for name in ["home", "work"] {
        accounts::add(&dir, Known::Claude, name, None).expect("add the account");
    }
    s.app.set_accounts_dir(&dir);
    let fake = s.app.config.agent_commands.get("fake").cloned().expect("the fake agent");
    s.app.config.agent = "claude".into();
    s.app.config.agent_commands.insert("claude".into(), fake);
    s.app.config.active_accounts = claude("work");
    s.app.relist_accounts();
}

fn press_button(s: &mut Setup, label: &str) {
    let labels = browser(&s.app).labels(issues_dialog::room(AREA).buttons);
    let i = labels.iter().position(|l| *l == label).expect("the button is shown");
    click(&mut s.app, ui::issue_buttons(ui::issues_area(AREA), &labels)[i].as_position());
}

fn pick_home(s: &mut Setup) {
    press_button(s, "account…");
    "home".chars().for_each(|c| send_key(&mut s.app, KeyCode::Char(c), KeyModifiers::NONE));
    enter(s);
}

fn chosen(term: &Term) -> BTreeMap<String, String> {
    term.accounts().chosen()
}

fn started(s: &mut Setup) -> BTreeMap<String, String> {
    enter(s);
    enter(s);
    pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);
    chosen(s.app.projects[0].workspaces[1].tab().and_then(Tab::pane).expect("the new pane"))
}

fn claude(account: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("claude".to_string(), account.to_string())])
}

fn reading(s: &mut Setup) {
    with_accounts(s);
    open_loaded(s, Source::Github);
    enter(s);
}

#[test]
fn an_issue_starts_with_the_account_picked_for_it_and_new_tabs_keep_the_one_in_use() {
    let mut s = listing();
    reading(&mut s);
    pick_home(&mut s);
    assert_eq!((started(&mut s), s.app.config.active_accounts.clone()), (claude("home"), claude("work")));
}

#[test]
fn starting_asks_for_the_account_with_the_one_in_use_selected() {
    let mut s = listing();
    reading(&mut s);
    enter(&mut s);
    assert!(browser(&s.app).picker.is_some(), "the account list opens");
    assert_eq!(s.app.projects[0].workspaces.len(), 1);
    assert_eq!(started(&mut s), claude("work"));
}

#[test]
fn a_story_in_a_new_tab_takes_the_picked_account() {
    let mut s = setup(false, "echo '[]'");
    with_accounts(&mut s);
    let _server = shortcut_with_token(&mut s);
    enter(&mut s);
    pick_home(&mut s);
    enter(&mut s);
    let workspace = &s.app.projects[0].workspaces[0];
    let term = workspace.tabs.get(1).and_then(Tab::pane).expect("the new tab");
    assert_eq!(chosen(term), claude("home"));
}

#[test]
fn the_pick_shows_the_names_given_to_the_accounts() {
    let mut s = listing();
    let labels = BTreeMap::from([(String::new(), "Main".to_string()), ("work".into(), "Job".into())]);
    s.app.config.account_names = BTreeMap::from([("claude".to_string(), labels)]);
    reading(&mut s);
    press_button(&mut s, "account…");
    let keys: Vec<String> = browser(&s.app).choices().into_iter().map(|c| c.key).collect();
    assert_eq!(keys, ["Main", "home", "Job"]);
}

#[test]
fn an_issue_scheduled_from_the_browser_keeps_the_account_picked_there() {
    let mut s = listing();
    reading(&mut s);
    pick_home(&mut s);
    press_button(&mut s, "schedule");
    let Some(Overlay::Queue(form)) = &mut s.app.overlay else { panic!("the schedule form is not open") };
    assert_eq!(form.chosen_account(), Some(("claude", "home")));
    form.prompt = todo::editor::Editor::new("fix the login");
    s.app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);

    let entry = s.app.panels.queued.queue.entries.last().expect("the entry");
    assert_eq!(entry.task.accounts, claude("home"));
}

fn flow_from_the_issue(s: &mut Setup) -> (String, Option<String>) {
    press_button(s, "start a flow");
    let Some(Overlay::StartFlow { start, .. }) = &s.app.overlay else { panic!("the flow dialog is not open") };
    let combo = start.accounts.iter().find(|agent| agent.kind == "claude").expect("claude's combo");
    (combo.chosen().to_string(), start.notice.clone())
}

#[test]
fn a_flow_from_an_issue_takes_the_account_picked_for_it() {
    let mut s = listing();
    reading(&mut s);
    pick_home(&mut s);
    assert_eq!(flow_from_the_issue(&mut s), ("home".into(), None));
}

#[test]
fn a_flow_from_an_issue_without_a_pick_starts_on_the_account_in_use() {
    let mut s = listing();
    reading(&mut s);
    assert_eq!(flow_from_the_issue(&mut s), ("work".into(), None));
}

#[test]
fn the_pick_stays_away_while_it_is_turned_off() {
    let mut s = listing();
    with_accounts(&mut s);
    s.app.config.account_picker = false;
    open_loaded(&mut s, Source::Github);
    enter(&mut s);
    assert!(browser(&s.app).accounts.is_none());
}

#[test]
fn the_pick_stays_away_without_accounts() {
    let mut s = listing();
    open_loaded(&mut s, Source::Github);
    enter(&mut s);
    assert!(browser(&s.app).accounts.is_none());
}

#[test]
fn the_pick_stays_away_for_an_agent_without_accounts() {
    let mut s = listing();
    reading(&mut s);
    assert!(browser(&s.app).labels(issues_dialog::room(AREA).buttons).contains(&"account…"));
    let Some(Overlay::Issues(b)) = &mut s.app.overlay else { panic!("issues") };
    b.agents.chosen = Some("fake".into());
    assert!(!b.labels(issues_dialog::room(AREA).buttons).contains(&"account…"));
}

const GONE: &str = "there is no claude account `home`; `cornercase account list` lists them";

fn home_picked_then_deleted() -> Setup {
    let mut s = listing();
    reading(&mut s);
    pick_home(&mut s);
    let dir = s.config.path().join("accounts");
    std::fs::remove_dir_all(accounts::path(&dir, Known::Claude, "home")).expect("delete the account");
    s.app.relist_accounts();
    s
}

#[test]
fn an_issue_whose_picked_account_was_deleted_says_so_and_starts_nothing() {
    let mut s = home_picked_then_deleted();

    enter(&mut s);
    enter(&mut s);

    let b = browser(&s.app);
    assert_eq!((b.error.as_deref(), b.starting), (Some(GONE), false));
    assert_eq!(s.app.projects[0].workspaces.len(), 1);
}

#[test]
fn a_flow_from_an_issue_whose_picked_account_was_deleted_says_so() {
    let mut s = home_picked_then_deleted();

    assert_eq!(flow_from_the_issue(&mut s), ("work".into(), Some(GONE.into())));
}
