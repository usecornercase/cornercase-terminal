use crossterm::event::KeyModifiers;
use rstest::rstest;

use super::*;

fn choice(label: &str, worktree: bool) -> Choice {
    Choice { label: label.into(), place: Place::default(), worktree }
}

fn form(when: When) -> Form {
    let places = vec![choice("web-shop · root", true), choice("web-shop › main", false)];
    let choices =
        Choices { places, place: Some(0), kinds: vec!["auto".into(), "claude".into()], agent: 1, flows: Vec::new() };
    let mut form = Form::new(choices, "Europe/Madrid".into(), "2026-10-20");
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

#[test]
fn ctrl_and_the_arrows_move_a_field_by_word() {
    let mut f = form(When::At);
    f.focus = Control::Prompt;
    f.prompt = Editor::new("fix the login");
    f.key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL), 60, 8);
    press(&mut f, KeyCode::Char('x'));
    assert_eq!(f.prompt.text(), "fix the xlogin");
}

mod routines {
    use super::*;

    fn routine_form(when: When) -> Form {
        let mut f = form(when);
        f.places.push(choice("api · root", true));
        f.flows = vec!["Quick".into()];
        f.refit();
        f
    }

    #[rstest]
    #[case::every(When::Every, "2h")]
    #[case::daily(When::Daily, "day 14:30")]
    #[case::weekdays(When::Weekdays, "weekday 14:30")]
    #[case::weekly(When::Weekly, "monday 14:30")]
    fn each_repeat_writes_the_text_the_server_reads(#[case] when: When, #[case] text: &str) {
        let f = routine_form(when);
        assert_eq!((f.repeat().as_deref(), f.at().as_str()), (Some(text), ""));
        assert!(form(When::In).repeat().is_none());
    }

    #[test]
    fn weeks_are_a_unit_of_a_delay_and_of_a_repeat() {
        let mut f = routine_form(When::In);
        f.unit.choose(3);
        assert_eq!(f.at(), "in 2w");
        f.when = When::Every.combo();
        assert_eq!(f.repeat().as_deref(), Some("2w"));
    }

    #[rstest]
    #[case::weekly(When::Weekly, &[Control::Day, Control::Clock][..])]
    #[case::daily(When::Daily, &[Control::Clock])]
    #[case::every(When::Every, &[Control::Amount, Control::Unit])]
    fn a_routine_shows_its_values_but_neither_open_in_nor_a_branch(#[case] when: When, #[case] values: &[Control]) {
        let f = routine_form(when);
        let controls = f.controls();
        let place = controls.iter().position(|control| *control == Control::Place).expect("place");
        assert_eq!(&controls[2..place], values);
        assert!(!controls.contains(&Control::OpenIn) && !controls.contains(&Control::Branch));
        assert_eq!(controls.last(), Some(&Control::Cleanup));
    }

    #[test]
    fn only_repository_roots_are_offered_and_the_chosen_project_is_kept() {
        let mut f = routine_form(When::Daily);
        assert_eq!(f.place.options(), ["web-shop · root", "api · root"]);
        f.place.choose(1);
        f.when = When::In.combo();
        f.refit();
        assert_eq!(f.place.options().len(), 3);
        assert_eq!(f.choice().map(|choice| choice.label.as_str()), Some("api · root"));
        f.when = When::Daily.combo();
        f.refit();
        assert_eq!(f.choice().map(|choice| choice.label.as_str()), Some("api · root"));
    }

    #[test]
    fn a_workspace_choice_falls_back_to_a_root_of_its_own_project() {
        let mut f = routine_form(When::In);
        let project = |label: &str, path: &str, workspace: Option<&str>| {
            let mut found = choice(label, true);
            found.place.project = path.into();
            found.place.workspace = workspace.map(Into::into);
            found
        };
        f.places = vec![
            project("web-shop · root", "/web", None),
            project("api · root", "/api", None),
            project("api · feature", "/api", Some("/api/feature")),
        ];
        f.refit();
        f.place.choose(2);
        f.when = When::Daily.combo();
        f.refit();
        assert_eq!(f.choice().map(|choice| choice.label.as_str()), Some("api · root"));
    }

    #[test]
    fn flows_are_listed_for_routines_only_and_drop_mode_and_account() {
        let mut f = routine_form(When::Daily);
        assert_eq!(f.agent.options(), ["auto", "claude", "Quick (flow)"]);
        assert_eq!(f.flow(), None);
        f.agent.choose(2);
        assert_eq!(f.flow(), Some("Quick"));
        assert!(!f.controls().contains(&Control::Mode));
        f.when = When::In.combo();
        f.refit();
        assert_eq!(f.agent.options(), ["auto", "claude"]);
        assert_eq!(f.flow(), None);
    }

    #[test]
    fn space_toggles_the_cleanup_unless_the_agent_cannot_be_read() {
        let mut f = routine_form(When::Daily);
        f.focus = Control::Cleanup;
        press(&mut f, KeyCode::Char(' '));
        assert!(f.cleanup);
        f.cleanup_off = Some("cornercase cannot tell when mine finishes".into());
        assert!(!f.controls().contains(&Control::Cleanup));
    }
}
