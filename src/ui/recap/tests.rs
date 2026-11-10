use std::collections::{BTreeMap, BTreeSet};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::recap::text::parse;
use crate::recap::{Named, Place};

fn places() -> BTreeMap<String, Place> {
    let mut places = BTreeMap::new();
    places.insert(
        "c1".to_string(),
        Place {
            kind: "conversation".into(),
            project: Some(Named { label: "api".into(), path: "/p".into() }),
            workspace: Some(Named { label: "rate-limit".into(), path: "/p/w".into() }),
            tab: Some("claude".into()),
            ..Place::default()
        },
    );
    places
}

fn view(state: State) -> View {
    let text = "## On you\n- [c1]: asked \"Redis or in memory?\"\n- plain bullet that is long enough to wrap around the panel width";
    View {
        choice: 1,
        line: "yesterday · sat 31 oct · Claude Code · 1 conversation".into(),
        state,
        blocks: parse(text, &places()),
        scroll: 0,
        muted: Color::DarkGray,
        busy: false,
        changed: BTreeSet::new(),
        outdated: false,
    }
}

const AREA: Rect = Rect { x: 0, y: 0, width: 40, height: 12 };

fn drawn(view: &View, hover: Option<Position>) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).expect("terminal");
    terminal.draw(|f| draw(f, AREA, view, hover)).expect("draw");
    terminal
}

fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..40).map(|x| buffer[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string()
}

#[test]
fn the_header_names_the_three_choices_with_refresh_and_close() {
    let terminal = drawn(&view(State::Idle), None);
    let header = row(&terminal, 0);
    assert!(header.starts_with(" Recap   now · yesterday · 7 days"), "{header}");
    assert!(header.ends_with("↻  ×"), "{header}");
    let status = row(&terminal, 1);
    assert!(status.starts_with(" yesterday · sat 31 oct · Claude Code") && status.ends_with('…'), "{status}");
}

#[test]
fn the_body_wraps_and_marks_bullets_and_links() {
    let terminal = drawn(&view(State::Idle), None);
    assert_eq!(row(&terminal, 3), " On you");
    assert_eq!(row(&terminal, 4), " ▸ api › rate-limit › claude: asked");
    assert!(row(&terminal, 5).starts_with("   \"Redis or in memory?\""), "{}", row(&terminal, 5));
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(5, 4)].fg, Color::Cyan);
    assert_eq!(buffer[(4, 3)].fg, Color::Reset);
}

#[test]
fn the_current_choice_is_cyan_and_bold() {
    let terminal = drawn(&view(State::Idle), None);
    let rects = choices(AREA);
    let cell = &terminal.backend().buffer()[(rects[1].x, 0)];
    assert_eq!(cell.fg, Color::Cyan);
    assert!(cell.modifier.contains(Modifier::BOLD));
    assert_eq!(terminal.backend().buffer()[(rects[0].x, 0)].fg, Color::DarkGray);
}

#[test]
fn a_failure_shows_its_reason_in_red_with_a_retry_button() {
    let state = State::Failed("Not logged in".into());
    let terminal = drawn(&view(state.clone()), None);
    assert_eq!(row(&terminal, 1), " Not logged in  retry");
    assert_eq!(terminal.backend().buffer()[(2, 1)].fg, Color::Red);
    let shown = view(state);
    let at = retry(AREA, "Not logged in");
    assert_eq!(hit(AREA, &shown, Position::new(at.x, at.y)), Some(Hit::Retry));
}

#[test]
fn running_shows_its_progress_and_empty_its_sentence() {
    let running = drawn(&view(State::Running("reading your conversations… 3 steps".into())), None);
    assert!(row(&running, 1).contains("reading your conversations… 3 steps"));
    let empty = drawn(
        &View { blocks: Vec::new(), ..view(State::Empty("No agent activity in the last 30 days.".into())) },
        None,
    );
    assert_eq!(row(&empty, 3), " No agent activity in the last 30 days.");
}

#[test]
fn hits_find_the_buttons_the_choices_and_the_links() {
    let shown = view(State::Idle);
    let at = |rect: Rect| Position::new(rect.x, rect.y);
    assert_eq!(hit(AREA, &shown, at(close(AREA))), Some(Hit::Close));
    assert_eq!(hit(AREA, &shown, at(refresh(AREA))), Some(Hit::Refresh));
    assert_eq!(hit(AREA, &shown, at(choices(AREA)[2])), Some(Hit::Choice(2)));
    assert_eq!(hit(AREA, &shown, Position::new(8, 4)), Some(Hit::Link("c1".into())));
    assert_eq!(hit(AREA, &shown, Position::new(3, 3)), None);
}

#[test]
fn a_hovered_link_is_underlined() {
    let terminal = drawn(&view(State::Idle), Some(Position::new(4, 4)));
    assert!(terminal.backend().buffer()[(4, 4)].modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn the_wheel_scrolls_the_body_and_the_rest_is_counted() {
    let mut shown = view(State::Idle);
    let all = total(AREA, &shown);
    assert!(all > 4);
    shown.scroll = 1;
    let terminal = drawn(&shown, None);
    assert_eq!(row(&terminal, 3), " ▸ api › rate-limit › claude: asked");
}

#[test]
fn a_status_line_longer_than_the_panel_is_cut_with_an_ellipsis_and_retry_stays_whole() {
    let long = "x".repeat(80);
    let terminal = drawn(&View { line: long.clone(), ..view(State::Idle) }, None);
    assert!(row(&terminal, 1).ends_with('…'), "{}", row(&terminal, 1));
    let failed = drawn(&view(State::Failed(long)), None);
    let line = row(&failed, 1);
    assert!(line.contains('…') && line.ends_with("retry"), "{line}");
}

#[test]
fn a_link_that_changed_is_struck_through_and_muted() {
    let mut changed = view(State::Idle);
    changed.changed = BTreeSet::from(["c1".to_string()]);
    let terminal = drawn(&changed, None);
    let cell = &terminal.backend().buffer()[(5, 4)];
    assert!(cell.modifier.contains(Modifier::CROSSED_OUT));
    assert_eq!(cell.fg, Color::DarkGray);
    let hovered = drawn(&changed, Some(Position::new(5, 4)));
    assert!(hovered.backend().buffer()[(5, 4)].modifier.contains(Modifier::UNDERLINED));
    let plain = drawn(&view(State::Idle), None);
    assert_eq!(plain.backend().buffer()[(5, 4)].fg, Color::Cyan);
}

#[test]
fn an_outdated_recap_says_so_first_on_its_status_line() {
    let mut outdated = view(State::Idle);
    outdated.outdated = true;
    let terminal = drawn(&outdated, None);
    assert!(row(&terminal, 1).starts_with(" outdated · yesterday"), "{}", row(&terminal, 1));
    assert_eq!(terminal.backend().buffer()[(1, 1)].fg, Color::Yellow);
}
