use super::card_menu::assert_a_card_click_opens_its_routines;
use super::routines::{START, answered_run, bot_routine, scheduled};
use super::*;
use crate::bots::routines::{Button, Hit};
use crate::queue::form::{Control, When};
use crate::queue::{State, Task};

fn show(c: &mut Chat) {
    c.app.window_attached();
    let row = bot_row(&mut c.app, 0);
    render(&mut c.app, AREA);
    click(&mut c.app, Position::new(row.x + 3, row.y));
}

fn reminder(at: i64) -> Task {
    Task { label: "Check #482".into(), repeat: None, ..bot_routine(at, "2h") }
}

fn later() -> i64 {
    crate::time::now() + 86_400
}

fn view(c: &Chat) -> ui::BotRoutines {
    let Some(overlay) = &c.app.overlay else { panic!("no dialog") };
    let Some(ui::Overlay::BotRoutines(view)) = c.app.overlay_view(overlay, AREA) else { panic!("not the dialog") };
    *view
}

fn opened(c: &mut Chat) {
    let key = c.app.panels.bots.live[0].key;
    c.app.open_bot_routines(key, None);
    render(&mut c.app, AREA);
}

pub(super) fn spot_in(view: &ui::BotRoutines, wanted: Hit) -> Position {
    (0..AREA.height)
        .flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)))
        .find(|pos| ui::routines_hit(AREA, view, *pos) == Some(wanted))
        .unwrap_or_else(|| panic!("nothing at {wanted:?}"))
}

pub(super) fn press_in(c: &mut Chat, view: &ui::BotRoutines, (row, button): (usize, Button)) {
    let rect = ui::routine_button(AREA, view, row, button);
    assert!(!rect.is_empty(), "no {button:?} on row {row}");
    click(&mut c.app, rect.as_position());
}

fn spot(c: &Chat, wanted: Hit) -> Position {
    spot_in(&view(c), wanted)
}

fn press_button(c: &mut Chat, row: usize, button: Button) {
    let view = view(c);
    press_in(c, &view, (row, button));
}

fn names(view: &ui::BotRoutines) -> Vec<&str> {
    view.rows.iter().map(|row| row.name.as_str()).collect()
}

#[test]
fn the_routines_button_opens_the_dialog_with_the_bots_routines_and_reminders() {
    let mut c = chat(ANSWER);
    show(&mut c);
    scheduled(&mut c, bot_routine(later(), "2h"));
    scheduled(&mut c, reminder(crate::time::now() + 3_600));
    let wide = Rect::new(0, 0, 160, 30);
    render(&mut c.app, wide);
    let button = c.app.chat_geometry(wide).routines;
    assert!(!button.is_empty());
    click_in(&mut c.app, button.as_position(), wide);
    assert!(matches!(c.app.overlay, Some(Overlay::BotRoutines(_))));
    let view = view(&c);
    assert_eq!(view.title, "Routines › PRs");
    assert_eq!(names(&view), ["Check #482", "Review PRs"]);
    assert!(view.rows[0].when.starts_with("once · "), "{}", view.rows[0].when);
    assert!(view.rows[1].when.starts_with("↻ 2h · next "), "{}", view.rows[1].when);
    assert!(view.rows.iter().all(|row| row.state.is_none()));
}

#[test]
fn routines_in_the_menu_and_a_routine_on_the_card_open_it_too() {
    let mut c = chat(ANSWER);
    show(&mut c);
    scheduled(&mut c, reminder(crate::time::now() + 3_600));
    let id = scheduled(&mut c, bot_routine(later(), "2h"));
    render(&mut c.app, AREA);
    let row = bot_row(&mut c.app, 0);
    right_click(&mut c.app, Position::new(row.x + 3, row.y));
    pick(&mut c.app, "routines…");
    assert!(matches!(&c.app.overlay, Some(Overlay::BotRoutines(dialog)) if dialog.list.selected.is_some()));
    c.app.overlay = None;
    render(&mut c.app, AREA);
    assert_a_card_click_opens_its_routines(&mut c, id);
}

#[test]
fn a_click_on_a_routine_whose_last_run_answered_shows_the_chat_at_that_run() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    let (_, run) = answered_run(&mut c);
    opened(&mut c);
    let view = view(&c);
    assert_eq!(view.rows[0].state.as_deref(), Some("answered"));
    let row = spot(&c, Hit::Row(0));
    click(&mut c.app, row);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.shown_bot(), Some(0));
    let anchor = c.app.panels.bots.shown.as_ref().and_then(|shown| shown.anchor);
    let first = c.lines().iter().find(|line| line.routine == Some(run)).map(|line| line.id);
    assert!(first.is_some());
    assert_eq!(anchor, first);
}

#[test]
fn enter_on_a_routine_never_run_or_missed_keeps_the_dialog() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(START, "2h"));
    opened(&mut c);
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(c.app.overlay, Some(Overlay::BotRoutines(_))));
    assert!(view(&c).hint.starts_with("each run is a new conversation of PRs"));
    c.fire(START + 7 * 86_400 + 3_600);
    assert!(c.app.panels.queued.queue().runs_of(id).all(|run| run.state == State::Missed));
    send_key(&mut c.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(c.app.overlay, Some(Overlay::BotRoutines(_))));
    let view = view(&c);
    assert!(view.hint.starts_with("missed its run at "), "{}", view.hint);
    assert_eq!(view.rows[0].state.as_deref(), Some("missed"));
}

#[test]
fn pause_and_resume_from_the_row_change_the_routine_and_its_row() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(later(), "2h"));
    opened(&mut c);
    press_button(&mut c, 0, Button::Pause);
    assert!(c.app.panels.queued.queue().entry(id).expect("routine").task.paused);
    let row = &view(&c).rows[0];
    assert!(row.paused && row.when.ends_with("paused"), "{}", row.when);
    assert!(row.buttons.contains(&Button::Resume) && !row.buttons.contains(&Button::Pause));
    press_button(&mut c, 0, Button::Resume);
    assert!(!c.app.panels.queued.queue().entry(id).expect("routine").task.paused);
    assert!(view(&c).rows[0].when.contains("next "));
}

#[test]
fn run_now_from_the_row_answers_in_the_chat() {
    let mut c = chat(ANSWER);
    let id = scheduled(&mut c, bot_routine(later(), "2h"));
    opened(&mut c);
    press_button(&mut c, 0, Button::RunNow);
    c.runs_settle();
    let runs: Vec<_> = c.app.panels.queued.queue().runs_of(id).collect();
    assert!(matches!(runs[..], [run] if matches!(run.state, State::Answered { .. })), "{runs:?}");
    assert_eq!(view(&c).rows[0].state.as_deref(), Some("answered"));
}

#[test]
fn remove_from_the_row_takes_the_routine_and_its_runs_with_an_undo() {
    let mut c = chat(ANSWER);
    let (id, _) = answered_run(&mut c);
    opened(&mut c);
    let before = c.app.panels.queued.queue().entries.clone();
    press_button(&mut c, 0, Button::Remove);
    assert_eq!(c.app.panels.queued.queue().entries, []);
    assert!(view(&c).rows.is_empty());
    let toast = c.app.messages.toast.as_ref().expect("a toast");
    assert_eq!(toast.message, format!("removed routine {id} and its 1 run"));
    let undo = ui::toast_button(AREA, toast.view());
    click(&mut c.app, undo.as_position());
    assert_eq!(c.app.panels.queued.queue().entries, before);
    assert_eq!(names(&view(&c)), ["Review PRs"]);
}

#[test]
fn edit_opens_the_form_with_where_fixed_to_the_bot_and_esc_comes_back() {
    let mut c = chat(ANSWER);
    scheduled(&mut c, bot_routine(later(), "2h"));
    opened(&mut c);
    press_button(&mut c, 0, Button::Edit);
    let Some(Overlay::Queue(form)) = &c.app.overlay else { panic!("the form") };
    assert_eq!(form.places.iter().map(|choice| choice.label.as_str()).collect::<Vec<_>>(), ["bot: PRs"]);
    assert!(!form.controls().contains(&Control::Place));
    let drawn = format!("{}", render(&mut c.app, AREA).backend());
    assert!(drawn.contains("bot: PRs") && !drawn.contains("bot: PRs ▾"), "{drawn}");
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(c.app.overlay, Some(Overlay::BotRoutines(_))));
}

#[test]
fn plus_routine_opens_a_daily_form_for_the_bot_and_saving_lists_it() {
    let mut c = chat(ANSWER);
    opened(&mut c);
    assert!(view(&c).empty.starts_with("PRs has no routines yet."));
    let new = spot(&c, Hit::New);
    click(&mut c.app, new);
    let Some(Overlay::Queue(form)) = &c.app.overlay else { panic!("the form") };
    assert_eq!(form.kind(), When::Daily);
    assert_eq!(form.bot(), Some("prs"));
    for ch in "check".chars() {
        send_key(&mut c.app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
    save_queue_form(&mut c.app);
    assert!(matches!(c.app.overlay, Some(Overlay::BotRoutines(_))), "{:?}", c.app.overlay);
    let view = view(&c);
    assert_eq!(view.rows.len(), 1);
    assert!(view.rows[0].when.starts_with("↻ day 09:00"), "{}", view.rows[0].when);
    let entries = &c.app.panels.queued.queue().entries;
    assert!(entries.iter().all(|entry| entry.bot() == Some("prs") && entry.task.repeat.is_some()));
}

#[test]
fn removing_the_bot_closes_its_routines_dialog_and_its_form() {
    let mut c = chat(ANSWER);
    scheduled(&mut c, bot_routine(later(), "2h"));
    opened(&mut c);
    c.app.bot_remove("prs").expect("removed");
    assert!(c.app.overlay.is_none());
    let (mut c, key) = (chat(ANSWER), 0);
    scheduled(&mut c, bot_routine(later(), "2h"));
    opened(&mut c);
    press_button(&mut c, key, Button::Edit);
    assert!(matches!(c.app.overlay, Some(Overlay::Queue(_))));
    c.app.bot_remove("prs").expect("removed");
    assert!(c.app.overlay.is_none());
    assert!(c.app.panels.queued.take_back().is_none());
}

#[test]
fn opening_on_a_late_routine_scrolls_it_into_view() {
    let mut c = chat(ANSWER);
    let ids: Vec<u64> = (0..14).map(|i| scheduled(&mut c, bot_routine(later() + i * 3_600, "2h"))).collect();
    let key = c.app.panels.bots.live[0].key;
    c.app.open_bot_routines(key, ids.last().copied());
    render(&mut c.app, AREA);
    let shown = view(&c);
    assert_eq!(shown.selected, Some(13));
    assert!(shown.scroll > 0);
    assert!(!ui::routine_button(AREA, &shown, 13, Button::Remove).is_empty());
    send_key(&mut c.app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(view(&c).selected, Some(12));
    assert!(!ui::routine_button(AREA, &view(&c), 12, Button::Remove).is_empty());
}

#[test]
fn the_wheel_scrolls_past_the_first_page_with_the_first_row_selected() {
    let mut c = chat(ANSWER);
    for i in 0..14 {
        scheduled(&mut c, bot_routine(later() + i * 3_600, "2h"));
    }
    opened(&mut c);
    let list = ui::routines_room(AREA, false).1;
    let over = Position::new(list.x + 4, list.y + 1);
    for _ in 0..4 {
        mouse(&mut c.app, MouseEventKind::ScrollDown, over);
    }
    let shown = view(&c);
    assert_eq!(shown.selected, Some(0));
    assert!(shown.scroll > 0, "the list moved");
    assert!(ui::routine_button(AREA, &shown, 0, Button::Remove).is_empty(), "the first row left the page");
    assert!(!ui::routine_button(AREA, &shown, shown.scroll, Button::Remove).is_empty());
}

#[test]
fn opening_without_a_selection_pins_the_first_routine_to_its_entry() {
    let mut c = chat(ANSWER);
    let late = scheduled(&mut c, bot_routine(later() + 7_200, "2h"));
    let early = scheduled(&mut c, bot_routine(later(), "2h"));
    let key = c.app.panels.bots.live[0].key;
    c.app.open_bot_routines(key, None);
    assert!(matches!(&c.app.overlay, Some(Overlay::BotRoutines(dialog)) if dialog.list.selected == Some(early)));
    let mut task = c.app.panels.queued.queue().entry(early).expect("early").task.clone();
    task.at = later() + 20_000;
    c.app.panels.queued.schedule(Some(early), task).expect("moved after the other");
    render(&mut c.app, AREA);
    let shown = view(&c);
    assert_eq!(shown.selected, Some(1), "the highlight followed its entry down");
    assert_eq!(shown.rows[1].name, "Review PRs");
    assert!(matches!(&c.app.overlay, Some(Overlay::BotRoutines(dialog)) if dialog.list.selected == Some(early)));
    assert!(c.app.panels.queued.queue().entry(late).is_some());
}
