use super::routine_panel::seed_routine;
use super::*;

fn queue_dialog(app: &App) -> &queue::dialog::Dialog {
    let Some(Overlay::QueueList(dialog)) = &app.overlay else { panic!("the queue modal") };
    dialog
}

fn selected(app: &App) -> Option<u64> {
    queue_dialog(app).selected().map(|row| row.id)
}

fn routine_with_runs(app: &mut App) -> (u64, u64, u64) {
    let id = seed_routine(app, "2h");
    let older = app.panels.queued.run_now(id, time::now() - 60).expect("a run");
    app.panels.queued.set_started(older, time::now() - 60);
    let newer = app.panels.queued.run_now(id, time::now()).expect("another run");
    app.panels.queued.set_started(newer, time::now());
    (id, older, newer)
}

#[test]
fn more_lists_queue_after_new_bot_and_opens_the_modal() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    let more = app_areas(&app).more.as_position();
    click(&mut app, more);
    assert_eq!(menu_labels(&app)[..2], ["new bot…", "queue"]);
    pick(&mut app, "queue");
    assert!(matches!(app.overlay, Some(Overlay::QueueList(_))));
}

#[test]
fn e_in_the_keys_menu_opens_it() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    app.config.prefix_key = "ctrl+]".into();
    send_key(&mut app, KeyCode::Char(']'), KeyModifiers::CONTROL);
    assert!(matches!(app.overlay, Some(Overlay::Keys(None))));
    send_key(&mut app, KeyCode::Char('e'), KeyModifiers::NONE);
    assert!(matches!(app.overlay, Some(Overlay::QueueList(_))));
}

#[test]
fn it_opens_with_no_project_open() {
    let (mut app, _rx) = empty_app();
    render(&mut app, AREA);
    from_more(&mut app, "queue");
    assert_eq!(queue_dialog(&app).shown().len(), 0);
    let drawn = format!("{}", render(&mut app, AREA).backend());
    assert!(drawn.contains("nothing queued: new schedules a prompt"), "{drawn}");
}

#[test]
fn a_routines_runs_follow_it_newest_first() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    let one_off = app.panels.queued.schedule(None, task(&app, time::now() + 600, None)).expect("an entry");
    let (id, older, newer) = routine_with_runs(&mut app);
    open_queue(&mut app);
    assert_eq!(shown_ids(&app), [one_off, id, newer, older]);
    let drawn = format!("{}", render(&mut app, AREA).backend());
    assert!(drawn.contains("▾ "), "{drawn}");
}

#[test]
fn a_click_on_the_mark_folds_the_runs_and_the_fold_outlives_the_modal() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    let (id, ..) = routine_with_runs(&mut app);
    open_queue(&mut app);
    click(&mut app, queue_mark(0));
    assert_eq!(shown_ids(&app), [id]);
    assert_eq!(queue_dialog(&app).shown()[0].hidden, 2);
    let drawn = format!("{}", render(&mut app, AREA).backend());
    assert!(drawn.contains("▸ ") && drawn.contains(" · 2 runs"), "{drawn}");
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    open_queue(&mut app);
    assert_eq!(shown_ids(&app), [id]);
    send_key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(shown_ids(&app).len(), 3);
}

#[test]
fn left_on_a_run_folds_its_routine_and_selects_it() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    let (id, older, _) = routine_with_runs(&mut app);
    app.open_queue(Some(older));
    send_key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!((shown_ids(&app), selected(&app)), (vec![id], Some(id)));
}

#[test]
fn the_selection_stays_on_its_entry_when_a_routine_fires() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    let (id, older, _) = routine_with_runs(&mut app);
    open_queue(&mut app);
    for _ in 0..3 {
        send_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(selected(&app), Some(older));
    let fired = app.panels.queued.run_now(id, time::now() + 60).expect("a new run");
    app.panels.queued.set_started(fired, time::now() + 60);
    app.sync_queued();
    assert_eq!(shown_ids(&app)[1], fired);
    assert_eq!(selected(&app), Some(older));
}

#[test]
fn a_save_that_the_filter_would_hide_clears_the_filter_and_selects_it() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    app.panels.queued.schedule(None, task(&app, time::now() + 600, None)).expect("an entry");
    open_queue(&mut app);
    app.handle_event(AppEvent::Input(Event::Paste("login".into())), AREA).expect("paste");
    press_button(&mut app, "new");
    let Some(Overlay::Queue(form)) = &mut app.overlay else { panic!("the form") };
    form.prompt = Editor::new("update the docs");
    save_queue_form(&mut app);
    let saved = app
        .panels
        .queued
        .queue()
        .entries
        .iter()
        .find(|entry| entry.task.prompt == "update the docs")
        .map(|entry| entry.id);
    assert_eq!((queue_dialog(&app).query(), selected(&app)), ("", saved));
}

#[test]
fn done_escape_and_a_click_outside_close_it() {
    let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
    open_queue(&mut app);
    press_button(&mut app, "done");
    assert!(app.overlay.is_none());
    open_queue(&mut app);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay.is_none());
    open_queue(&mut app);
    click(&mut app, Position::new(0, 0));
    assert!(app.overlay.is_none());
}

mod schedule_on_an_issue {
    use super::*;

    fn scheduling(app: &mut App) {
        app.open_issues(AREA).expect("open issues");
        let issue = issues::issue(Source::Github, 7, "Fix login");
        app.schedule_issue(&issue, "fake", None);
        assert!(matches!(app.overlay, Some(Overlay::Queue(_))));
    }

    #[test]
    fn cancelling_goes_back_to_the_issues() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        scheduling(&mut app);
        app.queue_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), AREA);
        assert!(matches!(&app.overlay, Some(Overlay::Issues(browser)) if !browser.scheduling));
    }

    #[test]
    fn saving_opens_the_queue_with_the_new_entry_selected() {
        let (mut app, _rx, _repo, _config, _worktrees) = queued_app();
        let earlier = app.panels.queued.schedule(None, task(&app, time::now() + 60, None)).expect("an entry");
        scheduling(&mut app);
        save_queue_form(&mut app);
        let saved = app.panels.queued.queue().entries.iter().map(|entry| entry.id).find(|id| *id != earlier);
        assert!(saved.is_some());
        assert_eq!(selected(&app), saved);
    }
}
