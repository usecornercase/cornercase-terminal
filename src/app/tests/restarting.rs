use super::*;

fn open(app: &mut App) {
    click(app, areas().settings.as_position());
    click(app, ui::settings_restart(ui::settings_area(AREA)).as_position());
}

fn notes(app: &App) -> String {
    let Some(ui::Overlay::Update(dialog)) = app.overlay_view(app.overlay.as_ref().expect("open"), AREA) else {
        panic!("the restart dialog");
    };
    dialog.notes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
}

#[test]
fn the_settings_button_says_what_stops_before_restarting() {
    let (mut app, _rx) = empty_app();
    app.open_here(AREA).expect("open a project");
    running_sleep(&mut app);

    open(&mut app);

    assert!(matches!(app.overlay, Some(Overlay::Restart)));
    assert!(notes(&app).contains("sleep in"), "{}", notes(&app));
}

#[test]
fn restart_now_restarts() {
    let (mut app, _rx) = empty_app();
    open(&mut app);
    click(&mut app, ui::update_buttons(AREA, RESTART_SUBMIT, ui::CANCEL_LABEL)[0].as_position());
    assert_eq!((app.overlay.is_none(), app.take_restart().is_some()), (true, true));
}

#[test]
fn cancel_closes_without_restarting() {
    let (mut app, _rx) = empty_app();
    open(&mut app);
    click(&mut app, ui::update_buttons(AREA, RESTART_SUBMIT, ui::CANCEL_LABEL)[1].as_position());
    assert_eq!((app.overlay.is_none(), app.take_restart()), (true, None));
}
