use super::routines::runs;
use super::*;
use crate::queue::repeat::Repeat;

fn opened() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, repo, config, worktrees) = queued_app();
    queue_tab(&mut app);
    (app, rx, vec![repo, config, worktrees])
}

fn seed_routine(app: &mut App, every: &str) -> u64 {
    let at = time::now() + 3600;
    let place = Place { project: app.projects[0].path.clone(), ..Place::default() };
    let repeat = Repeat::parse(every, "UTC", at).expect("repeat");
    app.panels.queued.schedule(None, Task { repeat: Some(repeat), place, ..task(app, at, None) }).expect("routine")
}

fn routine(app: &App, id: u64) -> queue::Entry {
    app.panels.queued.queue().entry(id).expect("routine").clone()
}

fn labels(app: &mut App) -> Vec<&'static str> {
    app.sync_queued();
    let Some(Overlay::Issues(b)) = &app.overlay else { panic!("issues") };
    b.labels(issues_dialog::room(AREA).buttons)
}

#[test]
fn pause_and_resume_work_from_the_buttons_and_the_row_says_so() {
    let (mut app, _rx, _dirs) = opened();
    let id = seed_routine(&mut app, "2h");
    assert!(labels(&mut app).contains(&"pause"));
    press_button(&mut app, "pause");
    assert!(routine(&app, id).task.paused);
    assert!(app.messages.toast.as_ref().is_some_and(|toast| toast.message == format!("paused routine {id}")));
    let Some(Overlay::Issues(b)) = &app.overlay else { panic!("issues") };
    assert_eq!(b.queued[0].when, "paused");
    assert!(labels(&mut app).contains(&"resume"));
    press_button(&mut app, "resume");
    assert!(!routine(&app, id).task.paused);
    assert!(app.messages.toast.as_ref().is_some_and(|toast| toast.message.starts_with("resumed routine")));
}

#[test]
fn a_routines_row_shows_its_repeat_and_its_runs_follow_it() {
    let (mut app, _rx, _dirs) = opened();
    let id = seed_routine(&mut app, "day 09:00");
    let run = app.panels.queued.run_now(id, time::now()).expect("run");
    app.panels.queued.set_state(run, QueueState::Removed);
    app.sync_queued();
    let Some(Overlay::Issues(b)) = &app.overlay else { panic!("issues") };
    let routine = b.queued.iter().find(|queued| queued.id == id).expect("routine row");
    assert!(routine.meta.starts_with("↻ every day 09:00 · fake · "), "{}", routine.meta);
    let run = b.queued.iter().find(|queued| queued.id == run).expect("run row");
    assert!(run.meta.starts_with("removed · fake"), "{}", run.meta);
    assert_eq!(run.label, routine.label);
}

#[test]
fn start_now_adds_a_run_and_keeps_the_schedule() {
    let (mut app, _rx, _dirs) = opened();
    let id = seed_routine(&mut app, "2h");
    let at = routine(&app, id).task.at;
    press_button(&mut app, "start now");
    assert_eq!(runs(&app, id).len(), 1);
    assert_eq!(routine(&app, id).task.at, at);
}

#[test]
fn removing_a_routine_takes_its_runs_and_undo_brings_everything_back() {
    let (mut app, _rx, _dirs) = opened();
    let id = seed_routine(&mut app, "2h");
    app.panels.queued.run_now(id, time::now()).expect("run");
    let before = app.panels.queued.queue().entries.clone();
    send_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press_button(&mut app, "remove");
    assert_eq!(app.panels.queued.queue().entries, []);
    assert!(
        app.messages
            .toast
            .as_ref()
            .is_some_and(|toast| toast.message == format!("removed routine {id} and its 1 runs"))
    );
    let button = ui::toast_button(AREA, app.messages.toast.as_ref().expect("undo").view());
    click(&mut app, button.as_position());
    assert_eq!(app.panels.queued.queue().entries, before);
}

#[test]
fn enter_on_a_routine_opens_its_form_filled_and_saving_keeps_its_schedule() {
    let (mut app, _rx, _dirs) = opened();
    let id = seed_routine(&mut app, "day 09:00");
    let at = routine(&app, id).task.at;
    click(&mut app, queue_row(0));
    let Some(Overlay::Queue(form)) = &app.overlay else { panic!("form") };
    assert_eq!((form.kind(), form.repeat().as_deref(), form.id), (When::Daily, Some("day 09:00"), Some(id)));
    app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
    assert_eq!(routine(&app, id).task.at, at);
    assert!(app.messages.toast.as_ref().is_some_and(|toast| toast.message.starts_with("scheduled routine")));
}

#[test]
fn a_run_opens_no_form() {
    let (mut app, _rx, _dirs) = opened();
    let id = seed_routine(&mut app, "2h");
    let run = app.panels.queued.run_now(id, time::now()).expect("run");
    app.panels.queued.set_state(run, QueueState::Failed { reason: "gone".into() });
    app.sync_queued();
    let rows = app.panels.queued.queue().entries.iter().position(|entry| entry.id == run).expect("row");
    click(&mut app, queue_row(u16::try_from(rows).expect("row")));
    assert!(matches!(app.overlay, Some(Overlay::Issues(_))));
}

mod the_form {
    use super::*;

    fn choose_kind(app: &mut App, downs: usize) {
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        form.focus = Control::When;
        let key = |app: &mut App, code| app.queue_key(KeyEvent::new(code, KeyModifiers::NONE), AREA);
        key(app, KeyCode::Down);
        for _ in 0..downs {
            key(app, KeyCode::Down);
        }
        key(app, KeyCode::Enter);
    }

    #[test]
    fn saving_every_weekday_at_makes_a_routine_with_that_repeat() {
        let (mut app, _rx, _dirs) = opened();
        press_button(&mut app, "new");
        choose_kind(&mut app, 3);
        let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("form") };
        assert_eq!(form.kind(), When::Weekdays);
        form.prompt = Editor::new("review the open pull requests");
        form.clock = Editor::new("09:00");
        app.queue_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), AREA);
        let routine =
            app.panels.queued.queue().entries.iter().find(|entry| entry.is_routine()).expect("routine").clone();
        assert_eq!(routine.task.repeat.map(|repeat| repeat.to_string()).as_deref(), Some("weekday 09:00"));
        assert!(app.messages.toast.as_ref().is_some_and(|toast| toast.message.starts_with("scheduled routine")));
    }

    #[test]
    fn a_routine_lists_only_repository_roots_and_greys_the_checkbox_for_a_custom_agent() {
        let (mut app, _rx, _dirs) = opened();
        press_button(&mut app, "new");
        choose_kind(&mut app, 1);
        let Some(Overlay::Queue(form)) = &app.overlay else { panic!("form") };
        assert!(form.kind().repeats());
        assert!(form.place.options().iter().all(|label| label.ends_with("· root")), "{:?}", form.place.options());
        assert!(form.cleanup_off.as_deref().is_some_and(|why| why.contains("cannot tell when fake finishes")));
        assert!(!form.controls().contains(&Control::Cleanup));
    }
}
