use super::*;
use crate::issues::browser::{Accounts, AgentAccounts};

fn claude_with_home_and_work() -> Accounts {
    let claude = AgentAccounts {
        names: vec!["home".into(), "work".into()],
        labels: vec!["Main".into(), "home".into(), "Job".into()],
        active: "work".into(),
    };
    Accounts { agents: HashMap::from([("claude".to_string(), claude)]), chosen: None }
}

fn reading(accounts: Option<Accounts>) -> Browser {
    let mut b = Browser { accounts, ..with_lists() };
    press(&mut b, KeyCode::Enter);
    b
}

fn row(b: &Browser, label: &str) -> Option<String> {
    b.detail_lines(ROOM)
        .into_iter()
        .find(|l| l.spans.first().is_some_and(|s| s.content.trim() == label))
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
}

fn pick(b: &mut Browser, text: &str) {
    let pos = button(b, "account…");
    click(b, pos);
    type_text(b, text);
    press(b, KeyCode::Enter);
}

fn pick_agent(b: &mut Browser, kind: &str) {
    let pos = button(b, "agent…");
    click(b, pos);
    type_text(b, kind);
    press(b, KeyCode::Enter);
}

fn offered(b: &Browser) -> (bool, Option<String>) {
    (b.labels(ROOM.buttons).contains(&"account…"), row(b, "account"))
}

#[test]
fn without_accounts_nothing_is_offered() {
    assert_eq!(offered(&reading(None)), (false, None));
}

#[test]
fn the_account_in_use_is_the_default_with_its_name() {
    let b = reading(Some(claude_with_home_and_work()));
    assert_eq!(offered(&b), (true, Some("account Job  (in use)".into())));
}

#[test]
fn the_picker_lists_the_agent_s_own_login_and_every_account_from_the_one_in_use() {
    let mut b = reading(Some(claude_with_home_and_work()));
    let pos = button(&b, "account…");
    click(&mut b, pos);
    let choices: Vec<(String, String, String)> = b.choices().into_iter().map(|c| (c.key, c.title, c.meta)).collect();
    let selected = b.picker.as_ref().map(|p| (p.pick, p.search.selected()));
    assert_eq!(
        (choices, selected),
        (
            vec![
                ("Main".into(), "the agent's own login".into(), String::new()),
                ("home".into(), String::new(), String::new()),
                ("Job".into(), String::new(), "in use".into()),
            ],
            Some((Pick::Account, 2))
        )
    );
}

#[test]
fn another_account_can_be_picked_by_its_name() {
    let mut b = reading(Some(claude_with_home_and_work()));
    pick(&mut b, "home");
    let chosen = b.accounts.as_ref().and_then(|a| a.chosen.clone());
    assert_eq!((chosen, row(&b, "account").as_deref()), (Some(("claude".into(), "home".into())), Some("account home")));
}

#[test]
fn the_agent_s_own_login_is_picked_as_no_account() {
    let mut b = reading(Some(claude_with_home_and_work()));
    pick(&mut b, "main");
    let chosen = b.accounts.as_ref().and_then(|a| a.chosen.clone());
    assert_eq!((chosen, row(&b, "account").as_deref()), (Some(("claude".into(), String::new())), Some("account Main")));
}

#[test]
fn picking_an_account_does_not_start() {
    let mut b = reading(Some(claude_with_home_and_work()));
    let pos = button(&b, "account…");
    click(&mut b, pos);
    assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
}

#[test]
fn an_agent_without_accounts_hides_the_pick() {
    let mut b = reading(Some(claude_with_home_and_work()));
    pick_agent(&mut b, "codex");
    assert_eq!(offered(&b), (false, None));
}

#[test]
fn another_agent_forgets_the_pick_and_coming_back_shows_the_one_in_use() {
    let mut b = reading(Some(claude_with_home_and_work()));
    pick(&mut b, "home");
    pick_agent(&mut b, "codex");
    pick_agent(&mut b, "claude");
    let chosen = b.accounts.as_ref().and_then(|a| a.chosen.clone());
    assert_eq!((chosen, row(&b, "account").as_deref()), (None, Some("account Job  (in use)")));
}

#[test]
fn starting_asks_for_the_account_and_then_starts_with_the_one_chosen() {
    let mut b = reading(Some(claude_with_home_and_work()));
    let pos = button(&b, "start");
    click(&mut b, pos);
    assert_eq!(b.picker.as_ref().map(|p| p.pick), Some(Pick::Account));
    type_text(&mut b, "Main");
    assert!(matches!(press(&mut b, KeyCode::Enter), Action::Name(..)));
    assert_eq!(b.accounts.as_ref().and_then(|a| a.chosen.clone()), Some(("claude".into(), String::new())));
}

#[test]
fn reading_another_issue_forgets_the_account_chosen_for_the_last() {
    let mut b = reading(Some(claude_with_home_and_work()));
    pick(&mut b, "home");
    press(&mut b, KeyCode::Esc);
    press(&mut b, KeyCode::Enter);
    assert_eq!(b.accounts.as_ref().and_then(|a| a.chosen.clone()), None);
}
