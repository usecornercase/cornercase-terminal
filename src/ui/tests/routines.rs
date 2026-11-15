use ratatui::style::Color;

use super::*;
use crate::bots::routines::{Button, Hit};

const BIG: (u16, u16) = (120, 30);

const SMALL: (u16, u16) = (70, 20);

fn rows() -> Vec<RoutineRow> {
    vec![
        RoutineRow {
            name: "Review PRs".into(),
            when: "↻ weekday 09:00 · next Mon 09:00".into(),
            state: Some("answered".into()),
            colour: Some(Color::Green),
            at: "Fri 14 Nov 09:00".into(),
            paused: false,
            buttons: vec![Button::RunNow, Button::Pause, Button::Edit, Button::Remove],
        },
        RoutineRow {
            name: "Digest".into(),
            when: "↻ day 18:00 · paused".into(),
            state: Some("failed".into()),
            colour: Some(Color::Red),
            at: "Thu 13 Nov 18:00".into(),
            paused: true,
            buttons: vec![Button::RunNow, Button::Resume, Button::Edit, Button::Remove],
        },
        RoutineRow {
            name: "Check #482".into(),
            when: "once · Sun 16 Nov 09:00".into(),
            state: None,
            colour: None,
            at: String::new(),
            paused: false,
            buttons: vec![Button::RunNow, Button::Edit, Button::Remove],
        },
    ]
}

fn routines(rows: Vec<RoutineRow>, compact: bool) -> BotRoutines {
    BotRoutines {
        title: "Routines › prs".into(),
        selected: (!rows.is_empty()).then_some(0),
        rows,
        scroll: 0,
        hint: "enter shows its last run in the chat".into(),
        empty: "prs has no routines yet. + routine schedules one, or ask prs in its chat.".into(),
        compact,
    }
}

fn shown(r: BotRoutines) -> View<'static> {
    View { overlay: Some(Overlay::BotRoutines(Box::new(r))), ..view(&["web-shop"]) }
}

fn drawn(r: BotRoutines, (width, height): (u16, u16)) -> Terminal<TestBackend> {
    render_sized(&shown(r), width, height)
}

fn screen((width, height): (u16, u16)) -> Rect {
    Rect::new(0, 0, width, height)
}

fn find(t: &Terminal<TestBackend>, text: &str) -> Position {
    let buffer = t.backend().buffer();
    let area = buffer.area;
    for y in 0..area.height {
        let line = row_text(t, Rect::new(0, y, area.width, 1));
        if let Some(at) = line.find(text) {
            return Position::new(u16::try_from(line[..at].chars().count()).expect("small"), y);
        }
    }
    panic!("no {text}");
}

#[test]
fn the_dialog_lists_routines_and_reminders_with_how_their_last_run_ended() {
    let t = drawn(routines(rows(), false), BIG);
    insta::assert_snapshot!(t.backend());
    let buffer = t.backend().buffer();
    assert_eq!(buffer[find(&t, "answered")].fg, Color::Green);
    assert_eq!(buffer[find(&t, "failed")].fg, Color::Red);
    let text = format!("{}", t.backend());
    assert_eq!(text.matches("run now").count(), 1, "only the selected row has buttons: {text}");
}

#[test]
fn the_empty_dialog_says_how_to_add_one() {
    let t = drawn(routines(Vec::new(), false), BIG);
    insta::assert_snapshot!(t.backend());
}

#[test]
fn compact_fills_the_screen_and_shows_every_rows_buttons() {
    let t = drawn(routines(rows(), true), SMALL);
    insta::assert_snapshot!(t.backend());
    let text = format!("{}", t.backend());
    assert_eq!(text.matches("run now").count(), 3, "{text}");
    assert_eq!(overlay_area(&shown(routines(rows(), true)), SMALL), screen(SMALL));
}

fn overlay_area(view: &View, size: (u16, u16)) -> Rect {
    view.overlay.as_ref().and_then(|overlay| overlay.area(screen(size))).expect("a dialog")
}

#[test]
fn a_click_lands_on_its_row_its_button_or_the_bottom_buttons() {
    let r = routines(rows(), false);
    let area = screen(BIG);
    let (room, list) = routines_room(area, false);
    assert_eq!(room, usize::from(list.height / 2));
    let pause = routine_button(area, &r, 0, Button::Pause);
    assert!(!pause.is_empty());
    assert_eq!(routines_hit(area, &r, pause.as_position()), Some(Hit::Button(0, Button::Pause)));
    let resume = routine_button(area, &r, 1, Button::Resume);
    assert_eq!(routines_hit(area, &r, resume.as_position()), Some(Hit::Button(1, Button::Resume)));
    assert!(routine_button(area, &r, 1, Button::Pause).is_empty());
    assert_eq!(routines_hit(area, &r, Position::new(list.x + 2, list.y)), Some(Hit::Row(0)));
    assert_eq!(routines_hit(area, &r, Position::new(list.x + 2, list.y + 3)), Some(Hit::Row(1)));
    assert_eq!(routines_hit(area, &r, Position::new(list.x + 2, list.y + 4)), Some(Hit::Row(2)));
    let t = drawn(r, BIG);
    let new = find(&t, "+ routine");
    let done = find(&t, "done");
    let r = routines(rows(), false);
    assert_eq!(routines_hit(area, &r, Position::new(new.x + 1, new.y)), Some(Hit::New));
    assert_eq!(routines_hit(area, &r, Position::new(done.x + 1, done.y)), Some(Hit::Done));
}

#[test]
fn the_dialog_is_drawn_where_its_area_says() {
    let view = shown(routines(rows(), false));
    assert_eq!(corners_of_its_area(&view), (String::from("╭"), String::from("╯")));
}
