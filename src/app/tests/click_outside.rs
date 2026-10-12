use super::*;

fn outside() -> Position {
    Position::new(0, 0)
}

#[test]
fn a_click_outside_a_form_closes_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.overlay = Some(Overlay::NewGroup { input: "work".into() });

    click(&mut app, outside());

    assert!(app.overlay.is_none(), "{:?}", app.overlay);
}

#[test]
fn a_click_inside_a_form_keeps_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.overlay = Some(Overlay::NewGroup { input: "work".into() });
    let form = ui::form_area(AREA);

    click(&mut app, Position::new(form.x + 1, form.y + 1));

    assert!(matches!(app.overlay, Some(Overlay::NewGroup { .. })), "{:?}", app.overlay);
}

#[test]
fn a_click_outside_the_settings_closes_them() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.open_settings();

    click(&mut app, outside());

    assert!(app.overlay.is_none(), "{:?}", app.overlay);
}

#[test]
fn a_busy_dialog_stays_open() {
    let (mut app, _rx, _dirs) = app_with(1);
    let project = app.projects[0].id;
    app.overlay = Some(Overlay::NewWorkspace {
        project,
        input: "login".into(),
        worktree: None,
        error: None,
        creating: true,
    });

    click(&mut app, outside());

    assert!(matches!(app.overlay, Some(Overlay::NewWorkspace { creating: true, .. })), "{:?}", app.overlay);
}
