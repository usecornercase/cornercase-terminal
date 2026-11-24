use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

fn agent(name: &str, bot: bool, accesses: &[&str], efforts: &[&str]) -> AgentInfo {
    AgentInfo {
        agent: name.into(),
        title: name.to_uppercase(),
        installed: true,
        bot,
        why: (!bot).then(|| String::from("not yet")),
        accesses: accesses.iter().map(ToString::to_string).collect(),
        reads: None,
        efforts: efforts.iter().map(ToString::to_string).collect(),
        models: vec!["opus".into(), "sonnet".into()],
    }
}

fn agents() -> Vec<AgentInfo> {
    vec![
        agent("claude", true, &["cornercase", "full"], &["low", "high"]),
        agent("gemini", false, &[], &[]),
        agent("codex", true, &["full"], &[]),
    ]
}

fn key(form: &mut Form, code: KeyCode) -> Step {
    form.key(KeyEvent::new(code, KeyModifiers::NONE), 40, 8)
}

#[test]
fn the_preferred_agent_is_chosen_when_it_can_be_a_bot() {
    assert_eq!(Form::new(agents(), "codex").info().map(|info| info.agent.as_str()), Some("codex"));
    assert_eq!(Form::new(agents(), "gemini").info().map(|info| info.agent.as_str()), Some("claude"));
}

#[test]
fn an_agent_that_cannot_be_a_bot_is_greyed_with_its_reason() {
    let form = Form::new(agents(), "claude");
    assert_eq!(form.agent.note(1), Some("not yet"));
    assert_eq!(form.agent.note(0), None);
}

#[test]
fn the_effort_shows_only_for_an_agent_that_takes_one() {
    let mut form = Form::new(agents(), "claude");
    assert!(form.fields().contains(&Field::Effort));
    form.agent.choose(2);
    form.chose(Field::Agent);
    assert!(!form.fields().contains(&Field::Effort));
}

#[test]
fn the_account_shows_only_with_accounts() {
    let mut form = Form::new(agents(), "claude");
    assert!(!form.fields().contains(&Field::Account));
    form.account =
        Some(AgentAccounts::new("claude".into(), &["work".into()], vec!["default".into(), "work".into()], ""));
    assert!(form.fields().contains(&Field::Account));
}

#[test]
fn tab_skips_the_agent_when_editing() {
    let bot = crate::bots::sample("prs", "PRs");
    let mut form = Form::editing(agents(), &bot, "");
    key(&mut form, KeyCode::Tab);
    assert_eq!(form.focus, Field::About);
    key(&mut form, KeyCode::Tab);
    assert_eq!(form.focus, Field::Model);
}

#[test]
fn what_it_is_for_comes_right_after_the_name() {
    let form = Form::new(agents(), "claude");
    assert_eq!(form.fields()[..2], [Field::Name, Field::About]);
}

#[test]
fn enter_breaks_a_line_in_what_it_is_for_and_submits_elsewhere() {
    let mut form = Form::new(agents(), "claude");
    form.focus_on(Field::About);
    assert_eq!(key(&mut form, KeyCode::Enter), Step::None);
    assert_eq!(form.about.text(), "\n");
    form.focus_on(Field::Name);
    assert_eq!(key(&mut form, KeyCode::Enter), Step::Submit);
}

#[test]
fn choosing_an_agent_refits_its_model_effort_and_access() {
    let mut form = Form::new(agents(), "claude");
    assert_eq!(form.access.label(), Some("through cornercase"));
    form.agent.choose(2);
    form.chose(Field::Agent);
    assert_eq!(form.access.chosen(), None);
    assert_eq!(form.model.label(), Some(DEFAULT));
}

#[test]
fn an_agent_without_cornercase_access_gets_no_access_chosen_for_the_user() {
    let mut form = Form::new(agents(), "codex");
    form.name = Editor::new("prs");
    assert_eq!(form.access.chosen(), None);
    assert_eq!(form.new_bot(), Err(String::from("choose its access")));
    assert!(form.access.note(0).is_some());
}

#[test]
fn a_new_bot_carries_what_the_form_holds() {
    let mut form = Form::new(agents(), "claude");
    form.name = Editor::new("PRs");
    form.about = Editor::new("review");
    form.model.choose(1);
    form.effort.choose(2);
    let new = form.new_bot().expect("a bot");
    assert_eq!(new.name, "PRs");
    assert_eq!(new.agent.as_deref(), Some("claude"));
    assert_eq!((new.model.as_deref(), new.effort.as_deref()), (Some("opus"), Some("high")));
    assert_eq!((new.access.as_deref(), new.about.as_str()), (Some("cornercase"), "review"));
}

#[test]
fn a_default_model_and_effort_are_sent_as_none() {
    let mut form = Form::new(agents(), "claude");
    form.name = Editor::new("PRs");
    let new = form.new_bot().expect("a bot");
    assert_eq!((new.model, new.effort), (None, None));
}

#[test]
fn an_agent_that_cannot_be_a_bot_is_refused_with_its_reason() {
    let mut form = Form::new(agents(), "claude");
    form.agent = Combo::new(vec!["CLAUDE".into(), "GEMINI".into()], Some(1));
    assert_eq!(form.new_bot(), Err(String::from("GEMINI cannot be a bot: not yet")));
}

#[test]
fn editing_sends_about_only_when_it_changed() {
    let bot = crate::bots::sample("prs", "PRs");
    let mut form = Form::editing(agents(), &bot, "old");
    assert_eq!(form.edit_bot().expect("an edit").about, None);
    form.about = Editor::new("new");
    assert_eq!(form.edit_bot().expect("an edit").about.as_deref(), Some("new"));
}

#[test]
fn an_empty_model_is_sent_as_an_empty_string_to_go_back_to_the_agents_own() {
    let mut bot = crate::bots::sample("prs", "PRs");
    bot.model = Some("opus".into());
    let mut form = Form::editing(agents(), &bot, "");
    assert_eq!(form.edit_bot().expect("an edit").model.as_deref(), Some("opus"));
    form.model.choose(0);
    assert_eq!(form.edit_bot().expect("an edit").model.as_deref(), Some(""));
}

#[test]
fn space_toggles_follow_and_both_requests_carry_it() {
    let mut form = Form::new(agents(), "claude");
    form.name = Editor::new("PRs");
    assert_eq!(form.new_bot().expect("a bot").follow, Some(true));
    form.focus_on(Field::Follow);
    key(&mut form, KeyCode::Char(' '));
    assert!(!form.follow);
    assert_eq!(form.new_bot().expect("a bot").follow, Some(false));
    let bot = crate::bots::sample("prs", "PRs");
    let mut editing = Form::editing(agents(), &bot, "");
    assert_eq!(editing.edit_bot().expect("an edit").follow, Some(true));
    editing.focus_on(Field::Follow);
    key(&mut editing, KeyCode::Char(' '));
    assert_eq!(editing.edit_bot().expect("an edit").follow, Some(false));
}

#[test]
fn editing_starts_from_the_bots_follow() {
    let off = Bot { follow: false, ..crate::bots::sample("prs", "PRs") };
    assert!(!Form::editing(agents(), &off, "").follow);
}

#[test]
fn tab_reaches_follow_after_access_and_then_the_name_again() {
    let mut form = Form::new(agents(), "claude");
    form.focus_on(Field::Access);
    key(&mut form, KeyCode::Tab);
    assert_eq!(form.focus, Field::Follow);
    key(&mut form, KeyCode::Tab);
    assert_eq!(form.focus, Field::Name);
}

#[test]
fn the_chat_apps_row_is_reached_by_tab_only_when_a_chat_app_is_paired() {
    let mut form = Form::new(agents(), "claude");
    assert!(form.fields().contains(&Field::ChatApps));
    form.focus_on(Field::Follow);
    key(&mut form, KeyCode::Tab);
    assert_eq!(form.focus, Field::Name);
    form.chat_ready = true;
    form.focus_on(Field::Follow);
    key(&mut form, KeyCode::Tab);
    assert_eq!(form.focus, Field::ChatApps);
}

#[test]
fn both_requests_carry_chat_apps_only_when_a_chat_app_is_paired() {
    let mut form = Form::new(agents(), "claude");
    form.name = Editor::new("PRs");
    assert_eq!(form.new_bot().expect("a bot").chat_apps, None);
    form.chat_ready = true;
    assert_eq!(form.new_bot().expect("a bot").chat_apps.as_deref(), Some("away"));
    let bot = Bot { chat_apps: crate::bots::ChatApps::Always, ..crate::bots::sample("prs", "PRs") };
    let mut editing = Form::editing(agents(), &bot, "");
    assert_eq!(editing.edit_bot().expect("an edit").chat_apps, None);
    editing.chat_ready = true;
    assert_eq!(editing.edit_bot().expect("an edit").chat_apps.as_deref(), Some("always"));
    editing.chat_apps.choose(2);
    assert_eq!(editing.edit_bot().expect("an edit").chat_apps.as_deref(), Some("off"));
}
