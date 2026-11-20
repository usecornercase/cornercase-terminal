use crossterm::event::Event;

use super::*;
use crate::queue::form::Control;

fn new_form(app: &mut App) {
    open_queue(app);
    press_button(app, "new");
    assert!(matches!(app.overlay, Some(Overlay::Queue(_))));
}

fn prompt(app: &App) -> String {
    let Some(Overlay::Queue(form)) = &app.overlay else { panic!("the form") };
    form.prompt.text()
}

fn field(app: &App) -> Rect {
    let Some(Overlay::Queue(form)) = &app.overlay else { panic!("the form") };
    ui::queue_form::text(AREA, form, Control::Prompt)
}

fn top(app: &App) -> usize {
    let Some(Overlay::Queue(form)) = &app.overlay else { panic!("the form") };
    ui::queue_form::top(AREA, form)
}

#[test]
fn enter_in_the_prompt_adds_a_line_and_does_not_save() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    new_form(&mut app);
    type_text(&mut app, "first");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    type_text(&mut app, "second");
    assert!(matches!(app.overlay, Some(Overlay::Queue(_))));
    assert_eq!(prompt(&app), "first\nsecond");
    assert_eq!(app.panels.queued.queue().entries, []);
    save_queue_form(&mut app);
    assert_eq!(app.panels.queued.queue().entries[0].task.prompt, "first\nsecond");
}

#[test]
fn a_paste_keeps_its_line_breaks() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    new_form(&mut app);
    app.handle_event(AppEvent::Input(Event::Paste("one\r\ntwo\nthree".into())), AREA).expect("paste");
    assert_eq!(prompt(&app), "one\ntwo\nthree");
}

#[test]
fn the_wheel_scrolls_the_prompt_and_a_click_lands_on_the_line_shown() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    new_form(&mut app);
    let text: Vec<String> = (1..=12).map(|n| format!("line {n}")).collect();
    let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("the form") };
    form.prompt = Editor::new(&text.join("\n"));
    let inside = field(&app).as_position();
    mouse(&mut app, MouseEventKind::ScrollDown, inside);
    let shown = top(&app);
    assert!(shown > 0);
    click(&mut app, inside);
    let width = usize::from(field(&app).width);
    let Some(Overlay::Queue(form)) = &app.overlay else { panic!("the form") };
    assert_eq!(form.prompt.position(width), (shown, 0));
}

#[test]
fn editing_a_long_prompt_opens_with_its_cursor_in_view() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    let long = (1..=12).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
    let id = app.panels.queued.schedule(None, Task { prompt: long, ..task(&app, time::now() + 3600, None) });
    app.new_queue_form(Some(id.expect("an entry")));
    let rows = usize::from(field(&app).height);
    assert_eq!(top(&app), 12 - rows);
}
