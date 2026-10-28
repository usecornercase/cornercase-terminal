use crossterm::event::{KeyCode, KeyModifiers};

use super::*;

const ROOM: Room = Room { setup_width: 40, setup_rows: 4, teardown_width: 40, teardown_rows: 2, builds_width: 40 };

fn key(form: &mut Form, code: KeyCode) -> Action {
    form.key(KeyEvent::new(code, KeyModifiers::NONE), ROOM)
}

fn typed(form: &mut Form, text: &str) {
    for c in text.chars() {
        key(form, KeyCode::Char(c));
    }
}

#[test]
fn enter_adds_a_line_and_save_is_the_button_only() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    typed(&mut form, "npm ci");
    assert_eq!(key(&mut form, KeyCode::Enter), Action::None);
    typed(&mut form, "cp .env .env");

    let action = form.hit(Hit::Button(SAVE), ROOM);

    assert_eq!(
        action,
        Action::Save(ProjectSettings {
            setup: "npm ci\ncp .env .env".into(),
            teardown: String::new(),
            ..Default::default()
        })
    );
}

#[test]
fn ctrl_left_crosses_a_line_break() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    typed(&mut form, "npm ci");
    key(&mut form, KeyCode::Enter);
    typed(&mut form, "cp");
    for _ in 0..2 {
        form.key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL), ROOM);
    }
    typed(&mut form, "x");
    assert_eq!(form.setup.text(), "npm cix\ncp");
}

#[test]
fn tab_moves_between_the_fields_and_esc_closes() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    key(&mut form, KeyCode::Tab);
    typed(&mut form, "docker compose down");

    assert_eq!(form.focus, Field::Teardown);
    assert_eq!(form.settings().teardown, "docker compose down");
    assert_eq!(key(&mut form, KeyCode::Esc), Action::Close);
}

#[test]
fn a_click_focuses_the_field_and_places_the_cursor() {
    let saved = ProjectSettings { setup: "abc\ndef".into(), teardown: "x".into(), ..Default::default() };
    let mut form = Form::new(1, "shop".into(), &saved);

    form.hit(Hit::Field { field: Field::Setup, line: 1, col: 2 }, ROOM);
    typed(&mut form, "!");

    assert_eq!(form.settings().setup, "abc\nde!f");
}

#[test]
fn paste_keeps_its_line_breaks() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    form.paste("a\r\nb", ROOM);
    assert_eq!(form.settings().setup, "a\nb");
}

#[test]
fn the_hint_follows_the_focused_field() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    assert!(form.hint().contains("each new worktree"));
    key(&mut form, KeyCode::Tab);
    assert!(form.hint().contains("before a worktree is removed"));
    key(&mut form, KeyCode::Tab);
    assert!(form.hint().contains("folders free builds deletes"));
}

#[test]
fn tab_cycles_three_fields_and_the_builds_field_starts_with_the_effective_list() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    assert_eq!(form.builds.text(), "node_modules target .next .turbo dist build .venv __pycache__");
    for expected in [Field::Teardown, Field::Builds, Field::Setup] {
        key(&mut form, KeyCode::Tab);
        assert_eq!(form.focus, expected);
    }
}

#[test]
fn enter_in_build_folders_saves_the_list_and_the_defaults_save_as_none() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    form.focus = Field::Builds;
    assert_eq!(key(&mut form, KeyCode::Enter), Action::Save(ProjectSettings::default()));
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    form.focus = Field::Builds;
    typed(&mut form, " out");
    match key(&mut form, KeyCode::Enter) {
        Action::Save(settings) => assert_eq!(settings.builds.expect("list").last().map(String::as_str), Some("out")),
        other => panic!("not a save: {other:?}"),
    }
}

#[test]
fn a_bad_build_folder_sets_a_notice_and_saves_nothing() {
    let mut form = Form::new(1, "shop".into(), &ProjectSettings::default());
    form.focus = Field::Builds;
    typed(&mut form, " a/b");
    assert_eq!(key(&mut form, KeyCode::Enter), Action::None);
    assert_eq!(form.notice.as_deref(), Some("`a/b` is not a folder name"));
    assert_eq!(form.hit(Hit::Button(SAVE), ROOM), Action::None);
}

#[test]
fn an_empty_build_folders_field_means_none_and_pasting_keeps_one_line() {
    let saved = ProjectSettings { builds: Some(vec!["out".into()]), ..Default::default() };
    let mut form = Form::new(1, "shop".into(), &saved);
    form.focus = Field::Builds;
    form.paste("\nx\ny", ROOM);
    assert_eq!(form.builds.text(), "outx y");
    let mut empty = Form::new(1, "shop".into(), &ProjectSettings { builds: Some(Vec::new()), ..Default::default() });
    empty.focus = Field::Builds;
    assert_eq!(
        key(&mut empty, KeyCode::Enter),
        Action::Save(ProjectSettings { builds: Some(Vec::new()), ..Default::default() })
    );
}
