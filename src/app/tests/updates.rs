use super::*;
use crate::error::Error;
use crate::test_util::FakeHttp;

fn scrolled_notes() -> (App, Receiver<AppEvent>) {
    let notes: Vec<String> = (0..60).map(|i| format!("- change {i}")).collect();
    let (mut app, rx) = with_notes(&notes.join("\n"));
    mouse(&mut app, MouseEventKind::ScrollDown, ui::update_notes(AREA).as_position());
    (app, rx)
}

const LATEST: &str = r#"{"tag_name": "v9.0.0", "assets": []}"#;

fn release() -> Release {
    update::release(&serde_json::json!({"tag_name": "v9.0.0", "assets": []})).expect("a release")
}

fn found(install: Install) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = empty_app();
    app.update.updates.install = install;
    app.handle_event(AppEvent::Update(UpdateEvent::Checked(Ok(Some(release())))), AREA).expect("handle check");
    (app, rx)
}

fn replaced() -> Install {
    Install::Replace(PathBuf::from("/opt/cc/bin/cornercase"))
}

fn open(app: &mut App) {
    let label = app.update_label().expect("an update is shown");
    click(app, ui::update_button(areas().settings, &label).as_position());
}

fn step(app: &App) -> Option<&UpdateStep> {
    match &app.overlay {
        Some(Overlay::Update(step)) => Some(step),
        _ => None,
    }
}

fn submit(app: &mut App) {
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
}

fn installed(app: &mut App) -> String {
    app.update.updates.installed = true;
    open(app);
    let Some(ui::Overlay::Update(update)) = app.overlay_view(app.overlay.as_ref().expect("open"), AREA) else {
        panic!("the update dialog");
    };
    update.message
}

#[test]
fn nothing_running_is_said_before_restarting() {
    let (mut app, _rx) = found(replaced());
    app.open_here(AREA).expect("open a project");
    wait_until("the shell is at its prompt", || app.term().is_some_and(Term::shell_in_foreground));

    installed(&mut app);

    assert!(shown_notes(&app).join(" ").contains("Nothing is running in your terminals."));
}

#[test]
fn the_whole_restart_list_is_shown() {
    let (mut app, _rx) = found(replaced());
    app.open_here(AREA).expect("open a project");
    running_sleep(&mut app);

    let message = installed(&mut app);

    let notes = shown_notes(&app).join(" ");
    assert_eq!(message, "cornercase 9.0.0 is installed. Restart to use it.");
    assert!(notes.contains("sleep in") && notes.contains("each tab with a new shell in its folder."), "{notes}");
}

#[test]
fn later_closes_without_restarting() {
    let (mut app, _rx) = found(replaced());
    installed(&mut app);

    click(&mut app, ui::update_buttons(AREA, RESTART_SUBMIT, LATER)[1].as_position());

    assert_eq!((app.overlay.is_none(), app.take_restart()), (true, None));
}

#[test]
fn a_newer_release_shows_a_button_and_a_toast() {
    let (app, _rx) = found(replaced());
    assert_eq!(app.update_label().as_deref(), Some("↑ 9.0.0"));
    assert_eq!(toast(&app), Some(UPDATE_AVAILABLE));
}

#[test]
fn nothing_new_shows_nothing() {
    let (mut app, _rx) = empty_app();
    app.handle_event(AppEvent::Update(UpdateEvent::Checked(Ok(None))), AREA).expect("handle check");
    assert_eq!((app.update_label(), toast(&app)), (None, None));
}

#[test]
fn the_button_asks_before_updating() {
    let (mut app, _rx) = found(replaced());
    open(&mut app);
    assert_eq!(step(&app), Some(&UpdateStep::Ask));
}

#[test]
fn the_settings_button_still_opens_settings() {
    let (mut app, _rx) = found(replaced());
    click(&mut app, areas().settings.as_position());
    assert!(matches!(app.overlay, Some(Overlay::Settings(_))));
}

fn with_notes(notes: &str) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = found(replaced());
    let body = format!("## Release Notes\n\n{notes}\n\n## Install cornercase 9.0.0\n");
    let json = serde_json::json!({"tag_name": "v9.0.0", "assets": [], "body": body});
    app.update.updates.available = update::release(&json);
    open(&mut app);
    (app, rx)
}

fn shown_notes(app: &App) -> Vec<String> {
    app.update_notes(AREA).iter().map(ToString::to_string).collect()
}

#[test]
fn the_dialog_shows_the_release_notes() {
    let (app, _rx) = with_notes("- Faster startup.");
    assert_eq!(shown_notes(&app), ["What's new in 9.0.0", "", "• Faster startup."]);
}

#[test]
fn the_wheel_scrolls_long_notes() {
    let (app, _rx) = scrolled_notes();

    assert_eq!(app.update.scroll, 3);
}

#[test]
fn the_restart_list_starts_at_its_top() {
    let (mut app, _rx) = scrolled_notes();

    app.handle_event(AppEvent::Update(UpdateEvent::Installed(Ok(()))), AREA).expect("handle the update");

    assert_eq!((step(&app), app.update.scroll), (Some(&UpdateStep::Installed), 0));
}

#[test]
fn the_dialog_buttons_update_and_cancel() {
    let (mut app, _rx) = found(replaced());
    open(&mut app);
    let [_, cancel] = ui::update_buttons(AREA, UPDATE_SUBMIT, ui::CANCEL_LABEL);
    click(&mut app, cancel.as_position());
    assert!(app.overlay.is_none());

    open(&mut app);
    let [submit, _] = ui::update_buttons(AREA, UPDATE_SUBMIT, ui::CANCEL_LABEL);
    click(&mut app, submit.as_position());
    assert_eq!(step(&app), Some(&UpdateStep::Updating));
}

#[test]
fn a_homebrew_install_offers_to_copy_the_command() {
    let (mut app, _rx) = found(Install::Command(update::BREW));
    open(&mut app);

    submit(&mut app);

    assert_eq!(app.take_host_writes(), [clipboard::osc52(update::BREW)]);
    assert!(app.overlay.is_none());
}

#[test]
fn a_finished_update_offers_a_restart() {
    let (mut app, _rx) = found(replaced());
    open(&mut app);
    app.overlay = Some(Overlay::Update(UpdateStep::Updating));

    app.handle_event(AppEvent::Update(UpdateEvent::Installed(Ok(()))), AREA).expect("handle update");
    submit(&mut app);

    assert_eq!(app.take_restart(), Some(PathBuf::from("/opt/cc/bin/cornercase")));
    assert_eq!(app.update_label().as_deref(), Some(RESTART_LABEL));
}

#[test]
fn a_failed_update_can_be_tried_again() {
    let (mut app, _rx) = found(replaced());
    app.overlay = Some(Overlay::Update(UpdateStep::Updating));

    app.handle_event(AppEvent::Update(UpdateEvent::Installed(Err(Error::Api("no network".into())))), AREA)
        .expect("handle update");

    assert_eq!(step(&app), Some(&UpdateStep::Failed("no network".into())));
    assert_eq!(app.overlay.as_ref().map(Overlay::submit_label), Some(RETRY_UPDATE_SUBMIT));
    assert_eq!(app.take_restart(), None);
}

#[test]
fn an_update_in_progress_cannot_be_dismissed() {
    let (mut app, _rx) = found(replaced());
    app.overlay = Some(Overlay::Update(UpdateStep::Updating));

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(step(&app), Some(&UpdateStep::Updating));
}

fn checking(server: &FakeHttp) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = empty_app();
    app.update.updates.enabled = true;
    app.update.updates.url = format!("{}/releases/latest", server.url());
    (app, rx)
}

#[test]
fn the_check_asks_github_once_an_hour() {
    let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
    let (mut app, rx) = checking(&server);
    let now = Instant::now();

    app.refresh(now);
    pump_until(&mut app, &rx, "the update is found", |app| app.update_label().is_some());
    app.refresh(now + Duration::from_secs(60));
    assert_eq!(server.requests().len(), 1);

    app.refresh(now + update::CHECK_EVERY);
    wait_until("the second check", || server.requests().len() == 2);
}

#[test]
fn a_later_release_replaces_the_one_shown_and_says_so_again() {
    let (mut app, _rx) = found(replaced());
    app.messages.toast = None;
    let later = update::release(&serde_json::json!({"tag_name": "v9.1.0", "assets": []})).expect("a release");

    app.handle_event(AppEvent::Update(UpdateEvent::Checked(Ok(Some(later.clone())))), AREA).expect("handle check");
    assert_eq!(app.update_label().as_deref(), Some("↑ 9.1.0"));
    assert_eq!(toast(&app), Some(UPDATE_AVAILABLE));

    app.messages.toast = None;
    app.handle_event(AppEvent::Update(UpdateEvent::Checked(Ok(Some(later)))), AREA).expect("handle check");
    assert_eq!(toast(&app), None);
}

#[test]
fn the_dialog_shows_the_changelog_of_every_version_since_this_one() {
    let changelog = format!(
        "# Changelog\n\n## 9.0.0\n\n- Newest.\n\n## 8.0.0\n\n- Skipped one.\n\n## {}\n\n- This one.\n",
        version::CURRENT
    );
    let files = FakeHttp::start(vec![("GET /CHANGELOG.md", 200, changelog)]);
    let latest = serde_json::json!({"tag_name": "v9.0.0", "assets": [
        {"name": "CHANGELOG.md", "browser_download_url": format!("{}/CHANGELOG.md", files.url())}
    ]});
    let server = FakeHttp::start(vec![("GET /releases/latest", 200, latest.to_string())]);
    let (mut app, rx) = checking(&server);
    app.update.updates.install = replaced();

    app.refresh(Instant::now());
    pump_until(&mut app, &rx, "the update is found", |app| app.update_label().is_some());
    open(&mut app);

    assert_eq!(
        shown_notes(&app),
        ["What's new in 9.0.0", "", "• Newest.", "", "What's new in 8.0.0", "", "• Skipped one."]
    );
}

#[test]
fn the_check_can_be_turned_off() {
    let (mut app, _rx) = empty_app();
    app.update.updates.enabled = true;
    app.update.updates.url = "http://127.0.0.1:1/never".into();
    app.config.check_updates = false;

    app.refresh(Instant::now());

    assert_eq!(app.update.updates.checked, None);
}
