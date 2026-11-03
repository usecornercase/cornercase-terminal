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
    assert_eq!(menu_labels(&plain.app), ["rename project", "copy path"]);

    let mut repo = setup(true);
    right_click(&mut repo.app, entry_pos());
    assert_eq!(menu_labels(&repo.app), ["rename project", "project settings", "worktrees", "copy path"]);
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
        config::ProjectSettings { setup: "npm ci\ncp .env .env".into(), teardown: String::new(), ..Default::default() },
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
        config::ProjectSettings {
            setup: "npm ci\nnpm run build".into(),
            teardown: String::new(),
            ..Default::default()
        },
    );
    let id = s.app.projects[0].id;
    s.app.overlay = Some(Overlay::NewWorkspace {
        project: id,
        input: Editor::new("feat"),
        worktree: Some(true),
        error: None,
        creating: false,
    });
    let shown = format!("{}", render(&mut s.app, SIZE).backend());
    assert!(shown.contains("then runs the project's setup: npm ci …"), "{shown}");
}

mod selecting {
    use super::*;
    use crate::project::settings::Field;

    fn field_pos(col: u16, row: u16) -> Position {
        let text = ui::project_settings_field(SIZE, Field::Setup);
        Position::new(text.x + col, text.y + row)
    }

    #[test]
    fn right_click_in_a_dialog_field_keeps_the_dialog() {
        let mut s = setup(true);
        open(&mut s.app);
        type_text(&mut s.app, "npm ci");
        right_click(&mut s.app, field_pos(2, 0));
        assert!(s.app.screen.field_menu.is_some() && matches!(s.app.overlay, Some(Overlay::ProjectSettings(_))));
        let menu = ui::menu_area(SIZE, field_pos(2, 0), &["copy", "cut", "select all"]);
        click_in(&mut s.app, ui::menu_item(menu, 2).as_position(), SIZE);
        type_text(&mut s.app, "x");
        let Some(Overlay::ProjectSettings(form)) = &s.app.overlay else { panic!("the dialog is not open") };
        assert_eq!(form.setup.text(), "x");
    }

    #[test]
    fn a_drag_from_the_field_past_the_dialog_keeps_it_open() {
        let mut s = setup(true);
        open(&mut s.app);
        type_text(&mut s.app, "npm ci");
        mouse_in(&mut s.app, MouseEventKind::Down(MouseButton::Left), field_pos(1, 0), SIZE);
        mouse_in(&mut s.app, MouseEventKind::Drag(MouseButton::Left), Position::new(0, 0), SIZE);
        mouse_in(&mut s.app, MouseEventKind::Up(MouseButton::Left), Position::new(0, 0), SIZE);
        assert!(matches!(s.app.overlay, Some(Overlay::ProjectSettings(_))));
    }

    #[test]
    fn dragging_above_a_scrolled_field_scrolls_it_up() {
        let mut s = setup(true);
        open(&mut s.app);
        type_text(&mut s.app, &"x\n".repeat(40));
        let scroll = |app: &App| match &app.overlay {
            Some(Overlay::ProjectSettings(form)) => form.setup.scroll(),
            _ => panic!("the dialog is not open"),
        };
        let before = scroll(&s.app);
        mouse_in(&mut s.app, MouseEventKind::Down(MouseButton::Left), field_pos(0, 3), SIZE);
        let above = ui::project_settings_field(SIZE, Field::Setup).y - 1;
        mouse_in(&mut s.app, MouseEventKind::Drag(MouseButton::Left), Position::new(0, above), SIZE);
        mouse_in(&mut s.app, MouseEventKind::Drag(MouseButton::Left), Position::new(0, above), SIZE);
        assert!(before > 0 && scroll(&s.app) < before);
    }
}
