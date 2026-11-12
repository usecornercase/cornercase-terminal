use super::queue_results::{RESULT, STARTED, gone, keep, opened, shop};
use crossterm::event::Event;

use super::*;
use crate::agents::context::Pointer;
use crate::issues::browser::Tab as IssueTab;
use crate::queue::outcome::Taken;
use crate::queue::{Place, State as QueueState, Task};
use crate::test_util::TempDir;
use crate::ui::Overlay as View;

fn key(app: &mut App, code: KeyCode) {
    app.handle_event(AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))), AREA).expect("handle key");
}

fn open_row(app: &mut App) {
    app.sync_queued();
    let list = ui::issues_list(ui::issues_area(AREA));
    click(app, Position::new(list.x + 2, list.y));
}

fn await_conversation(app: &mut App, rx: &Receiver<AppEvent>) {
    loop {
        let event = rx.recv_timeout(Duration::from_secs(30)).expect("the conversation arrives");
        if matches!(event, AppEvent::QueueResult(_)) {
            app.handle_event(event, AREA).expect("handle the conversation");
            return;
        }
    }
}

fn click_entry(app: &mut App, id: u64) {
    app.sync_queued();
    let row = app.panels.queued.queue().entries.iter().position(|entry| entry.id == id).expect("row");
    let list = ui::issues_list(ui::issues_area(AREA));
    click(app, Position::new(list.x + 2, list.y + u16::try_from(row).expect("row")));
}

fn update(app: &App) -> ui::Update {
    let View::Update(update) = overlay_shown(app, AREA) else { panic!("an update dialog") };
    update
}

fn lines(app: &App) -> String {
    let text: Vec<String> = update(app)
        .notes
        .iter()
        .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
        .collect();
    text.join("\n")
}

fn back_on_the_queue_tab(app: &App) -> bool {
    matches!(&app.overlay, Some(Overlay::Issues(b)) if b.tabs.get(b.tab) == Some(&IssueTab::Queue))
}

#[test]
fn enter_on_an_entry_whose_pane_is_gone_opens_its_detail_and_esc_goes_back_to_the_queue_tab() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", None);

    open_row(&mut app);

    assert!(matches!(app.overlay, Some(Overlay::QueueResult(_))));
    key(&mut app, KeyCode::Esc);
    assert!(back_on_the_queue_tab(&app));
}

#[test]
fn a_click_outside_and_done_go_back_too() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", None);
    open_row(&mut app);

    click(&mut app, Position::new(0, 0));
    assert!(back_on_the_queue_tab(&app));

    open_row(&mut app);
    let [_, done] = ui::update_buttons(AREA, "copy", "done");
    click(&mut app, done.as_position());
    assert!(back_on_the_queue_tab(&app));
}

#[test]
fn a_failed_run_opens_its_detail_and_a_failed_one_off_entry_its_form() {
    let (mut app, _rx, _dirs) = opened();
    let one_off = app.panels.queued.schedule(None, shop()).expect("entry");
    app.panels.queued.set_state(one_off, QueueState::Failed { reason: "no room".into() });

    open_row(&mut app);

    assert!(matches!(app.overlay, Some(Overlay::Queue(_))));
}

fn long_text() -> String {
    (0..60).map(|n| format!("line {n:02}")).collect::<Vec<_>>().join("\n")
}

#[test]
fn the_wheel_and_the_arrows_scroll_a_long_result() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, &long_text(), None);
    open_row(&mut app);
    let inside = ui::update_notes(AREA).as_position();

    mouse(&mut app, MouseEventKind::ScrollDown, inside);
    let wheeled = update(&app).scroll;
    key(&mut app, KeyCode::Down);
    let arrowed = update(&app).scroll;
    for _ in 0..200 {
        key(&mut app, KeyCode::Down);
    }
    let furthest = update(&app).scroll;

    assert!(wheeled > 0 && arrowed > wheeled, "{wheeled} {arrowed}");
    let most = update(&app).notes.len() - usize::from(ui::update_notes(AREA).height);
    assert_eq!(furthest, most);
}

fn transcript(dir: &Path) -> Pointer {
    let reply = serde_json::json!({
        "type": "assistant", "isSidechain": false, "timestamp": "2026-11-12T09:41:00.000Z",
        "message": {"id": "m1", "model": "claude-opus-5-5", "role": "assistant",
                    "content": [{"type": "text", "text": "Reviewed #480 and #482."}]},
    });
    let prompt =
        serde_json::json!({"type": "user", "origin": {"kind": "human"}, "message": {"content": "review them"}});
    let path = dir.join("record.jsonl");
    std::fs::write(&path, format!("{prompt}\n{reply}\n")).expect("transcript");
    Pointer::Claude(path)
}

#[test]
fn conversation_reads_the_record_and_result_comes_back() {
    let (mut app, rx, dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480 and #482.", Some(transcript(dirs[0].path())));
    open_row(&mut app);
    assert_eq!(update(&app).extra, Some("conversation"));

    click(&mut app, ui::update_extra(AREA, "copy", "done", "conversation").as_position());
    assert!(matches!(update(&app).note, Some(ui::Note::Busy(_))));
    await_conversation(&mut app, &rx);

    let shown = lines(&app);
    assert!(shown.starts_with("prompt\nreview them\n\nreply · 2026-11-12 09:41\nReviewed #480 and #482."), "{shown}");
    assert_eq!(update(&app).extra, Some("result"));
    click(&mut app, ui::update_extra(AREA, "copy", "done", "result").as_position());
    assert!(lines(&app).contains("agent     claude"), "{}", lines(&app));
}

#[test]
fn a_gone_record_offers_no_conversation_and_says_why() {
    let (mut app, _rx, dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", Some(Pointer::Claude(dirs[0].path().join("missing.jsonl"))));

    open_row(&mut app);

    assert_eq!(update(&app).extra, None);
    assert!(lines(&app).contains("no longer has this conversation's record"), "{}", lines(&app));
}

#[test]
fn copy_copies_what_is_shown_and_keeps_the_detail_open() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    keep(&mut app, id, "Reviewed #480.", None);
    open_row(&mut app);
    let [copy, _] = ui::update_buttons(AREA, "copy", "done");

    click(&mut app, copy.as_position());

    let writes = app.take_host_writes();
    let expected = lines(&app).trim_end().to_string();
    assert_eq!(writes, [clipboard::osc52(&expected)]);
    assert!(expected.contains("Reviewed #480.") && expected.contains("branch"), "{expected}");
    assert!(matches!(app.overlay, Some(Overlay::QueueResult(_))));
}

#[test]
fn copy_on_a_run_with_no_message_copies_what_the_dialog_says() {
    let (mut app, _rx, _dirs) = opened();
    let routine = Task { repeat: crate::queue::repeat::Repeat::parse("2h", "UTC", STARTED).ok(), ..shop() };
    let routine = app.panels.queued.schedule(None, routine).expect("routine");
    let run = app.panels.queued.run_now(routine, STARTED).expect("run");
    app.panels.queued.set_state(run, QueueState::Failed { reason: "the folder is gone".into() });
    click_entry(&mut app, run);
    let [copy, _] = ui::update_buttons(AREA, "copy", "done");

    click(&mut app, copy.as_position());

    let writes = app.take_host_writes();
    assert_eq!(writes.len(), 1);
    assert!(lines(&app).contains("it did not start: the folder is gone"));
    assert_eq!(writes, [clipboard::osc52(lines(&app).trim_end())]);
}

#[test]
fn the_detail_closes_when_its_entry_goes_on_the_next_refresh() {
    let (mut app, _rx, _dirs) = opened();
    let id = gone(&mut app);
    open_row(&mut app);

    app.panels.queued.remove(id).expect("removed");
    app.refresh(Instant::now());

    assert!(matches!(app.overlay, Some(Overlay::Issues(_))));
}

#[test]
fn the_hint_says_what_enter_does() {
    let (mut app, _rx, _dirs) = opened();
    gone(&mut app);
    app.sync_queued();

    let shown = format!("{}", render(&mut app, AREA).backend());

    assert!(shown.contains("enter shows its result"), "{shown}");
}

mod pictures {
    use super::*;

    fn picture(app: &mut App) -> String {
        format!("{}", render(app, AREA).backend())
    }

    fn pictured() -> (App, Receiver<AppEvent>, [TempDir; 2]) {
        let (mut app, rx, dirs) = opened();
        app.set_queue_zone("UTC".into());
        (app, rx, dirs)
    }

    #[test]
    fn the_detail_of_a_removed_run() {
        let (mut app, _rx, _dirs) = pictured();
        let id = gone(&mut app);
        app.panels.queued.set_state(id, QueueState::Removed);
        keep(&mut app, id, "Reviewed #480 and #482. #482 needed a rebase; done.\nLeft: #479 waits on a reply.", None);
        open_row(&mut app);
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn the_detail_of_a_custom_agent_says_why_it_has_no_message() {
        let (mut app, _rx, _dirs) = pictured();
        let id = gone(&mut app);
        let taken = Taken { agent: Some("fake".into()), branch: Some("feat/x".into()), ..Taken::default() };
        app.panels.queued.keep_result(id, taken, None, RESULT);
        open_row(&mut app);
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn the_detail_of_a_failed_run() {
        let (mut app, _rx, _dirs) = pictured();
        let repeat = crate::queue::repeat::Repeat::parse("day 09:00", "UTC", STARTED).expect("repeat");
        let routine = app.panels.queued.schedule(None, Task { repeat: Some(repeat), ..shop() }).expect("routine");
        let run = app.panels.queued.run_now(routine, STARTED).expect("run");
        app.panels.queued.set_state(run, QueueState::Failed { reason: "the project folder is gone".into() });
        app.sync_queued();
        let row = app.panels.queued.queue().entries.iter().position(|entry| entry.id == run).expect("row");
        let list = ui::issues_list(ui::issues_area(AREA));
        click(&mut app, Position::new(list.x + 2, list.y + u16::try_from(row).expect("row")));
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn the_conversation_of_a_run() {
        let (mut app, rx, dirs) = pictured();
        let id = gone(&mut app);
        keep(&mut app, id, "Reviewed #480 and #482.", Some(transcript(dirs[0].path())));
        open_row(&mut app);
        click(&mut app, ui::update_extra(AREA, "copy", "done", "conversation").as_position());
        await_conversation(&mut app, &rx);
        insta::assert_snapshot!(picture(&mut app));
    }

    #[test]
    fn a_long_label_and_a_long_pull_request_title_keep_the_state_and_wrap_the_facts() {
        let (mut app, _rx, _dirs) = pictured();
        let place = Place {
            project: "/work/shop".into(),
            branch: Some("routine/review-20261112-0900".into()),
            ..Place::default()
        };
        let label = "Review every open pull request of the shop, comment on the ones that need a rebase and tell me which ones wait";
        let task = Task { label: label.into(), place, ..shop() };
        let id = app.panels.queued.schedule(None, task).expect("entry");
        app.panels.queued.started_at(id, 999, STARTED);
        let pull = crate::queue::outcome::Pull {
            number: 482,
            title: "Fix the login form so that a password with a trailing space is accepted again".into(),
            state: "open".into(),
            checks: Some("passing".into()),
            ..Default::default()
        };
        let taken = Taken { agent: Some("claude".into()), pull_request: Some(pull), ..Taken::default() };
        app.panels.queued.keep_result(id, taken, None, RESULT);
        open_row(&mut app);
        insta::assert_snapshot!(picture(&mut app));
    }
}
