use super::commands::Followed;
use super::*;
use cornercase::control::PaneRows;

fn size_of(app: &Harness, pane: &str, when: &str) -> String {
    let marker = format!("size-{when}");
    let command = format!("echo {marker}-$(stty size | tr ' ' x)");
    app.session.says(&["send", "--pane", pane, "--enter", &command]);
    let seen =
        app.session.says(&["wait", "--pane", pane, "--text", &format!("^{marker}-[0-9]+x[0-9]+$"), "--timeout", "10"]);
    seen.rsplit('-').next().unwrap_or_default().to_string()
}

#[test]
fn a_view_sizes_its_tab_and_moves_without_the_window_and_gives_both_back_when_it_ends() {
    let app = Harness::start();
    let pane = app.session.says(&["new-tab"]);
    let window = app.session.report().shown;
    let rows: PaneRows = serde_json::from_str(&app.session.says(&["status", "--panes", "--json"])).expect("rows");
    let tab = rows.panes.iter().find(|row| row.pane.to_string() == pane).expect("the new pane").tab.to_string();
    let before = size_of(&app, &pane, "before");

    let mut opened = app.session.spawn(&["view", "--cols", "100", "--rows", "30", "--tab", &tab]);
    let mut followed = Followed::of(&mut opened);
    followed.until("the view's first line", |line| line["view"]["controls"] == true && line["revision"] == 1);
    let id = followed.seen[0]["view"]["id"].as_u64().expect("an id").to_string();
    let sized = size_of(&app, &pane, "during");
    let moved = app.session.says(&["new-tab", "--view", &id, "--focus"]);
    followed.until("the view moves", |line| line["view"]["projection"] == 2);
    let inside = app.session.says(&["status", "--view", &id]);
    let unchanged = app.session.report().shown == window;
    let mut streaming = app.session.spawn(&["surface", "--view", &id]);
    let mut frames = Followed::of(&mut streaming);
    frames.until("a frame of the view's tab", |frame| frame["view"].as_u64() == id.parse().ok());

    let _ = opened.kill();
    let _ = opened.wait();
    app.session
        .wait_for_report("the view closes", |_| app.session.cli(&["status", "--view", &id]).status.code() != Some(0));
    let after = size_of(&app, &pane, "after");

    assert_eq!(sized, "30x100");
    assert!(unchanged, "the window kept showing what it showed");
    assert!(inside.contains("(view)"), "{inside}");
    assert_ne!(moved, "");
    assert_ne!(before, "30x100");
    assert_eq!(after, before);
    assert_eq!(finish_within(streaming, TIMEOUT).status.code(), Some(0));
}
