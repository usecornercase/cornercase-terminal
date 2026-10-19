use std::collections::BTreeMap;

use super::*;
use crate::accounts;
use crate::agents::Known;
use crate::control::QueueAdd;
use crate::queue::form::{Control, Form};
use rstest::rstest;

fn with_profiles(app: &mut App, config: &TempDir) {
    let dir = config.path().join("accounts");
    for name in ["home", "work"] {
        accounts::add(&dir, Known::Claude, name, None).expect("add the account");
    }
    app.set_accounts_dir(&dir);
    let fake = app.config.agent_commands.get("fake").cloned().expect("the fake agent");
    app.config.agent = "claude".into();
    app.config.agent_commands.insert("claude".into(), fake);
    app.config.profiles = BTreeMap::from([("Home".into(), claude("home")), ("Work".into(), claude("work"))]);
    app.config.profile = "Work".into();
    app.relist_accounts();
}

fn claude(account: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("claude".to_string(), account.to_string())])
}

fn chosen(app: &App, chosen: BTreeMap<String, String>) -> Task {
    Task { accounts: chosen, agent: "claude".into(), ..task(app, time::now() + 3600, None) }
}

type Held = (Receiver<AppEvent>, TempDir, TempDir, TempDir);

fn scheduled(accounts: BTreeMap<String, String>) -> (App, u64, Held) {
    let (mut app, rx, repo, config, worktrees) = queued_app();
    with_profiles(&mut app, &config);
    let id = app.queued.queue.add(chosen(&app, accounts)).expect("add");
    (app, id, (rx, repo, config, worktrees))
}

fn form(app: &mut App) -> &mut Form {
    let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("the schedule form") };
    form
}

fn save(app: &mut App) {
    form(app).prompt = todo::editor::Editor::new("fix the login");
    app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
    assert!(!matches!(app.overlay, Some(Overlay::Queue(_))), "the form saved");
}

#[test]
fn an_entry_starts_with_its_accounts_and_keeps_the_selection() {
    let (mut app, id, _held) = scheduled(claude("home"));

    app.run_queued(id, Instant::now()).expect("start");

    assert_eq!(app.agents.launches.len(), 1);
    let pane = app.projects[0].workspaces[0].tabs.last().and_then(Tab::pane).expect("the new tab");
    assert_eq!((pane.accounts().chosen(), app.config.profile.as_str()), (claude("home"), "Work"));
}

#[test]
fn an_entry_without_accounts_starts_with_the_selection_when_it_runs() {
    let (mut app, id, _held) = scheduled(BTreeMap::new());
    app.config.profile = "Home".into();

    app.run_queued(id, Instant::now()).expect("start");

    let pane = app.projects[0].workspaces[0].tabs.last().and_then(Tab::pane).expect("the new tab");
    assert_eq!(pane.accounts().chosen(), claude("home"));
}

#[test]
fn an_account_deleted_before_the_start_fails_the_entry_with_its_reason() {
    let (mut app, id, held) = scheduled(claude("home"));
    let tabs = app.projects[0].workspaces[0].tabs.len();
    accounts::remove(&held.2.path().join("accounts"), Known::Claude, "home").expect("remove");
    app.relist_accounts();

    let reason = app.run_queued(id, Instant::now()).expect_err("no account");

    assert!(reason.contains("there is no claude account `home`"), "{reason}");
    let state = &app.queued.queue.entry(id).expect("entry").state;
    assert!(
        matches!(state, QueueState::Failed { reason } if reason.contains("there is no claude account `home`")),
        "{state:?}"
    );
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), tabs);
}

#[rstest]
#[case::unknown_account("claude", "gone")]
#[case::agent_without_accounts("opencode", "home")]
fn adding_with_an_unknown_account_is_refused(#[case] agent: &str, #[case] account: &str) {
    let (mut app, _rx, _repo, config, _worktrees) = queued_app();
    with_profiles(&mut app, &config);
    let add = QueueAdd {
        at: "in 2h".into(),
        zone: "UTC".into(),
        prompt: Some("fix the login".into()),
        accounts: BTreeMap::from([(agent.to_string(), account.to_string())]),
        ..QueueAdd::default()
    };

    let refused = app.add_queue_request(None, None, None, &add).expect_err("unknown");

    assert_eq!(refused.code, crate::control::contract::Code::NotFound);
    assert_eq!(app.queued.queue.entries, []);
}

#[test]
fn an_edit_keeps_the_accounts_and_changes_only_the_agents_it_names() {
    let (mut app, _rx, _repo, config, _worktrees) = queued_app();
    with_profiles(&mut app, &config);
    accounts::add(&config.path().join("accounts"), Known::Codex, "team", None).expect("add codex");
    app.relist_accounts();
    let add = QueueAdd {
        at: "in 2h".into(),
        zone: "UTC".into(),
        prompt: Some("fix the login".into()),
        accounts: claude("home"),
        ..QueueAdd::default()
    };
    let id =
        app.add_queue_request(None, None, None, &add).expect("add").expect("answered")["id"].as_u64().expect("an id");
    let accounts_of = |app: &App| app.queued.queue.entry(id).expect("entry").task.accounts.clone();

    let edit = QueueAdd { prompt: Some("fix logout".into()), ..QueueAdd::default() };
    app.add_queue_request(None, None, Some(id), &edit).expect("edit");
    assert_eq!(accounts_of(&app), claude("home"));

    let accounts = BTreeMap::from([("claude".to_string(), String::new()), ("codex".into(), "team".into())]);
    app.add_queue_request(None, None, Some(id), &QueueAdd { accounts: accounts.clone(), ..edit }).expect("both");
    assert_eq!(accounts_of(&app), accounts);
}

fn shown(app: &mut App) -> (String, Option<String>) {
    let combo = &form(app).account.as_ref().expect("the account combo").combo;
    (combo.options().join(" "), combo.label().map(str::to_string))
}

fn type_keys(app: &mut App, keys: &str) {
    for c in keys.chars() {
        app.queue_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), AREA);
    }
}

fn pick_agent(app: &mut App, agent: &str) {
    form(app).focus_on(Control::Agent);
    type_keys(app, " ");
    type_keys(app, agent);
    app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
    assert_eq!(form(app).agent.label(), Some(agent));
}

#[test]
fn the_form_offers_the_agents_accounts_with_the_one_in_use_and_saves_the_pick() {
    let (mut app, _rx, _repo, config, _worktrees) = queued_app();
    with_profiles(&mut app, &config);
    app.new_queue_form(None);
    assert_eq!(shown(&mut app), ("System home work".into(), Some("work".into())));
    assert!(form(&mut app).controls().contains(&Control::Account));

    form(&mut app).account.as_mut().expect("combo").combo.choose(1);
    save(&mut app);

    let entry = app.queued.queue.entries.last().expect("the entry");
    assert_eq!(entry.task.accounts, claude("home"));
}

#[test]
fn the_accounts_follow_the_agent_chosen_in_the_form() {
    let (mut app, _rx, _repo, config, _worktrees) = queued_app();
    with_profiles(&mut app, &config);
    accounts::add(&config.path().join("accounts"), Known::Codex, "team", None).expect("add codex");
    app.relist_accounts();
    app.config.account_names =
        BTreeMap::from([("codex".to_string(), BTreeMap::from([(String::new(), "Main".to_string())]))]);
    app.new_queue_form(None);

    pick_agent(&mut app, "codex");
    assert_eq!(shown(&mut app), ("Main team".into(), Some("Main".into())));
    pick_agent(&mut app, "opencode");
    assert!(form(&mut app).account.is_none());
    assert!(!form(&mut app).controls().contains(&Control::Account));
}

#[test]
fn editing_an_entry_shows_its_account_and_saves_a_new_pick() {
    let (mut app, id, _held) = scheduled(claude("home"));

    app.new_queue_form(Some(id));
    assert_eq!(shown(&mut app).1.as_deref(), Some("home"));
    form(&mut app).account.as_mut().expect("combo").combo.choose(0);
    save(&mut app);

    assert_eq!(app.queued.queue.entry(id).expect("entry").task.accounts, claude(""));
}

#[test]
fn editing_an_entry_without_accounts_leaves_it_to_the_selection_when_it_runs() {
    let (mut app, id, _held) = scheduled(BTreeMap::new());

    app.new_queue_form(Some(id));
    assert_eq!(shown(&mut app).1.as_deref(), Some("work"));
    save(&mut app);

    assert_eq!(app.queued.queue.entry(id).expect("entry").task.accounts, BTreeMap::new());
}

#[test]
fn an_account_deleted_while_the_form_is_open_is_refused_on_save() {
    let (mut app, _rx, _repo, config, _worktrees) = queued_app();
    with_profiles(&mut app, &config);
    app.new_queue_form(None);
    form(&mut app).account.as_mut().expect("combo").combo.choose(1);
    accounts::remove(&config.path().join("accounts"), Known::Claude, "home").expect("remove");
    app.relist_accounts();

    form(&mut app).prompt = todo::editor::Editor::new("fix the login");
    app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);

    let error = form(&mut app).error.clone().expect("the form says why");
    assert!(error.contains("there is no claude account `home`"), "{error}");
    assert_eq!(app.queued.queue.entries, []);
}

#[rstest]
#[case::no_accounts(false, true)]
#[case::picker_off(true, false)]
fn without_accounts_or_with_the_picker_off_the_form_has_no_account(#[case] accounts: bool, #[case] picker: bool) {
    let (mut app, _rx, _repo, config, _worktrees) = queued_app();
    if accounts {
        with_profiles(&mut app, &config);
    }
    app.config.account_picker = picker;
    app.new_queue_form(None);
    assert!(form(&mut app).account.is_none());
    assert!(!form(&mut app).controls().contains(&Control::Account));
    save(&mut app);
    assert_eq!(app.queued.queue.entries.last().expect("entry").task.accounts, BTreeMap::new());
}

#[test]
fn editing_an_entry_whose_account_is_gone_says_so_and_keeps_it() {
    let (mut app, id, held) = scheduled(claude("home"));
    accounts::remove(&held.2.path().join("accounts"), Known::Claude, "home").expect("remove");
    app.relist_accounts();

    app.new_queue_form(Some(id));
    let error = form(&mut app).error.clone().expect("the form says why");
    assert_eq!(error, "the claude account `home` is gone: pick another account");
    assert_eq!(shown(&mut app), ("System work".into(), None));
    form(&mut app).clock = todo::editor::Editor::new("23:59");
    app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);

    let error = form(&mut app).error.clone().expect("the save is refused");
    assert!(error.contains("there is no claude account `home`"), "{error}");
    assert_eq!(app.queued.queue.entry(id).expect("entry").task.accounts, claude("home"));
}
