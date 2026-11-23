use ratatui::style::Color;

use super::*;

const TONE: ui::Tone = ui::Tone { muted: Color::DarkGray, surface: Color::Black };
const WIDTH: u16 = 60;

fn facts() -> Facts {
    Facts {
        name: String::from("PRs"),
        held: Vec::new(),
        zone: String::from("UTC"),
        now: 0,
        retry: None,
        answering: HashSet::new(),
        again: HashSet::new(),
        panes: HashSet::new(),
        boot: String::new(),
        asks: HashSet::new(),
    }
}

fn chat() -> Chat {
    let lines = vec![
        Line::notice(1, 0, "↻ PRs · 09:00").of_run(7),
        Line::you(2, 0, "which are blocked?", false),
        Line::notice(3, 0, "new conversation"),
        Line::you(4, 0, "and now?", false),
    ];
    Chat::new(String::from("prs"), lines)
}

#[test]
fn a_line_that_joins_a_run_above_grows_only_that_block() {
    let mut chat = chat();
    let growth = chat.took(&Line::failed(5, 0, "the run failed").of_run(7), Some((&facts(), WIDTH, TONE)));
    assert!(growth.height > 0 && growth.below > 0 && growth.top > growth.below, "{growth:?}");
    assert_eq!(growth.moved(0), Some(0));
    assert_eq!(growth.moved(growth.below - 1), Some(growth.below - 1));
    assert_eq!(growth.moved(growth.below), None);
    assert_eq!(growth.moved(growth.top), Some(growth.top + growth.height));
}

#[test]
fn a_line_at_the_end_moves_every_row_up() {
    let mut chat = chat();
    let growth = chat.took(&Line::notice(5, 0, "stopped"), Some((&facts(), WIDTH, TONE)));
    assert_eq!((growth.below, growth.top), (0, 0));
    assert_eq!(growth.moved(0), Some(growth.height));
    assert!(growth.height > 0);
}

#[test]
fn a_line_nobody_measures_moves_nothing() {
    let mut chat = chat();
    let growth = chat.took(&Line::notice(5, 0, "stopped"), None);
    assert_eq!(growth, Growth::default());
    assert_eq!(chat.lines.len(), 5);
}
