use super::*;
use crate::control::Matches;

fn search(query: &str) -> Command {
    Command::Search(wire::Search { query: query.into(), ..wire::Search::default() })
}

fn matches(response: Response) -> Matches {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("matches"),
        Response::Error(message) => panic!("the request failed: {message}"),
    }
}

#[test]
fn a_search_answers_with_the_ids_of_the_pane_that_printed_the_text() {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "echo needle-\"\"found");
    pump_until(&mut app, &rx, "the marker shows", |a| pane_screen(a, id).contains("needle-found"));

    ask(&mut app, None, search("NEEDLE-FOUND"));
    let found = matches(answered(&mut app, &rx, "the search answers"));

    let hit = found.matches.first().expect("a match");
    assert_eq!((hit.pane, hit.source.as_str()), (id, "screen"));
    assert!(hit.text.ends_with("needle-found"), "{}", hit.text);
    assert!(hit.line.is_some() && hit.place.contains(" › ") && !hit.who.is_empty());
    assert_eq!((hit.project != 0, hit.workspace != 0, hit.tab != 0), (true, true, true));
    assert!(!found.capped);
}

#[test]
fn a_search_without_a_match_answers_an_empty_list() {
    let (mut app, rx) = app();

    ask(&mut app, None, search("zzz-not-anywhere"));
    let found = matches(answered(&mut app, &rx, "the search answers"));

    assert_eq!(found, Matches::default());
}

#[test]
fn a_search_needs_text() {
    let (mut app, _rx) = app();

    let message = error(now(&mut app, None, search("   ")));

    assert!(message.contains("some text"), "{message}");
}

#[test]
fn a_search_goes_with_its_client() {
    let (mut app, rx) = app();
    ask(&mut app, None, search("anything"));

    app.forget(CLIENT);
    let mut ended = false;
    wait_until("the job answers", || {
        while let Ok(event) = rx.try_recv() {
            ended |= matches!(event, AppEvent::Search(_));
            app.handle_event(event, AREA).expect("handle event");
        }
        ended
    });

    assert_eq!(app.take_answers(), Vec::new());
}

fn tab_that_printed_a_marker(app: &mut App, rx: &Receiver<AppEvent>) -> u64 {
    ask(app, None, new_tab(Some("echo needle-\"\"found; seq 1 60; echo all-\"\"done")));
    let id = done(answered(app, rx, "the command is typed")).ids.pane.expect("the new pane");
    pump_until(app, rx, "the output ends", |a| pane_screen(a, id).contains("all-done"));
    id
}

fn line_of_the_marker(app: &mut App, rx: &Receiver<AppEvent>) -> usize {
    ask(app, None, search("needle-found"));
    let found = matches(answered(app, rx, "the search answers"));
    found.matches.first().and_then(|hit| hit.line).expect("a screen line")
}

#[test]
fn a_reveal_scrolls_the_pane_to_the_match_and_selects_it() {
    let (mut app, rx) = app();
    let was = first(&app);
    let id = tab_that_printed_a_marker(&mut app, &rx);
    let line = line_of_the_marker(&mut app, &rx);

    let args = json!({"item": {"pane": id}, "reveal": {"query": "needle-found", "line": line}});
    let answer = ok(&mut app, "focus", &args);

    assert_eq!(answer, json!({"revealed": true}));
    assert_ne!(first(&app), was);
    assert_eq!(first(&app), id);
    let emulator = &mut app.term_mut().expect("the pane").emulator;
    assert!(emulator.below() > 0);
    assert_eq!(emulator.finish_selection().expect("selection"), Some("needle-found".to_string()));
}

#[test]
fn a_reveal_of_text_the_pane_no_longer_holds_only_shows_it() {
    let (mut app, rx) = app();
    let id = tab_that_printed_a_marker(&mut app, &rx);

    let args = json!({"item": {"pane": id}, "reveal": {"query": "no-such-text-anywhere"}});
    let answer = ok(&mut app, "focus", &args);

    assert_eq!(answer, json!({"revealed": false}));
    assert_eq!(first(&app), id);
    let emulator = &mut app.term_mut().expect("the pane").emulator;
    assert_eq!(emulator.selection_text().expect("selection"), None);
}

#[test]
fn a_reveal_that_misses_clears_the_selection_an_earlier_one_left() {
    let (mut app, rx) = app();
    let id = tab_that_printed_a_marker(&mut app, &rx);
    let hit = json!({"item": {"pane": id}, "reveal": {"query": "needle-found"}});
    assert_eq!(ok(&mut app, "focus", &hit), json!({"revealed": true}));

    let miss = json!({"item": {"pane": id}, "reveal": {"query": "no-such-text-anywhere"}});

    assert_eq!(ok(&mut app, "focus", &miss), json!({"revealed": false}));
    let emulator = &mut app.term_mut().expect("the pane").emulator;
    assert_eq!(emulator.selection_text().expect("selection"), None);
}

#[rstest::rstest]
#[case::a_tab(json!({"item": {"tab": 1}, "reveal": {"query": "x"}}))]
#[case::blank_text(json!({"item": {"pane": 1}, "reveal": {"query": "  "}}))]
fn a_reveal_needs_a_pane_and_some_text(#[case] args: Value) {
    let (mut app, _rx) = app();
    let was = app.focus();

    let refusal = refused(&mut app, "focus", &args);

    assert_eq!((refusal.code.as_str(), app.focus()), ("malformed", was));
}

#[test]
fn a_focus_without_reveal_answers_as_before() {
    let (mut app, _rx) = app();
    let id = first(&app);

    assert_eq!(ok(&mut app, "focus", &json!({"item": {"pane": id}})), json!({}));
}

mod settings {
    use super::*;

    fn with_settings(query: &str, settings: Option<bool>) -> Command {
        Command::Search(wire::Search { query: query.into(), settings })
    }

    #[test]
    fn asked_for_they_come_in_their_own_list_after_the_matches() {
        let (mut app, rx) = app();

        ask(&mut app, None, with_settings("prefix key", Some(true)));
        let found = matches(answered(&mut app, &rx, "the search answers"));

        assert!(found.settings.iter().any(|setting| setting.title == "UI › prefix key"), "{found:?}");
    }

    #[test]
    fn a_hidden_row_says_so() {
        let (mut app, rx) = app();

        ask(&mut app, None, with_settings("closed workspaces", Some(true)));
        let found = matches(answered(&mut app, &rx, "the search answers"));

        assert!(found.settings.iter().any(|setting| setting.hidden), "{found:?}");
    }

    #[rstest::rstest]
    #[case::not_named(None)]
    #[case::turned_off(Some(false))]
    fn otherwise_a_search_lists_none(#[case] settings: Option<bool>) {
        let (mut app, rx) = app();

        ask(&mut app, None, with_settings("prefix key", settings));
        let Response::Ok(value) = answered(&mut app, &rx, "the search answers") else { panic!("an answer") };

        assert_eq!(value.get("settings"), None);
    }
}
