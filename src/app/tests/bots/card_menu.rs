use super::routines::{answered_run, bot_routine, scheduled};
use super::*;
use crate::queue::State;

fn shown(c: &mut Chat) {
    c.app.window_attached();
    c.app.show_bot(0);
    render(&mut c.app, AREA);
}

fn card_list(c: &Chat) -> Rect {
    c.app.layout(AREA).shown(c.app.screen.nav).workspaces_list
}

pub(super) fn card_rows_of(c: &Chat, routine: u64) -> Vec<Position> {
    let list = card_list(c);
    let card = c.app.bot_card(0);
    (list.y..list.bottom())
        .map(|y| Position::new(list.x + 3, y))
        .filter(|pos| ui::card_routine(list, &card, *pos) == Some(routine))
        .collect()
}

fn menu_on(c: &mut Chat, routine: u64) {
    let pos = card_rows_of(c, routine)[0];
    right_click(&mut c.app, pos);
}

fn button_of(c: &Chat, routine: u64) -> Position {
    ui::card_menu(card_list(c), &c.app.bot_card(0), routine).as_position()
}

fn hover_on(c: &mut Chat, pos: Position) {
    mouse(&mut c.app, MouseEventKind::Moved, pos);
    render(&mut c.app, AREA);
}

fn later() -> i64 {
    crate::time::now() + 86_400
}

fn routine(c: &mut Chat) -> u64 {
    let id = scheduled(c, bot_routine(later(), "2h"));
    shown(c);
    id
}

fn toast_text(c: &Chat) -> Option<String> {
    c.app.messages.toast.as_ref().map(|toast| toast.message.clone())
}

#[test]
fn a_right_click_on_either_row_of_a_routine_opens_its_menu() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    let rows = card_rows_of(&c, id);
    assert_eq!(rows.len(), 2);
    for pos in rows {
        right_click(&mut c.app, pos);
        assert_eq!(menu_labels(&c.app), ["run now", "pause", "edit…", "remove"]);
        c.app.overlay = None;
    }
    let list = card_list(&c);
    right_click(&mut c.app, Position::new(list.x + 3, list.y + 1));
    assert!(c.app.overlay.is_none());
}

#[test]
fn its_menu_button_opens_the_same_menu_and_a_click_beside_it_opens_the_dialog() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    let button = button_of(&c, id);
    click(&mut c.app, button);
    assert!(matches!(c.app.overlay, Some(Overlay::BotRoutines(_))), "a button that is not drawn is not a target");
    c.app.overlay = None;
    hover_on(&mut c, button);
    click(&mut c.app, button);
    assert_eq!(menu_labels(&c.app), ["run now", "pause", "edit…", "remove"]);
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    assert_a_card_click_opens_its_routines(&mut c, id);
}

pub(super) fn assert_a_card_click_opens_its_routines(c: &mut Chat, id: u64) {
    let pos = card_rows_of(c, id)[0];
    click(&mut c.app, pos);
    assert!(matches!(&c.app.overlay, Some(Overlay::BotRoutines(dialog)) if dialog.selected == Some(id)));
}

fn second_row_value(c: &mut Chat, id: u64) -> String {
    let card = c.app.bot_card(0);
    let list = card_list(c);
    let pos = card_rows_of(c, id)[1];
    let at = ui::card_row_at(list, &card, pos).expect("a row");
    card.rows[at].value.clone()
}

#[test]
fn pause_and_resume_from_the_card_change_the_routine_and_its_row() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    menu_on(&mut c, id);
    pick(&mut c.app, "pause");
    assert!(c.app.panels.queued.queue().entry(id).expect("entry").task.paused);
    assert_eq!(toast_text(&c), Some(format!("paused routine {id}")));
    render(&mut c.app, AREA);
    assert!(second_row_value(&mut c, id).starts_with("paused"));
    menu_on(&mut c, id);
    let labels = menu_labels(&c.app);
    assert!(labels.contains(&"resume".to_string()) && !labels.contains(&"pause".to_string()));
    pick(&mut c.app, "resume");
    assert!(!c.app.panels.queued.queue().entry(id).expect("entry").task.paused);
}

#[test]
fn remove_from_the_card_takes_the_routine_and_its_runs_with_an_undo() {
    let mut c = chat(ANSWER);
    c.app.window_attached();
    let (id, _) = answered_run(&mut c);
    shown(&mut c);
    let before = c.app.panels.queued.queue().entries.clone();
    menu_on(&mut c, id);
    pick(&mut c.app, "remove");
    assert_eq!(c.app.panels.queued.queue().entries.len(), 0);
    render(&mut c.app, AREA);
    assert_eq!(card_rows_of(&c, id).len(), 0);
    assert_eq!(toast_text(&c), Some(format!("removed routine {id} and its 1 run")));
    let toast = c.app.messages.toast.as_ref().expect("toast");
    let button = ui::toast_button(AREA, toast.view()).as_position();
    click(&mut c.app, button);
    assert_eq!(c.app.panels.queued.queue().entries, before);
}

#[test]
fn run_now_from_the_card_answers_in_the_chat() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    menu_on(&mut c, id);
    pick(&mut c.app, "run now");
    c.runs_settle();
    let runs: Vec<_> = c.app.panels.queued.queue().runs_of(id).cloned().collect();
    assert_eq!(runs.len(), 1);
    assert!(matches!(runs[0].state, State::Answered { .. }));
    assert!(c.lines().iter().any(|line| line.routine == Some(runs[0].id)));
}

#[test]
fn edit_from_the_card_opens_the_form_for_the_bot_and_esc_leaves_the_chat() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    menu_on(&mut c, id);
    pick(&mut c.app, "edit…");
    let Some(Overlay::Queue(form)) = &c.app.overlay else { panic!("no form") };
    assert_eq!(form.bot(), Some("prs"));
    send_key(&mut c.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(c.app.overlay.is_none());
    assert_eq!(c.app.shown_bot(), Some(0));
    assert!(c.app.panels.queued.take_back().is_none());
}

#[test]
fn removing_the_bot_closes_its_routines_menu() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    menu_on(&mut c, id);
    assert!(matches!(c.app.overlay, Some(Overlay::Menu { .. })));
    c.app.bot_remove("prs").expect("removed");
    assert!(c.app.overlay.is_none());
}

#[test]
fn an_item_of_a_routine_removed_meanwhile_does_nothing() {
    let mut c = chat(ANSWER);
    let a = routine(&mut c);
    let b = scheduled(&mut c, bot_routine(later() + 60, "3h"));
    render(&mut c.app, AREA);
    menu_on(&mut c, a);
    c.app.panels.queued.remove(a).expect("removed");
    pick(&mut c.app, "pause");
    assert!(c.app.messages.toast.is_none());
    render(&mut c.app, AREA);
    menu_on(&mut c, b);
    c.app.panels.queued.remove(b).expect("removed");
    pick(&mut c.app, "edit…");
    assert!(c.app.overlay.is_none());
}

#[test]
fn an_item_that_no_longer_applies_does_nothing() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    menu_on(&mut c, id);
    pick(&mut c.app, "pause");
    menu_on(&mut c, id);
    assert!(menu_labels(&c.app).contains(&"resume".to_string()));
    c.app.pause_queued(id, false);
    c.app.messages.toast = None;
    pick(&mut c.app, "resume");
    assert!(c.app.messages.toast.is_none());
}

#[test]
fn a_press_on_a_routine_removed_since_the_last_draw_opens_nothing_for_the_one_that_took_its_row() {
    let mut c = chat(ANSWER);
    let first = routine(&mut c);
    let second = scheduled(&mut c, bot_routine(later() + 60, "3h"));
    render(&mut c.app, AREA);
    let pos = card_rows_of(&c, first)[0];
    c.app.panels.queued.remove(first).expect("removed");
    right_click(&mut c.app, pos);
    assert!(c.app.overlay.is_none());
    click(&mut c.app, pos);
    assert!(c.app.overlay.is_none(), "{second} must not take the pressed row");
}

const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn a_press_opens_a_menu_somewhere_on(c: &mut Chat, row: Position, list: Rect, area: Rect) -> bool {
    (list.x + 10..list.right()).rev().any(|x| {
        click_in(&mut c.app, Position::new(x, row.y), area);
        let opened = matches!(c.app.overlay, Some(Overlay::Menu { .. }));
        c.app.overlay = None;
        opened
    })
}

#[test]
fn a_compact_card_has_its_menu_button_without_hover_and_a_wide_one_does_not() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    let wide = card_list(&c);
    let row = card_rows_of(&c, id)[0];
    assert!(!a_press_opens_a_menu_somewhere_on(&mut c, row, wide, AREA));
    c.app.screen.nav = Some(ui::Nav::Workspaces);
    render(&mut c.app, SMALL);
    let list = c.app.layout(SMALL).shown(c.app.screen.nav).workspaces_list;
    let card = c.app.bot_card(0);
    let row = (list.y..list.bottom())
        .map(|y| Position::new(list.x + 3, y))
        .find(|pos| ui::card_routine(list, &card, *pos) == Some(id))
        .expect("a routine row");
    assert!(a_press_opens_a_menu_somewhere_on(&mut c, row, list, SMALL));
}

#[test]
fn a_press_uses_the_list_of_the_frame_that_was_drawn() {
    let mut c = chat(ANSWER);
    let id = routine(&mut c);
    let pos = card_rows_of(&c, id)[0];
    c.app.screen.nav = Some(ui::Nav::Workspaces);
    click_in(&mut c.app, pos, SMALL);
    assert!(matches!(&c.app.overlay, Some(Overlay::BotRoutines(dialog)) if dialog.selected == Some(id)));
}
