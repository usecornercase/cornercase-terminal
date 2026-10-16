use super::*;

use crate::config;

const SIZE: Rect = AREA;

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    project: TempDir,
}

fn setup(repo: bool) -> Setup {
    let project = if repo { git_repo(&[("README", "hi")]) } else { TempDir::new() };
    let config = TempDir::new();
    let (mut app, rx) = app_in(project.path(), config.path().join("config.json"));
    app.projects[0].name = Some("web-shop".into());
    Setup { app, rx, config, project }
}

fn open(app: &mut App) {
    right_click(app, entry_pos());
    pick(app, "project settings");
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        let code = if c == '\n' { KeyCode::Enter } else { KeyCode::Char(c) };
        send_key(app, code, KeyModifiers::NONE);
    }
}

fn press(app: &mut App, label: &str) {
    let Some(Overlay::ProjectSettings(form)) = &app.overlay else { panic!("the dialog is not open") };
    let buttons = crate::project::settings::Form::buttons();
    let rects = ui::project_settings_buttons(SIZE, &buttons);
    let i = buttons.iter().position(|b| *b == label).expect("button");
    let _ = form;
    click_in(app, rects[i].as_position(), SIZE);
}

#[test]
fn only_a_repository_root_offers_project_settings() {
    let mut plain = setup(false);
    right_click(&mut plain.app, entry_pos());
    assert_eq!(menu_labels(&plain.app), ["rename project"]);

    let mut repo = setup(true);
    right_click(&mut repo.app, entry_pos());
    assert_eq!(menu_labels(&repo.app), ["rename project", "project settings"]);
}

#[test]
fn typing_then_save_writes_the_projects_key_to_config_json() {
    let mut s = setup(true);
    open(&mut s.app);
    type_text(&mut s.app, "npm ci\ncp .env .env");
    send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
    type_text(&mut s.app, "docker compose down");

    press(&mut s.app, "save");

    assert!(s.app.overlay.is_none());
    let saved = config::load(&s.config.path().join("config.json"));
    let settings = saved.project(&s.project.path().canonicalize().expect("path"), None);
    assert_eq!(settings.setup, "npm ci\ncp .env .env");
    assert_eq!(settings.teardown, "docker compose down");
    let _ = &s.rx;
}

#[test]
fn cancel_and_esc_discard_what_was_typed() {
    let mut s = setup(true);
    open(&mut s.app);
    type_text(&mut s.app, "npm ci");
    press(&mut s.app, "cancel");
    assert!(s.app.overlay.is_none());

    open(&mut s.app);
    type_text(&mut s.app, "npm ci");
    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(s.app.overlay.is_none());
    assert!(s.app.config.projects.is_empty());
}

#[test]
fn the_dialog_shows_the_saved_commands() {
    let mut s = setup(true);
    let path = s.project.path().canonicalize().expect("path");
    s.app.config.set_project(
        &path,
        None,
        config::ProjectSettings { setup: "npm ci\ncp .env .env".into(), teardown: String::new() },
    );
    open(&mut s.app);
    insta::assert_snapshot!(format!("{}", render(&mut s.app, SIZE).backend()));
}

#[test]
fn a_click_outside_closes_it() {
    let mut s = setup(true);
    open(&mut s.app);
    click_in(&mut s.app, Position::new(0, 0), SIZE);
    assert!(s.app.overlay.is_none());
}

#[test]
fn the_new_workspace_dialog_names_the_setup_when_a_worktree_is_chosen() {
    let mut s = setup(true);
    let path = s.project.path().canonicalize().expect("path");
    s.app.config.set_project(
        &path,
        None,
        config::ProjectSettings { setup: "npm ci\nnpm run build".into(), teardown: String::new() },
    );
    let id = s.app.projects[0].id;
    s.app.overlay = Some(Overlay::NewWorkspace {
        project: id,
        input: "feat".into(),
        worktree: Some(true),
        error: None,
        creating: false,
    });
    let shown = format!("{}", render(&mut s.app, SIZE).backend());
    assert!(shown.contains("then runs the project's setup: npm ci …"), "{shown}");
}
