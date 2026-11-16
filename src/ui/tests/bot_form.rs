use super::*;
use crate::accounts::AgentAccounts;
use crate::bots::form::{Field, Form};
use crate::control::bots::AgentInfo;
use crate::ui::dialog::bot as form_dialog;

fn agent(name: &str, bot: bool, why: Option<&str>, accesses: &[&str], efforts: &[&str]) -> AgentInfo {
    AgentInfo {
        agent: name.into(),
        title: name.to_uppercase(),
        installed: bot,
        bot,
        why: why.map(String::from),
        reads: None,
        accesses: accesses.iter().map(ToString::to_string).collect(),
        efforts: efforts.iter().map(ToString::to_string).collect(),
        models: vec!["opus".into(), "sonnet".into()],
    }
}

fn agents() -> Vec<AgentInfo> {
    vec![
        agent("claude", true, None, &["cornercase", "full"], &["low", "high"]),
        agent("gemini", false, Some("not yet"), &[], &[]),
        agent("codex", true, Some("its sandbox cannot reach cornercase"), &["full"], &[]),
        AgentInfo { reads: Some("folder".into()), ..agent("copilot", true, None, &["cornercase", "full"], &[]) },
        agent("opencode", true, Some("a command can write outside its folder"), &["full"], &[]),
    ]
}

fn shown(form: Form) -> View<'static> {
    View { overlay: Some(Overlay::BotForm(Box::new(form))), ..view(&["cornercase"]) }
}

fn drawn(form: Form) -> Terminal<TestBackend> {
    render_sized(&shown(form), 100, 24)
}

#[test]
fn a_new_bot_with_claude_shows_both_access_notes() {
    insta::assert_snapshot!(drawn(Form::new(agents(), "claude")).backend());
}

#[test]
fn an_agent_that_reads_only_its_folder_says_so_in_the_note() {
    insta::assert_snapshot!(drawn(Form::new(agents(), "copilot")).backend());
}

#[test]
fn the_open_access_list_has_room_for_the_reason_a_cornercase_access_is_missing() {
    let mut form = Form::new(agents(), "opencode");
    form.focus_on(Field::Access);
    form.access.open();
    insta::assert_snapshot!(drawn(form).backend());
}

#[test]
fn the_agent_list_greys_an_agent_that_cannot_be_a_bot_with_its_reason() {
    let mut form = Form::new(agents(), "claude");
    form.focus_on(Field::Agent);
    form.agent.open();
    insta::assert_snapshot!(drawn(form).backend());
}

#[test]
fn codex_has_no_access_chosen_for_the_user() {
    let mut form = Form::new(agents(), "codex");
    form.name = crate::editor::Editor::new("digest");
    insta::assert_snapshot!(drawn(form).backend());
}

#[test]
fn a_refusal_of_the_server_shows_under_the_form() {
    let mut form = Form::new(agents(), "claude");
    form.error = Some("there is a bot called PRs already".into());
    insta::assert_snapshot!(drawn(form).backend());
}

#[test]
fn an_account_row_shows_when_the_agent_has_accounts() {
    let mut form = Form::new(agents(), "claude");
    form.account =
        Some(AgentAccounts::new("claude".into(), &["work".into()], vec!["default".into(), "work".into()], ""));
    insta::assert_snapshot!(drawn(form).backend());
}

#[test]
fn editing_shows_save_and_the_agent_in_grey() {
    let bot = crate::bots::sample("prs", "PRs");
    let form = Form::editing(agents(), &bot, "Review my pull requests.");
    insta::assert_snapshot!(drawn(form).backend());
}

#[test]
fn the_border_is_drawn_at_the_corners_of_its_area() {
    let v = shown(Form::new(agents(), "claude"));
    let (top_left, bottom_right) = corners_of_its_area(&v);
    assert_eq!((top_left.as_str(), bottom_right.as_str()), ("╭", "╯"));
}

#[test]
fn a_click_on_the_buttons_and_the_fields_hits_them() {
    let screen = Rect::new(0, 0, 100, 24);
    let form = Form::new(agents(), "claude");
    let r = form_dialog::rect(screen, &form, Field::Model);
    assert!(matches!(
        form_dialog::hit(screen, &form, r.as_position()),
        Some(form_dialog::Hit::Field(Field::Model, 0, _))
    ));
    let area = form_dialog::area(screen);
    let corner = Position::new(area.right() - 4, area.bottom() - 2);
    assert!(matches!(form_dialog::hit(screen, &form, corner), Some(form_dialog::Hit::Cancel)));
}

#[test]
fn the_form_has_the_follow_box() {
    let mut form = Form::new(agents(), "claude");
    let text = format!("{}", drawn(form.clone()).backend());
    assert!(text.contains("Follow") && text.contains("[x] tell it when an agent it starts finishes"), "{text}");
    form.follow = false;
    form.focus_on(Field::Follow);
    let off = format!("{}", drawn(form).backend());
    assert!(off.contains("[ ] tell it when an agent it starts finishes"), "{off}");
}
