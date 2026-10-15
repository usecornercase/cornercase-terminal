use super::*;
use crate::control::Matches;

fn search(query: &str) -> Command {
    Command::Search(wire::Search { query: query.into() })
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
