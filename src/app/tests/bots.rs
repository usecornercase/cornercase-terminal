use super::*;
use crate::bots::chat::{self, Line};
use crate::bots::folder;
use crate::bots::sample as bot;

fn chat_of(dir: &Path, id: &str) -> Vec<Line> {
    chat::read(&folder::of(dir, id).join(folder::CHAT_FILE))
}

fn texts(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(|line| line.text.as_str()).collect()
}

#[test]
fn a_turn_that_never_ended_is_interrupted_and_what_was_held_is_dropped_once() {
    let dir = TempDir::new();
    folder::create(dir.path(), &bot("prs", "PRs"), "").expect("create");
    let path = folder::of(dir.path(), "prs").join(folder::CHAT_FILE);
    for line in [Line::you(1, 10, "first", false), Line::you(2, 11, "second", true)] {
        chat::append(&path, &line).expect("append");
    }
    let (mut app, _rx) = app();

    app.set_bots_dir(dir.path());
    app.set_bots_dir(dir.path());

    let lines = chat_of(dir.path(), "prs");
    assert_eq!(texts(&lines)[2..], ["interrupted by a restart", "1 held message dropped"]);
    assert_eq!(app.bots.live.len(), 1);
    assert_eq!(app.bots.live[0].next_line, 5);
    assert_eq!(app.bots.live[0].last.as_ref().map(|line| line.text.as_str()), Some("second"));
}

#[test]
fn a_finished_chat_gets_no_notice() {
    let dir = TempDir::new();
    folder::create(dir.path(), &bot("prs", "PRs"), "").expect("create");
    let path = folder::of(dir.path(), "prs").join(folder::CHAT_FILE);
    chat::append(&path, &Line::you(1, 10, "first", false)).expect("append");
    chat::append(&path, &Line::failed(2, 11, "no")).expect("append");
    let (mut app, _rx) = app();

    app.set_bots_dir(dir.path());

    assert_eq!(chat_of(dir.path(), "prs").len(), 2);
}
