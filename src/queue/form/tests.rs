use rstest::rstest;

use super::*;

fn choice(label: &str, worktree: bool) -> Choice {
    Choice { label: label.into(), place: Place::default(), worktree }
}

fn form(when: When) -> Form {
    let places = vec![choice("web-shop · root", true), choice("web-shop › main", false)];
    let mut form =
        Form::new(places, Some(0), vec!["auto".into(), "claude".into()], 1, "Europe/Madrid".into(), "2026-10-20");
    form.prompt = Editor::new("fix the login");
    form.when = when.combo();
    form.clock = Editor::new("14:30");
    form.branch = Editor::new("fix-login");
    form
}

fn press(form: &mut Form, code: KeyCode) -> Step {
    form.key(KeyEvent::new(code, KeyModifiers::NONE), 60, 8)
}

#[rstest]
#[case::at(When::At, "14:30")]
#[case::tomorrow(When::Tomorrow, "tomorrow 14:30")]
#[case::on(When::On, "2026-10-20 14:30")]
#[case::delay(When::In, "in 2h")]
#[case::claude(When::ClaudeReset, "claude reset")]
#[case::codex(When::CodexReset, "codex reset")]
fn each_kind_of_time_writes_the_text_the_server_reads(#[case] when: When, #[case] text: &str) {
    assert_eq!(form(when).at(), text);
}

#[rstest]
#[case::at(When::At, &[Control::Clock][..])]
#[case::on(When::On, &[Control::Date, Control::Clock])]
#[case::delay(When::In, &[Control::Amount, Control::Unit])]
#[case::reset(When::ClaudeReset, &[])]
fn only_the_values_a_kind_needs_are_shown(#[case] when: When, #[case] values: &[Control]) {
    let controls = form(when).controls();
    let place = controls.iter().position(|control| *control == Control::Place).expect("place");
    assert_eq!(&controls[2..place], values);
}

#[test]
fn the_branch_shows_only_for_a_new_worktree_in_a_repository_root() {
    let mut f = form(When::In);
    assert!(f.controls().contains(&Control::OpenIn) && !f.controls().contains(&Control::Branch));
    f.open_in.choose(1);
    assert!(f.worktree() && f.controls().contains(&Control::Branch));
    f.place.choose(1);
    assert!(!f.worktree() && !f.controls().contains(&Control::OpenIn));
}

#[test]
fn tab_and_shift_tab_walk_the_shown_controls_and_close_an_open_list() {
    let mut f = form(When::At);
    press(&mut f, KeyCode::Tab);
    assert_eq!(f.focus, Control::When);
    press(&mut f, KeyCode::Down);
    assert_eq!(f.opened(), Some(Control::When));
    press(&mut f, KeyCode::Tab);
    assert_eq!((f.focus, f.opened()), (Control::Clock, None));
    press(&mut f, KeyCode::BackTab);
    press(&mut f, KeyCode::BackTab);
    press(&mut f, KeyCode::BackTab);
    assert_eq!(f.focus, Control::Mode);
}

#[test]
fn choosing_a_kind_changes_the_values_and_says_what_was_chosen() {
    let mut f = form(When::At);
    f.focus = Control::When;
    press(&mut f, KeyCode::Down);
    press(&mut f, KeyCode::Down);
    press(&mut f, KeyCode::Down);
    press(&mut f, KeyCode::Down);
    assert_eq!(press(&mut f, KeyCode::Enter), Step::Chose(Control::When));
    assert_eq!((f.kind(), f.at()), (When::In, "in 2h".to_string()));
}

#[test]
fn enter_submits_unless_a_list_is_open_and_escape_closes_the_list_first() {
    let mut f = form(When::At);
    assert_eq!(press(&mut f, KeyCode::Enter), Step::Submit);
    f.focus = Control::Agent;
    assert_eq!(press(&mut f, KeyCode::Enter), Step::Submit);
    press(&mut f, KeyCode::Char(' '));
    assert_eq!(f.opened(), Some(Control::Agent));
    assert_eq!(press(&mut f, KeyCode::Enter), Step::None);
    press(&mut f, KeyCode::Up);
    assert_eq!(press(&mut f, KeyCode::Esc), Step::None);
    assert_eq!(press(&mut f, KeyCode::Esc), Step::Cancel);
}

#[test]
fn typing_goes_to_the_focused_field() {
    let mut f = form(When::In);
    f.focus = Control::Amount;
    press(&mut f, KeyCode::Backspace);
    press(&mut f, KeyCode::Char('9'));
    press(&mut f, KeyCode::Char('0'));
    assert_eq!(f.at(), "in 90h");
}
