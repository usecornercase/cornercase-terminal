use std::time::Instant;

use super::*;
use crate::project::search::Mode;
use crate::project::search::said::Origin;

const MARKER: &str = "needle-found";

fn with_marker() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(2);
    app.active = 0;
    type_line(&mut app, "echo needle-\"\"found");
    shows(&mut app, &rx, MARKER);
    app.active = 1;
    (app, rx, dirs)
}

fn open_sessions(app: &mut App) {
    click(app, areas().search.as_position());
    send_key(app, KeyCode::Tab, KeyModifiers::NONE);
}

fn mode(app: &App) -> Option<Mode> {
    match &app.overlay {
        Some(Overlay::Search(finder)) => Some(finder.mode),
        _ => None,
    }
}

fn answered(app: &mut App, rx: &Receiver<AppEvent>, query: &str) {
    wait_until("the answer", || {
        drain(app, rx);
        app.sessions.answer(query).is_some()
    });
}

#[test]
fn tab_and_the_button_switch_between_names_and_sessions() {
    let (mut app, _rx, _dirs) = app_with(1);
    click(&mut app, areas().search.as_position());
    assert_eq!(mode(&app), Some(Mode::Names));

    send_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(mode(&app), Some(Mode::Sessions));

    click(&mut app, ui::mode_button(areas().search).as_position());
    assert_eq!(mode(&app), Some(Mode::Names));
}

#[test]
fn switching_mode_clears_the_selection_and_keeps_the_search_open() {
    let (mut app, _rx, _dirs) = app_with(1);
    click(&mut app, areas().search.as_position());
    type_text(&mut app, "ab");

    send_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);

    assert!(matches!(&app.overlay, Some(Overlay::Search(finder)) if finder.field.selected() == 0));
}

#[test]
fn what_a_pane_printed_is_found_and_enter_goes_there_with_the_match_selected() {
    let (mut app, rx, _dirs) = with_marker();
    open_sessions(&mut app);
    type_text(&mut app, MARKER);
    answered(&mut app, &rx, MARKER);

    let found = app.sessions.answer(MARKER).expect("an answer");
    assert_eq!(found.matches.first().map(|m| m.origin), Some(Origin::Screen));
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!((app.overlay.is_none(), app.active), (true, 0));
    let copied = app.term().expect("a pane").emulator.screen_text().expect("text");
    assert!(copied.contains(MARKER));
    assert_eq!(app.term_mut().expect("a pane").emulator.finish_selection().expect("finish").as_deref(), Some(MARKER));
}

#[test]
fn enter_before_the_answer_opens_the_first_match_when_it_arrives() {
    let (mut app, rx, _dirs) = with_marker();
    open_sessions(&mut app);
    type_text(&mut app, MARKER);

    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(mode(&app), Some(Mode::Sessions));

    wait_until("the search opens the match", || {
        drain(&mut app, &rx);
        app.overlay.is_none()
    });
    assert_eq!(app.active, 0);
}

#[test]
fn the_results_are_two_lines_each_and_a_click_on_one_goes_there() {
    let (mut app, rx, _dirs) = with_marker();
    open_sessions(&mut app);
    type_text(&mut app, MARKER);
    answered(&mut app, &rx, MARKER);

    let at = ui::result_item(areas().results, 1, 0, 0, ui::SESSIONS_PITCH).as_position();
    click(&mut app, at);

    assert_eq!((app.overlay.is_none(), app.active), (true, 0));
}

#[test]
fn a_newer_query_drops_the_answer_of_an_older_job() {
    let (mut app, rx, _dirs) = with_marker();
    open_sessions(&mut app);
    type_text(&mut app, "needle");
    type_text(&mut app, "-found");

    answered(&mut app, &rx, MARKER);

    assert!(app.sessions.answer("needle").is_none());
}

#[test]
fn a_pane_of_a_workspace_being_removed_is_left_out() {
    let (mut app, _rx, _dirs) = app_with(2);
    let all = app.build_sources(false).len();
    assert!(all >= 2);

    app.projects[0].workspaces[0].start_removing();

    assert_eq!(app.build_sources(false).len(), all - 1);
}

#[test]
fn closing_the_search_drops_what_it_held() {
    let (mut app, rx, _dirs) = with_marker();
    open_sessions(&mut app);
    type_text(&mut app, MARKER);
    answered(&mut app, &rx, MARKER);
    assert!(!app.sessions.idle());

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.refresh(Instant::now());

    assert!(app.sessions.idle());
}

#[test]
fn nothing_is_searched_in_names_mode() {
    let (mut app, _rx, _dirs) = with_marker();
    click(&mut app, areas().search.as_position());
    type_text(&mut app, MARKER);

    assert!(app.sessions.idle());
}
